use crate::services::NativeServices;
use crate::{Result, TransferJob, operation};
use filebeam_client_core::services as core;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
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
    pub delete_token: String,
}

#[derive(Clone, uniffi::Record)]
pub struct OpenedNote {
    pub id: String,
    pub text: String,
    pub title: Option<String>,
    pub language: String,
    pub consumed: bool,
}

#[uniffi::export]
impl NativeServices {
    /// Passwords are passed only to this call and are never persisted by the native client.
    pub fn create_note(&self, request: NoteRequest) -> Result<CreatedNote> {
        if matches!(request.transport, NoteTransport::WebRtc) {
            return Err(crate::invalid("Use start_live_note for WebRTC notes"));
        }
        self.inner
            .notes()
            .create(core::NoteCreate {
                text: request.text,
                title: request.title,
                language: request.language,
                password: request.password,
                burn_on_read: request.burn_on_read,
                retention_hours: request.retention_hours,
            })
            .map(|note| CreatedNote {
                id: note.id,
                link: note.link,
                delete_token: note.delete_token,
            })
            .map_err(operation)
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
        let job = if let Some(runtime) = &self.runtime {
            filebeam_client_core::Job::spawn_in(
                &runtime.scheduler,
                runtime.settings.clone(),
                None,
                move |control| {
                    notes
                        .create_live(request, control, ending)
                        .map(|note| vec![note.link])
                },
            )
        } else {
            // Compatibility constructor callers retain the historical isolated pool.
            filebeam_client_core::Job::spawn(settings, move |control| {
                notes
                    .create_live(request, control, ending)
                    .map(|note| vec![note.link])
            })
        };
        Ok(Arc::new(TransferJob::new_live(job, end_requested)))
    }

    /// Opens, authenticates, and decodes a note before burning it when required.
    pub fn open_note(&self, link: String, password: Option<String>) -> Result<OpenedNote> {
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
}
