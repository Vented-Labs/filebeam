use anyhow::{Context, Result, bail};
use filebeam_transfer_native::{
    checkpoint::{FilesystemSecretStore, SecretStore, Store},
    control::TransferSettings,
};
use reqwest::{Url, blocking::Client, header::HeaderValue};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

const RECORD: &str = "note-management.json";

#[derive(Clone)]
pub struct NoteManagementStore {
    root: PathBuf,
    secrets: Arc<dyn SecretStore>,
}
#[derive(Serialize, Deserialize)]
struct Capability {
    instance: String,
    delete_token: String,
    upload_token: Option<String>,
}
#[derive(Clone, Copy)]
pub enum NoteManagementAction {
    EndLive,
    Revoke,
}
#[derive(Clone, Copy)]
pub struct NoteManagementAvailability {
    pub can_end_live: bool,
    pub can_revoke: bool,
}

impl NoteManagementStore {
    pub fn for_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            secrets: FilesystemSecretStore::for_state_root(&root),
            root,
        }
    }
    pub fn for_settings(settings: &TransferSettings) -> Self {
        Self {
            root: settings.state_home.clone(),
            secrets: settings
                .checkpoint_secret_store
                .clone()
                .unwrap_or_else(|| FilesystemSecretStore::for_state_root(&settings.state_home)),
        }
    }
    pub fn save(
        &self,
        id: &str,
        instance: &Url,
        delete_token: &str,
        upload_token: Option<&str>,
    ) -> Result<()> {
        let store = Store::create_with_secret_store(&self.root, id, self.secrets.clone())?;
        store.save_named(
            RECORD,
            &Capability {
                instance: instance.origin().ascii_serialization(),
                delete_token: delete_token.into(),
                upload_token: upload_token.map(str::to_owned),
            },
        )
    }
    pub fn action(&self, id: &str, action: NoteManagementAction) -> Result<()> {
        let store = Store::open_with_secret_store(&self.root, id, self.secrets.clone())?;
        let capability: Capability = store
            .load_named(RECORD)?
            .context("note management capability is unavailable")?;
        let instance = Url::parse(&capability.instance).context("invalid saved note origin")?;
        let response = match action {
            NoteManagementAction::EndLive => {
                let token = capability
                    .upload_token
                    .context("this note is not a live share")?;
                Client::new()
                    .post(instance.join(&format!("api/v1/transfers/{id}/webrtc/end"))?)
                    .header("X-Filebeam-Upload-Token", HeaderValue::from_str(&token)?)
                    .json(&serde_json::json!({}))
                    .send()
                    .context("end live note")?
            }
            NoteManagementAction::Revoke => Client::new()
                .delete(instance.join(&format!("api/v1/transfers/{id}"))?)
                .header(
                    "X-Filebeam-Delete-Token",
                    HeaderValue::from_str(&capability.delete_token)?,
                )
                .send()
                .context("revoke note")?,
        };
        if !response.status().is_success() {
            bail!("note management returned {}", response.status());
        }
        // Only an acknowledged remote action consumes the local capability.
        store.remove_named(RECORD)
    }
    pub fn contains(&self, id: &str) -> bool {
        Store::open_with_secret_store(&self.root, id, self.secrets.clone())
            .and_then(|store| store.load_named::<Capability>(RECORD))
            .ok()
            .flatten()
            .is_some()
    }
    pub fn availability(&self, id: &str) -> NoteManagementAvailability {
        match Store::open_with_secret_store(&self.root, id, self.secrets.clone())
            .and_then(|store| store.load_named::<Capability>(RECORD))
        {
            Ok(Some(capability)) => NoteManagementAvailability {
                can_end_live: capability.upload_token.is_some(),
                can_revoke: true,
            },
            _ => NoteManagementAvailability {
                can_end_live: false,
                can_revoke: false,
            },
        }
    }
    pub fn acknowledge_end(&self, id: &str) -> Result<()> {
        Store::open_with_secret_store(&self.root, id, self.secrets.clone())?.remove_named(RECORD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn server(status: &str) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sent, received) = std::sync::mpsc::channel();
        let status = status.to_owned();
        thread::spawn(move || {
            let mut stream = listener.accept().unwrap().0;
            let mut request = [0; 4096];
            let count = stream.read(&mut request).unwrap();
            sent.send(String::from_utf8_lossy(&request[..count]).into_owned())
                .unwrap();
            stream
                .write_all(
                    format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                )
                .unwrap();
        });
        (format!("http://{address}"), received)
    }

    #[test]
    fn http_note_revoke_uses_the_private_capability_and_consumes_it_after_ack() {
        let root = std::env::temp_dir().join(format!(
            "filebeam-note-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (instance, request) = server("202 Accepted");
        let store = NoteManagementStore::for_root(&root);
        store
            .save(
                "note-id",
                &Url::parse(&instance).unwrap(),
                "delete-secret",
                None,
            )
            .unwrap();
        store
            .action("note-id", NoteManagementAction::Revoke)
            .unwrap();
        let request = request.recv().unwrap();
        assert!(request.starts_with("DELETE /api/v1/transfers/note-id HTTP/1.1"));
        assert!(
            request.contains("x-filebeam-delete-token: delete-secret")
                || request.contains("X-Filebeam-Delete-Token: delete-secret")
        );
        assert!(!store.contains("note-id"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn live_end_keeps_the_capability_when_the_server_does_not_acknowledge() {
        let root = std::env::temp_dir().join(format!(
            "filebeam-note-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (instance, request) = server("500 Internal Server Error");
        let store = NoteManagementStore::for_root(&root);
        store
            .save(
                "live-id",
                &Url::parse(&instance).unwrap(),
                "delete-secret",
                Some("upload-secret"),
            )
            .unwrap();
        assert!(
            store
                .action("live-id", NoteManagementAction::EndLive)
                .is_err()
        );
        let request = request.recv().unwrap();
        assert!(request.starts_with("POST /api/v1/transfers/live-id/webrtc/end HTTP/1.1"));
        assert!(
            request.contains("x-filebeam-upload-token: upload-secret")
                || request.contains("X-Filebeam-Upload-Token: upload-secret")
        );
        assert!(store.contains("live-id"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
