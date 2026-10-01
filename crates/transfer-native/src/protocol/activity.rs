use std::{path::Path, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use reqwest::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use super::upload;

const RESPONSE_LIMIT: usize = 64 * 1024;
const SESSION_LIMIT: usize = 64;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug)]
pub struct SenderActivity {
    pub records: Vec<SenderActivityRecord>,
    pub unavailable: bool,
    pub retry: bool,
}

#[derive(Clone, Debug)]
pub struct SenderActivityRecord {
    pub id: String,
    pub number: Option<u64>,
    pub progress: Option<f64>,
    pub state: String,
    pub selection_count: Option<u64>,
    pub all_files: Option<bool>,
}

#[derive(Deserialize)]
struct Api<T> {
    data: T,
}

#[derive(Deserialize)]
struct MonitorPage {
    sessions: Vec<MonitorSession>,
}

#[derive(Deserialize)]
struct MonitorSession {
    id: String,
    number: u64,
    progress: f64,
    status: String,
    selection_count: u64,
    all_files: bool,
}

/// Reads the authenticated checkpoint and returns only sender activity suitable
/// for presentation. This intentionally has no path for returning credentials.
pub fn sender_activity_with_secret_store(
    home: &Path,
    id: &str,
    secrets: Arc<dyn crate::checkpoint::SecretStore>,
) -> Result<SenderActivity> {
    let id = Uuid::parse_str(id)
        .context("invalid saved transfer id")?
        .to_string();
    let store = crate::checkpoint::Store::open_with_secret_store(home, &id, secrets)?;
    let Some(job) = upload::activity(&store)? else {
        return Ok(empty());
    };
    if job.driver == "webrtc" {
        return crate::runtime::shared_tokio_runtime().block_on(webrtc(job));
    }
    if job.driver == "http" && job.turbo {
        return crate::runtime::shared_tokio_runtime().block_on(turbo(job));
    }
    // Ordinary HTTP transfers have no receiver activity endpoint.
    Ok(empty())
}

fn empty() -> SenderActivity {
    SenderActivity {
        records: Vec::new(),
        unavailable: false,
        retry: false,
    }
}

async fn webrtc(job: upload::UploadActivity) -> Result<SenderActivity> {
    let signaling = crate::webrtc::Signaling::new(
        crate::http::client(REQUEST_TIMEOUT)?,
        &job.instance,
        &job.transfer_id,
    )?;
    match signaling.sender_sessions(&job.upload_token).await {
        Ok((sessions, _)) => {
            if sessions.len() > SESSION_LIMIT {
                return Ok(retry());
            }
            let mut records = Vec::with_capacity(sessions.len());
            for session in sessions {
                if !matches!(
                    session.status.as_str(),
                    "active" | "completed" | "cancelled" | "failed"
                ) {
                    return Ok(retry());
                }
                records.push(SenderActivityRecord {
                    id: session.id,
                    number: None,
                    progress: Some(f64::from(session.progress)),
                    state: session.status,
                    selection_count: None,
                    all_files: None,
                });
            }
            Ok(SenderActivity {
                records,
                unavailable: false,
                retry: false,
            })
        }
        Err(_) => Ok(retry()),
    }
}

async fn turbo(job: upload::UploadActivity) -> Result<SenderActivity> {
    let Some(token) = job.monitor_token else {
        // Older checkpoints predate monitor-token persistence.
        return Ok(SenderActivity {
            records: Vec::new(),
            unavailable: true,
            retry: false,
        });
    };
    let endpoint = format!(
        "{}/api/v1/transfers/{}/monitor",
        job.instance, job.transfer_id
    );
    let client = crate::http::client(REQUEST_TIMEOUT)?;
    let response = match client
        .get(endpoint)
        .header("X-Filebeam-Monitor-Token", token)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return Ok(retry()),
    };
    if matches!(
        response.status(),
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ) {
        return Ok(SenderActivity {
            records: Vec::new(),
            unavailable: true,
            retry: false,
        });
    }
    if !response.status().is_success() {
        return Ok(retry());
    }
    let body = match crate::http::read_limited(
        response,
        crate::http::ResponseLimits {
            headers_timeout: REQUEST_TIMEOUT,
            body_idle_timeout: REQUEST_TIMEOUT,
            maximum_bytes: RESPONSE_LIMIT,
        },
        std::future::pending(),
    )
    .await
    {
        Ok(body) => body,
        Err(_) => return Ok(retry()),
    };
    let page: Api<MonitorPage> = match serde_json::from_slice(&body) {
        Ok(page) => page,
        Err(_) => return Ok(retry()),
    };
    if page.data.sessions.len() > SESSION_LIMIT {
        return Ok(retry());
    }
    let mut records = Vec::with_capacity(page.data.sessions.len());
    for session in page.data.sessions {
        if !session.progress.is_finite()
            || !(0.0..=100.0).contains(&session.progress)
            || !matches!(
                session.status.as_str(),
                "downloading"
                    | "waiting"
                    | "verifying"
                    | "completed"
                    | "cancelled"
                    | "error"
                    | "stale"
            )
        {
            return Ok(retry());
        }
        records.push(SenderActivityRecord {
            id: session.id,
            number: Some(session.number),
            progress: Some(session.progress),
            state: session.status,
            selection_count: Some(session.selection_count),
            all_files: Some(session.all_files),
        });
    }
    Ok(SenderActivity {
        records,
        unavailable: false,
        retry: false,
    })
}

