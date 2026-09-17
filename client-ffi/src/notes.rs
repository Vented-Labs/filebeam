use crate::services::NativeServices;
use crate::{Result, TransferJob, TransferSnapshot, operation};
use filebeam_client_core::services as core;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
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
    let mut settings = runtime
        .map(|runtime| runtime.settings.clone())
        .unwrap_or_else(note_settings);
    // The request policy is authoritative, including when the process runtime was
    // created before the user committed a relay-only setting.
    settings.webrtc_relay_only = relay_only;
    settings
}

/// Inputs for a cancellable note receive. A burn-on-read note cannot begin a
/// WebRTC connection unless `burn_acknowledged` is true.
#[derive(Clone, uniffi::Record)]
pub struct NoteReceiveRequest {
    pub link: String,
    pub password: Option<String>,
    pub burn_acknowledged: bool,
}

#[derive(Clone, uniffi::Record)]
pub struct NoteInspection {
    pub id: String,
    pub status: String,
    pub burn_on_read: bool,
    pub transport: NoteTransport,
    pub password_required: bool,
}

/// A receive job keeps verified note text and a failed burn capability only in
/// process memory. `snapshot` never exposes either value.
#[derive(uniffi::Object)]
pub struct ManagedNoteReceive {
    job: Arc<TransferJob>,
    received: Arc<Mutex<Option<core::ReceivedNote>>>,
    delivered: Mutex<bool>,
    notes: core::NotesService,
    cancel_requested: Arc<AtomicBool>,
}

/// HTTP note creation with an in-memory receipt. Job snapshots never contain
/// the delete capability or link key.
#[derive(uniffi::Object)]
pub struct ManagedNoteCreate {
    job: Arc<TransferJob>,
    receipt: Arc<Mutex<Option<CreatedNote>>>,
    delivered: Mutex<bool>,
}

#[uniffi::export]
impl ManagedNoteCreate {
    pub fn snapshot(&self) -> TransferSnapshot {
        self.job.snapshot()
    }

    pub fn take_receipt(&self) -> Option<CreatedNote> {
        if !matches!(self.job.snapshot().state, crate::JobState::Complete) {
            return None;
        }
        let mut delivered = self.delivered.lock().unwrap_or_else(|e| e.into_inner());
        if *delivered {
            return None;
        }
        let receipt = self
            .receipt
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if receipt.is_some() {
            *delivered = true;
        }
        receipt
    }
}

#[uniffi::export]
impl ManagedNoteReceive {
    pub fn snapshot(&self) -> TransferSnapshot {
        let mut snapshot = self.job.snapshot();
        if self.cancel_requested.load(Ordering::Relaxed) {
            // Note receives have no checkpoint or resume operation. Present the
            // terminal cancellation rather than the generic job's paused state.
            snapshot.state = crate::JobState::Failed;
            snapshot.error = Some("Cancelled".into());
            snapshot.error_category = Some(crate::ErrorCategory::Cancelled);
            snapshot.can_pause = false;
        }
        snapshot
    }

    pub fn respond(&self, prompt_id: u64, value: String) -> Result<()> {
        self.job.respond(prompt_id, value)
    }

    pub fn respond_consent(&self, prompt_id: u64, allowed: bool) -> Result<()> {
        self.job.respond_consent(prompt_id, allowed)
    }

    /// Cancels the managed receive. Cancellation is checked between every
    /// authenticated chunk and while awaiting peer consent.
    pub fn cancel(&self) {
        self.cancel_requested.store(true, Ordering::Relaxed);
        self.job.pause();
    }

    /// Returns verified plaintext once, after the worker reaches completion.
    pub fn take_note(&self) -> Option<OpenedNote> {
        if !matches!(self.job.snapshot().state, crate::JobState::Complete) {
            return None;
        }
        let mut delivered = self.delivered.lock().unwrap_or_else(|e| e.into_inner());
        if *delivered {
            return None;
        }
        let received = self.received.lock().unwrap_or_else(|e| e.into_inner());
        let result = received.as_ref()?;
        *delivered = true;
        Some(OpenedNote {
            id: result.note.id.clone(),
            text: result.note.text.clone(),
            title: result.note.title.clone(),
            language: result.note.language.clone(),
            consumed: result.note.consumed,
        })
    }

    /// Retries a burn removal after a receive completed with `consumed=false`.
    /// The opaque read capability is never returned across FFI.
    pub fn can_retry_burn(&self) -> bool {
        self.received
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .is_some_and(|received| received.pending_burn.is_some())
    }

    pub fn retry_burn(&self) -> Result<bool> {
        let mut received = self.received.lock().unwrap_or_else(|e| e.into_inner());
        let Some(result) = received.as_mut() else {
            return Ok(false);
        };
        let Some(pending) = result.pending_burn.as_ref() else {
            return Ok(result.note.consumed);
        };
        self.notes.retry_burn(pending).map_err(operation)?;
        result.pending_burn = None;
        result.note.consumed = true;
        Ok(true)
    }
}

