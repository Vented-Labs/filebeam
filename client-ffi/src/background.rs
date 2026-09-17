use crate::{
    BackgroundCapabilities, BackgroundHeader, BackgroundStatus, BackgroundWork, DownloadItem,
    Result, TransferClient, TransferJob, UploadOptions, UploadSource, invalid, operation,
};
use filebeam_client_core::{self as core, protocol};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(uniffi::Object)]
pub struct BackgroundTransfer {
    settings: core::control::TransferSettings,
    scheduler: core::Scheduler,
    allow_http: bool,
}

impl BackgroundTransfer {
    pub(crate) fn from_client(client: &TransferClient) -> Self {
        Self {
            settings: client.settings.clone(),
            scheduler: client.scheduler.clone(),
            allow_http: client.allow_http,
        }
    }

    fn secrets(&self) -> Arc<dyn filebeam_transfer_native::checkpoint::SecretStore> {
        self.settings
            .checkpoint_secret_store
            .clone()
            .unwrap_or_else(|| {
                filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(
                    &self.settings.state_home,
                )
            })
    }
}

#[uniffi::export]
impl BackgroundTransfer {
    pub fn capabilities(&self) -> BackgroundCapabilities {
        let value = protocol::background::capabilities();
        BackgroundCapabilities {
            upload: value.upload,
            download: value.download,
            turbo: value.turbo,
            detail: value.detail,
        }
    }

    pub fn status(&self, transfer_id: String) -> Result<BackgroundStatus> {
        let status =
            protocol::background::status(&self.settings.state_home, &transfer_id, self.secrets())
                .map_err(operation)?;
        Ok(BackgroundStatus {
            state: status.state,
            needs_execution: status.needs_execution,
            awaiting_unlock: status.awaiting_unlock,
            done: status.done,
            total: status.total,
            direction: status.direction,
            kind: status.kind,
            transport: status.transport,
            remote_id: status.remote_id,
        })
    }

    /// Performs foreground reservation and encryption, then returns a job whose
    /// result is the durable operation id. URLSession work starts only after
    /// `pending_upload_work` exposes descriptors for that id.
    pub fn prepare_upload(
        &self,
        instance: String,
        paths: Vec<String>,
        options: UploadOptions,
    ) -> Result<Arc<TransferJob>> {
        if paths.is_empty() || paths.iter().any(|path| !PathBuf::from(path).is_absolute()) {
            return Err(invalid("Select at least one absolute local file path"));
        }
        let instance = crate::client::origin(&instance, self.allow_http)?;
        let mode = if options.archive {
            filebeam_transfer_native::uploads::DirectoryMode::Zip
        } else {
            filebeam_transfer_native::uploads::DirectoryMode::Individual
        };
        let options = crate::client::upload_options(options)?;
        let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
        let settings = self.settings.clone();
        let job = core::Job::spawn_in(&self.scheduler, settings, None, move |control| {
            protocol::background::prepare_upload(&instance, &paths, mode, options, control)
                .map(|id| vec![id])
        });
        Ok(Arc::new(TransferJob::new_upload(job, "http")))
    }

    /// The source names and bounds are snapshotted before the existing HTTP
    /// artifact preparation path encrypts them for URLSession.
    pub fn prepare_upload_sources(
        &self,
        instance: String,
        sources: Vec<UploadSource>,
        options: UploadOptions,
    ) -> Result<Arc<TransferJob>> {
        if sources.is_empty() {
            return Err(invalid("Select at least one upload source"));
        }
        let instance = crate::client::origin(&instance, self.allow_http)?;
        let mode = if options.archive {
            filebeam_transfer_native::uploads::DirectoryMode::Zip
        } else {
            filebeam_transfer_native::uploads::DirectoryMode::Individual
        };
        let sources = sources
            .into_iter()
            .map(crate::client::source_spec)
            .collect::<Result<Vec<_>>>()?;
        let options = crate::client::upload_options(options)?;
        let settings = self.settings.clone();
        let job = core::Job::spawn_in(&self.scheduler, settings, None, move |control| {
            protocol::background::prepare_upload_sources(
                &instance, &sources, mode, options, control,
            )
            .map(|id| vec![id])
        });
        Ok(Arc::new(TransferJob::new_upload(job, "http")))
    }