fn retry() -> SenderActivity {
    SenderActivity {
        records: Vec::new(),
        unavailable: true,
        retry: true,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    fn server(response: String, token: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let length = socket.read(&mut request).unwrap();
            let request = std::str::from_utf8(&request[..length]).unwrap();
            assert!(request.starts_with("GET /api/v1/transfers/transfer/monitor HTTP/1.1"));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains(&format!("x-filebeam-monitor-token: {token}\r\n"))
            );
            assert!(!request.contains(token) || request.matches(token).count() == 1);
            socket.write_all(response.as_bytes()).unwrap();
        });
        format!("http://{address}")
    }

    fn checkpoint(instance: String) -> (tempfile::TempDir, String) {
        let home = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4().to_string();
        let store = crate::checkpoint::Store::create(home.path(), &id).unwrap();
        store
            .save(&serde_json::json!({
                "version": 1, "id": id, "direction": "upload", "state": "sending",
                "instance": instance, "done": 0, "total": 1, "transfer_id": "transfer",
                "upload_token": "upload-secret", "monitor_token": "monitor-secret",
                "share_url": "/transfer", "delete_token": "delete-secret", "share_key": [],
                "password_salt": null, "chunk_bytes": 1, "server_concurrency": 1,
                "upload_status": false, "turbo": true, "descriptor_published": true,
                "encrypted_descriptor": null, "transport": null, "driver": "http",
                "join_token": null, "exact_manifest": null, "receipt": null, "recipient": null,
                "items": [], "chunks": []
            }))
            .unwrap();
        (home, id)
    }

    fn poll(home: &Path, id: &str) -> SenderActivity {
        sender_activity_with_secret_store(
            home,
            id,
            crate::checkpoint::FilesystemSecretStore::for_state_root(home),
        )
        .unwrap()
    }

    #[test]
    fn turbo_monitor_maps_only_valid_public_session_fields() {
        let body = r#"{"data":{"sessions":[{"id":"session","number":2,"progress":0.5,"status":"downloading","selection_count":3,"all_files":false}]}}"#;
        let instance = server(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
            "monitor-secret",
        );
        let (home, id) = checkpoint(instance);
        let activity = poll(home.path(), &id);
        assert!(!activity.unavailable);
        assert_eq!(activity.records.len(), 1);
        assert_eq!(activity.records[0].progress, Some(0.5));
        assert_eq!(activity.records[0].selection_count, Some(3));
    }

    #[test]
    fn turbo_monitor_authentication_is_not_retryable() {
        let instance = server(
            "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            "monitor-secret",
        );
        let (home, id) = checkpoint(instance);
        let activity = poll(home.path(), &id);
        assert!(activity.unavailable);
        assert!(!activity.retry);
    }

    #[test]
    fn malformed_or_excessive_monitor_responses_request_retry() {
        let malformed = "{\"data\":{\"sessions\":[{\"id\":\"x\",\"number\":1,\"progress\":101,\"status\":\"downloading\",\"selection_count\":1,\"all_files\":true}]}}";
        let instance = server(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{malformed}",
                malformed.len()
            ),
            "monitor-secret",
        );
        let (home, id) = checkpoint(instance);
        assert!(poll(home.path(), &id).retry);

        let instance = server(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                RESPONSE_LIMIT + 1
            ),
            "monitor-secret",
        );
        let (home, id) = checkpoint(instance);
        assert!(poll(home.path(), &id).retry);
    }
}