#[uniffi::export]
impl NativeServices {
    /// Starts an HTTP note creation job. This is intentionally not pausable:
    /// the protocol has no resumable note-upload checkpoint without persisting secrets.
    pub fn start_create_note(&self, request: NoteRequest) -> Result<Arc<ManagedNoteCreate>> {
        if matches!(request.transport, NoteTransport::WebRtc) {
            return Err(crate::invalid("Use start_live_note for WebRTC notes"));
        }
        let notes = self.inner.notes().clone();
        let receipt = Arc::new(Mutex::new(None));
        let worker_receipt = receipt.clone();
        let management = self.runtime.as_ref().map(|runtime| {
            filebeam_client_core::services::note_management::NoteManagementStore::for_settings(
                &runtime.settings,
            )
        });
        let request = core::NoteCreate {
            text: request.text,
            title: request.title,
            language: request.language,
            password: request.password,
            burn_on_read: request.burn_on_read,
            retention_hours: request.retention_hours,
        };
        let run = move |control: &filebeam_client_core::control::Control| {
            let note = notes.create_with_control(request, control)?;
            if let Some(management) = &management {
                notes.save_management(&note, management)?;
            }
            *worker_receipt.lock().unwrap_or_else(|e| e.into_inner()) = Some(CreatedNote {
                id: note.id,
                link: note.link,
            });
            Ok(Vec::new())
        };
        let job = if let Some(runtime) = &self.runtime {
            filebeam_client_core::Job::spawn_in(
                &runtime.scheduler,
                runtime.settings.clone(),
                None,
                run,
            )
        } else {
            filebeam_client_core::Job::spawn(note_settings(), run)
        };
        Ok(Arc::new(ManagedNoteCreate {
            job: Arc::new(TransferJob::new(job)),
            receipt,
            delivered: Mutex::new(false),
        }))
    }

    /// Revokes a note with the creator capability retained by the caller.
    pub fn revoke_note(&self, transfer_id: String, delete_token: String) -> Result<()> {
        self.inner
            .notes()
            .revoke(&transfer_id, &delete_token)
            .map_err(operation)
    }
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
        if let Some(runtime) = &self.runtime {
            let management =
                filebeam_client_core::services::note_management::NoteManagementStore::for_settings(
                    &runtime.settings,
                );
            self.inner
                .notes()
                .save_management(&note, &management)
                .map_err(operation)?;
        }
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
        let settings = note_settings();
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

    /// Safe pre-open metadata inspection. This does not fetch ciphertext,
    /// signal WebRTC, or consume a burn-on-read note.
    pub fn inspect_note(&self, link: String) -> Result<NoteInspection> {
        self.inner
            .notes()
            .inspect(&link)
            .map(|note| NoteInspection {
                id: note.id,
                status: note.status,
                burn_on_read: note.burn_on_read,
                transport: match note.transport {
                    core::NoteTransport::Http => NoteTransport::Http,
                    core::NoteTransport::WebRtc => NoteTransport::WebRtc,
                },
                password_required: note.password_required,
            })
            .map_err(operation)
    }

    /// Starts a managed receive using the native runtime when supplied. Text
    /// and burn capabilities are intentionally absent from job snapshots.
    pub fn start_receive_note(
        &self,
        request: NoteReceiveRequest,
    ) -> Result<Arc<ManagedNoteReceive>> {
        let notes = self.inner.notes().clone();
        let worker_notes = notes.clone();
        let received = Arc::new(Mutex::new(None));
        let worker_received = received.clone();
        let cancel_requested = Arc::new(AtomicBool::new(false));
        let run = move |control: &filebeam_client_core::control::Control| {
            let result = worker_notes.receive(
                &request.link,
                request.password.as_deref(),
                core::NoteReceiveOptions {
                    burn_acknowledged: request.burn_acknowledged,
                    control: Some(control),
                },
            )?;
            *worker_received.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
            Ok(Vec::new())
        };
        let job = if let Some(runtime) = &self.runtime {
            filebeam_client_core::Job::spawn_in(
                &runtime.scheduler,
                runtime.settings.clone(),
                None,
                run,
            )
        } else {
            // Compatibility construction does not require a filesystem state home.
            filebeam_client_core::Job::spawn(note_settings(), run)
        };
        Ok(Arc::new(ManagedNoteReceive {
            job: Arc::new(TransferJob::new(job)),
            received,
            delivered: Mutex::new(false),
            notes,
            cancel_requested,
        }))
    }
}

fn note_settings() -> filebeam_client_core::control::TransferSettings {
    filebeam_client_core::control::TransferSettings {
        state_home: std::env::temp_dir().join("filebeam-notes"),
        max_concurrency: Some(1),
        memory_budget: 256 * 1024 * 1024,
        client_user_agent: None,
        webrtc_relay_only: false,
        checkpoint_secret_store: None,
        source_resolver: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_job_request_policy_overrides_a_stale_runtime_default() {
        let runtime = Arc::new(
            crate::NativeRuntime::new(crate::ClientConfig {
                state_directory: std::env::temp_dir()
                    .join("filebeam-note-test")
                    .display()
                    .to_string(),
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