    pub fn pending_upload_work(&self, transfer_id: String) -> Result<Vec<BackgroundWork>> {
        protocol::background::pending_upload_work(
            &self.settings.state_home,
            &transfer_id,
            self.secrets(),
        )
        .map_err(operation)
        .map(|work| {
            work.into_iter()
                .map(|work| BackgroundWork {
                    operation_id: work.operation_id,
                    transfer_id: work.transfer_id,
                    method: work.method,
                    url: work.url,
                    headers: work
                        .headers
                        .into_iter()
                        .map(|header| BackgroundHeader {
                            name: header.name,
                            value: header.value,
                        })
                        .collect(),
                    body_path: work.body_path,
                    expected_response_bytes: work.expected_response_bytes,
                })
                .collect()
        })
    }

    /// Performs foreground link metadata and manifest authentication. Its job
    /// result is the checkpoint ID; URLSession descriptors remain HTTP-only,
    /// while a WebRTC checkpoint waits for explicit foreground resume.
    pub fn prepare_download(
        &self,
        instance: String,
        link: String,
        output_directory: String,
    ) -> Result<Arc<TransferJob>> {
        if !PathBuf::from(&output_directory).is_absolute() {
            return Err(invalid("The output directory must be absolute"));
        }
        let instance = crate::client::origin(&instance, self.allow_http)?;
        let settings = self.settings.clone();
        Ok(Arc::new(TransferJob::new(core::Job::spawn_in(
            &self.scheduler,
            settings,
            None,
            move |control| {
                protocol::background::prepare_download(
                    &instance,
                    &link,
                    Path::new(&output_directory),
                    control,
                )
                .map(|id| vec![id])
            },
        ))))
    }

    pub fn pending_download_work(&self, transfer_id: String) -> Result<Vec<BackgroundWork>> {
        protocol::background::pending_download_work(
            &self.settings.state_home,
            &transfer_id,
            None,
            self.secrets(),
        )
        .map_err(operation)
        .map(map_work)
    }

    /// Lists local authenticated manifest metadata after preparation. It does
    /// not perform network I/O and does not start URLSession work.
    pub fn download_items(&self, checkpoint_id: String) -> Result<Vec<DownloadItem>> {
        protocol::background::download_items(
            &self.settings.state_home,
            &checkpoint_id,
            self.secrets(),
        )
        .map_err(operation)
        .map(|items| {
            items
                .into_iter()
                .map(|item| DownloadItem {
                    id: item.id,
                    name: item.name,
                    size: item.size,
                })
                .collect()
        })
    }

    /// Selects the exact non-empty set of manifest item IDs to download.
    /// Selection is durable and becomes immutable when work is first requested.
    pub fn select_download_items(
        &self,
        checkpoint_id: String,
        item_ids: Vec<String>,
    ) -> Result<()> {
        protocol::background::select_download_items(
            &self.settings.state_home,
            &checkpoint_id,
            &item_ids,
            self.secrets(),
        )
        .map_err(operation)
    }

    /// Inbox credentials are deliberately request-scoped. They are required
    /// again whenever descriptors are reconstructed after an app relaunch.
    pub fn pending_inbox_download_work(
        &self,
        transfer_id: String,
        cookie_context: String,
    ) -> Result<Vec<BackgroundWork>> {
        protocol::background::pending_download_work(
            &self.settings.state_home,
            &transfer_id,
            Some(&cookie_context),
            self.secrets(),
        )
        .map_err(operation)
        .map(map_work)
    }

    pub fn prepare_inbox_download(
        &self,
        instance: String,
        transfer_id: String,
        working_key: Vec<u8>,
        cookie_context: String,
        output_directory: String,
    ) -> Result<Arc<TransferJob>> {
        if working_key.len() != 32
            || cookie_context.is_empty()
            || cookie_context.contains(['\r', '\n'])
        {
            return Err(invalid(
                "A valid account session and inbox key are required",
            ));
        }
        if !PathBuf::from(&output_directory).is_absolute() {
            return Err(invalid("The output directory must be absolute"));
        }
        let instance = crate::client::origin(&instance, self.allow_http)?;
        let settings = self.settings.clone();
        Ok(Arc::new(TransferJob::new(core::Job::spawn_in(
            &self.scheduler,
            settings,
            None,
            move |control| {
                protocol::background::prepare_inbox_download(
                    &instance,
                    &transfer_id,
                    &working_key,
                    &cookie_context,
                    Path::new(&output_directory),
                    control,
                )
                .map(|id| vec![id])
            },
        ))))
    }

