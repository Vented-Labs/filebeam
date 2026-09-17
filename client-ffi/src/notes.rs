use crate::services::NativeServices;
use crate::{Result, TransferJob, operation};
use filebeam_client_core::services as core;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

#[derive(Clone, Copy, uniffi::Enum)]
pub enum NoteTransport {
    Http,
    WebRtc,
}

#[derive(Clone, uniffi::Record)]
pub struct NoteRequest {
    pub text: String,
    pub title: Option<String>,
    pub language: String,
    pub password: Option<String>,
    pub burn_on_read: bool,
    pub retention_hours: Option<u64>,
    pub transport: NoteTransport,
}

#[derive(Clone, uniffi::Record)]
pub struct CreatedNote {
    pub id: String,
    pub link: String,
}

#[derive(Clone, uniffi::Record)]
pub struct OpenedNote {
    pub id: String,
    pub text: String,
    pub title: Option<String>,
    pub language: String,
    pub consumed: bool,
}

fn note_read_settings(
    runtime: Option<&Arc<crate::NativeRuntime>>,
    relay_only: bool,
) -> filebeam_client_core::control::TransferSettings {
    let mut settings = runtime.map(|runtime| runtime.settings.clone()).unwrap_or(
        filebeam_client_core::control::TransferSettings {
            state_home: PathBuf::from("/tmp/filebeam-notes"),
            max_concurrency: Some(1),
            memory_budget: 256 * 1024 * 1024,
            client_user_agent: None,
            webrtc_relay_only: relay_only,
            checkpoint_secret_store: None,
            source_resolver: None,
        },
    );
    // The request policy is authoritative, including when the process runtime was
    // created before the user committed a relay-only setting.
    settings.webrtc_relay_only = relay_only;
    settings
}

#[uniffi::export]
impl NativeServices {
    /// Passwords are passed only to this call and are never persisted by the native client.
    pub fn create_note(&self, request: NoteRequest) -> Result<CreatedNote> {
        if matches!(request.transport, NoteTransport::WebRtc) {
            return Err(crate::invalid("Use start_live_note for WebRTC notes"));
        }
        let _memory = self
            .runtime
            .as_ref()
            .map(|runtime| {
                runtime
                    .reserve_service_memory(filebeam_client_core::TRANSIENT_MEMORY_ALLOWANCE_BYTES)
            })
            .transpose()?;
        let note = self
            .inner
            .notes()
            .create(core::NoteCreate {
                text: request.text,
                title: request.title,
                language: request.language,
                password: request.password,
                burn_on_read: request.burn_on_read,
                retention_hours: request.retention_hours,
            })
            .map_err(operation)?;
        let management =
            filebeam_client_core::services::note_management::NoteManagementStore::for_settings(
                &self
                    .runtime
                    .as_ref()
                    .map(|runtime| runtime.settings.clone())
                    .unwrap_or(note_read_settings(None, false)),
            );
        self.inner
            .notes()
            .save_management(&note, &management)
            .map_err(operation)?;
        Ok(CreatedNote {
            id: note.id,
            link: note.link,
        })
    }

    pub fn start_live_note(&self, request: NoteRequest) -> Result<Arc<TransferJob>> {
        if !matches!(request.transport, NoteTransport::WebRtc) {
            return Err(crate::invalid("Live note jobs require WebRTC transport"));
        }
        let notes = self.inner.notes().clone();
        let request = core::NoteCreate {
            text: request.text,
            title: request.title,
            language: request.language,
            password: request.password,
            burn_on_read: request.burn_on_read,
            retention_hours: request.retention_hours,
        };
        let settings = filebeam_client_core::control::TransferSettings {
            state_home: PathBuf::from("/tmp/filebeam-notes"),
            max_concurrency: Some(1),
            memory_budget: 256 * 1024 * 1024,
            client_user_agent: None,
            webrtc_relay_only: false,
            checkpoint_secret_store: None,
            source_resolver: None,
        };
        let end_requested = Arc::new(AtomicBool::new(false));
        let ending = end_requested.clone();
        let management = self.runtime.as_ref().map(|runtime| {
            filebeam_client_core::services::note_management::NoteManagementStore::for_settings(
                &runtime.settings,
            )
        });
        let job = if let Some(runtime) = &self.runtime {
            filebeam_client_core::Job::spawn_in(
                &runtime.scheduler,
                runtime.settings.clone(),
                None,
                move |control| {
                    notes
                        .create_live(request, control, ending, management)
                        .map(|note| vec![note.link])
                },
            )
        } else {
            // Compatibility constructor callers retain the historical isolated pool.
            filebeam_client_core::Job::spawn(settings, move |control| {
                notes
                    .create_live(request, control, ending, None)
                    .map(|note| vec![note.link])
            })
        };
        Ok(Arc::new(TransferJob::new_live(job, end_requested)))
    }

    /// Opens, authenticates, and decodes a note before burning it when required.
    pub fn open_note(&self, link: String, password: Option<String>) -> Result<OpenedNote> {
        let _memory = self
            .runtime
            .as_ref()
            .map(|runtime| {
                runtime
                    .reserve_service_memory(filebeam_client_core::TRANSIENT_MEMORY_ALLOWANCE_BYTES)
            })
            .transpose()?;
        self.inner
            .notes()
            .open(&link, password.as_deref())
            .map(|note| OpenedNote {
                id: note.id,
                text: note.text,
                title: note.title,
                language: note.language,
                consumed: note.consumed,
            })
            .map_err(operation)
    }

    /// Starts an owned read operation. Direct peer connections require an explicit
    /// consent response; relay-only reads never request address exposure.
    pub fn open_note_job(
        &self,
        link: String,
        password: Option<String>,
        relay_only: bool,
    ) -> Result<Arc<TransferJob>> {
        let notes = self.inner.notes().clone();
        let result = Arc::new(Mutex::new(None));
        let output = result.clone();
        let settings = note_read_settings(self.runtime.as_ref(), relay_only);
        let run = move |control: &filebeam_client_core::control::Control| {
            let opened = notes
                .open_controlled(&link, password.as_deref(), Some(control))
                .map(|note| OpenedNote {
                    id: note.id,
                    text: note.text,
                    title: note.title,
                    language: note.language,
                    consumed: note.consumed,
                })
                .map_err(|error| error.to_string());
            *output.lock().unwrap_or_else(|e| e.into_inner()) = Some(opened);
            Ok(Vec::new())
        };
        let job = if let Some(runtime) = &self.runtime {
            filebeam_client_core::Job::spawn_in(&runtime.scheduler, settings, None, run)
        } else {
            filebeam_client_core::Job::spawn(settings, run)
        };
        Ok(Arc::new(TransferJob::new_note_read(job, result)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_job_request_policy_overrides_a_stale_runtime_default() {
        let runtime = Arc::new(
            crate::NativeRuntime::new(crate::ClientConfig {
                state_directory: "/tmp/filebeam-note-test".into(),
                memory_budget_mib: 64,
                max_concurrency: 1,
                relay_only: false,
                allow_http: true,
            })
            .unwrap(),
        );
        assert!(note_read_settings(Some(&runtime), true).webrtc_relay_only);
        assert!(!note_read_settings(None, false).webrtc_relay_only);
    }
}