    /// Call for every URLSession completion, including redeliveries after app
    /// relaunch. A completion is idempotent only for its exact operation id.
    pub fn ingest_upload_completion(
        &self,
        transfer_id: String,
        operation_id: String,
        status: u16,
        response_file: Option<String>,
    ) -> Result<()> {
        let response = response_file.as_deref().map(Path::new);
        protocol::background::ingest_upload_completion(
            &self.settings.state_home,
            &transfer_id,
            &operation_id,
            status,
            response,
            self.secrets(),
        )
        .map_err(operation)
    }

    /// Use after an ambiguous URLSession outcome (for example status `0`).
    /// The authenticated server ledger is authoritative, not the callback.
    pub fn reconcile_upload(&self, transfer_id: String) -> Arc<TransferJob> {
        let settings = self.settings.clone();
        Arc::new(TransferJob::new_upload(
            core::Job::spawn_in(
                &self.scheduler,
                settings,
                Some(transfer_id.clone()),
                move |control| {
                    protocol::background::reconcile_upload(&transfer_id, control)
                        .map(|()| Vec::new())
                },
            ),
            "http",
        ))
    }

    /// The response headers are copied from URLSession. Rust requires the
    /// exact v1 Content-Length and a strong ETag before retaining ciphertext.
    pub fn ingest_download_completion(
        &self,
        transfer_id: String,
        operation_id: String,
        status: u16,
        response_headers: Vec<BackgroundHeader>,
        response_file: String,
    ) -> Result<()> {
        let headers = response_headers
            .into_iter()
            .map(|header| protocol::background::BackgroundHeader {
                name: header.name,
                value: header.value,
            })
            .collect::<Vec<_>>();
        protocol::background::ingest_download_completion(
            &self.settings.state_home,
            &transfer_id,
            &operation_id,
            status,
            &headers,
            Path::new(&response_file),
            self.secrets(),
        )
        .map_err(operation)
    }

    /// Runs the foreground, idempotent manifest finalization only after all
    /// URLSession work has been acknowledged by Rust.
    pub fn finalize_upload(&self, transfer_id: String) -> Arc<TransferJob> {
        let settings = self.settings.clone();
        Arc::new(TransferJob::new_upload(
            core::Job::spawn_in(
                &self.scheduler,
                settings,
                Some(transfer_id.clone()),
                move |control| {
                    protocol::background::finalize_upload(&transfer_id, control)
                        .map(|url| vec![url])
                },
            ),
            "http",
        ))
    }

    /// Reopens the native checkpoint to authenticate staged ciphertext, prompt
    /// again when a password is required, verify full digests, and publish.
    pub fn finalize_download(&self, transfer_id: String) -> Arc<TransferJob> {
        let settings = self.settings.clone();
        Arc::new(TransferJob::new(core::Job::spawn_in(
            &self.scheduler,
            settings,
            Some(transfer_id.clone()),
            move |control| protocol::background::finalize_download(&transfer_id, control),
        )))
    }

    pub fn finalize_inbox_download(
        &self,
        transfer_id: String,
        instance: String,
        working_key: Vec<u8>,
        cookie_context: String,
    ) -> Result<Arc<TransferJob>> {
        if working_key.len() != 32
            || cookie_context.is_empty()
            || cookie_context.contains(['\r', '\n'])
        {
            return Err(invalid(
                "A valid account session and inbox key are required",
            ));
        }
        let instance = crate::client::origin(&instance, self.allow_http)?;
        let settings = self.settings.clone();
        Ok(Arc::new(TransferJob::new(core::Job::spawn_in(
            &self.scheduler,
            settings,
            Some(transfer_id.clone()),
            move |control| {
                protocol::background::finalize_inbox_download(
                    &transfer_id,
                    &instance,
                    &working_key,
                    &cookie_context,
                    control,
                )
            },
        ))))
    }
}

fn map_work(work: Vec<protocol::background::BackgroundWork>) -> Vec<BackgroundWork> {
    work.into_iter()
        .map(|work| BackgroundWork {
            operation_id: work.operation_id,
            transfer_id: work.transfer_id,
            method: work.method,
            url: work.url,
            headers: work
                .headers
                .into_iter()
                .map(|header| BackgroundHeader {
                    name: header.name,
                    value: header.value,
                })
                .collect(),
            body_path: work.body_path,
            expected_response_bytes: work.expected_response_bytes,
        })
        .collect()
}
