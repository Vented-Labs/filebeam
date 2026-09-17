use crate::{
    ClientConfig, DriverLimit, InstanceInfo, LinkInspection, NativeRuntime, Result, SavedTransfer,
    SavedTransferDetails, SecretStoreCallback, ShareLinkPresentation, SourceCallback, SourceKind,
    TransferJob, Transport, UploadAuthentication, UploadOptions, UploadSource, invalid, operation,
};
use filebeam_client_core::{self as core, control::TransferSettings, protocol, uploads};
use std::{path::PathBuf, sync::Arc};

#[derive(uniffi::Object)]
pub struct TransferClient {
    pub(super) settings: TransferSettings,
    scheduler: core::Scheduler,
    allow_http: bool,
}

#[uniffi::export]
impl TransferClient {
    #[uniffi::constructor]
    pub fn new(config: ClientConfig) -> Result<Self> {
        if !PathBuf::from(&config.state_directory).is_absolute() {
            return Err(invalid("The transfer state directory must be absolute"));
        }
        if !(64..=512).contains(&config.memory_budget_mib)
            || !(1..=4).contains(&config.max_concurrency)
        {
            return Err(invalid(
                "Use a 64-512 MiB buffer budget and 1-4 concurrent requests",
            ));
        }
        let memory_budget = u64::from(config.memory_budget_mib)
            .checked_mul(1024 * 1024)
            .ok_or_else(|| invalid("The transfer memory budget is too large"))?;
        let scheduler_memory = memory_budget
            .checked_add(core::RUNTIME_ALLOWANCE_BYTES)
            .and_then(|per_job| per_job.checked_add(core::TRANSIENT_MEMORY_ALLOWANCE_BYTES))
            .and_then(|per_job| per_job.checked_mul(u64::from(config.max_concurrency)))
            .ok_or_else(|| invalid("The scheduler memory budget is too large"))?;
        let scheduler = core::Scheduler::new(core::SchedulerLimits {
            workers: config.max_concurrency as usize,
            memory_bytes: scheduler_memory,
        })
        .map_err(operation)?;
        Ok(Self {
            settings: TransferSettings {
                state_home: config.state_directory.into(),
                max_concurrency: Some(config.max_concurrency),
                memory_budget,
                client_user_agent: Some(
                    concat!("filebeam-native/", env!("CARGO_PKG_VERSION")).into(),
                ),
                webrtc_relay_only: config.relay_only,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
            scheduler,
            allow_http: config.allow_http,
        })
    }

    #[uniffi::constructor]
    pub fn new_with_runtime(config: ClientConfig, runtime: Arc<NativeRuntime>) -> Result<Self> {
        let mut client = Self::new(config)?;
        client.scheduler = runtime.scheduler.clone();
        Ok(client)
    }

    #[uniffi::constructor]
    pub fn new_with_runtime_callbacks(
        config: ClientConfig,
        runtime: Arc<NativeRuntime>,
        secret_store: Arc<dyn SecretStoreCallback>,
        source: Arc<dyn SourceCallback>,
    ) -> Result<Self> {
        let mut client = Self::new_with_runtime(config, runtime)?;
        crate::secret_store::install(&mut client.settings, secret_store);
        crate::source::install(&mut client.settings, source);
        Ok(client)
    }

    #[uniffi::constructor]
    pub fn new_with_secret_store(
        config: ClientConfig,
        secret_store: Arc<dyn SecretStoreCallback>,
    ) -> Result<Self> {
        let mut client = Self::new(config)?;
        crate::secret_store::install(&mut client.settings, secret_store);
        Ok(client)
    }

    #[uniffi::constructor]
    pub fn new_with_source_callback(
        config: ClientConfig,
        source: Arc<dyn SourceCallback>,
    ) -> Result<Self> {
        let mut client = Self::new(config)?;
        crate::source::install(&mut client.settings, source);
        Ok(client)
    }

    #[uniffi::constructor]
    pub fn new_with_callbacks(
        config: ClientConfig,
        secret_store: Arc<dyn SecretStoreCallback>,
        source: Arc<dyn SourceCallback>,
    ) -> Result<Self> {
        let mut client = Self::new(config)?;
        crate::secret_store::install(&mut client.settings, secret_store);
        crate::source::install(&mut client.settings, source);
        Ok(client)
    }

    /// Performs network I/O: call from the host's I/O dispatcher.
    pub fn discover(&self, instance: String) -> Result<InstanceInfo> {
        let info =
            protocol::instance_info(&origin(&instance, self.allow_http)?).map_err(operation)?;
        let live = info
            .transport_limits
            .get("webrtc")
            .cloned()
            .unwrap_or_default();
        let drivers = info
            .enabled_drivers
            .iter()
            .map(|driver| {
                let limits = info.transport_limits.get(driver);
                DriverLimit {
                    driver: driver.clone(),
                    maximum_transfer_bytes: limits
                        .and_then(|limits| limits.maximum_transfer_bytes)
                        .or_else(|| {
                            (driver == "http")
                                .then_some(info.maximum_transfer_bytes)
                                .flatten()
                        }),
                    maximum_file_count: limits
                        .and_then(|limits| limits.maximum_file_count)
                        .or_else(|| {
                            (driver == "http")
                                .then_some(info.maximum_file_count)
                                .flatten()
                        })
                        .map(|value| value as u64),
                    maximum_note_bytes: limits.and_then(|limits| limits.maximum_note_bytes),
                }
            })
            .collect();
        Ok(InstanceInfo {
            name: info.name,
            anonymous_uploads: info.anonymous_uploads_enabled,
            enabled_transports: info.enabled_drivers,
            maximum_transfer_bytes: info.maximum_transfer_bytes,
            maximum_file_count: info.maximum_file_count.map(|n| n as u64),
            retention_hours: info.file_retention_hours,
            webrtc_maximum_transfer_bytes: live.maximum_transfer_bytes,
            webrtc_maximum_file_count: live.maximum_file_count.map(|n| n as u64),
            retention_options_hours: info.file_retention_options,
            drivers,
            default_driver: info.default_driver,
            chunk_bytes: info.chunk_bytes,
        })
    }

    /// Reads only public routing metadata; it never unlocks, claims, consumes,
    /// or starts a peer session.
    pub fn inspect_link(&self, instance: String, input: String) -> Result<LinkInspection> {
        let inspected =
            protocol::inspect_link_for_instance(&input, &origin(&instance, self.allow_http)?)
                .map_err(operation)?;
        Ok(LinkInspection {
            instance: inspected.instance,
            id: inspected.id,
            kind: inspected.kind,
            driver: inspected.driver,
            status: inspected.status,
            password_required: inspected.password_required,
        })
    }

    pub fn start_upload(
        &self,
        instance: String,
        paths: Vec<String>,
        transport: Transport,
        archive: bool,
    ) -> Result<Arc<TransferJob>> {
        self.start_upload_with_options(
            instance,
            paths,
            UploadOptions {
                transport,
                turbo: false,
                archive,
                password: false,
                retention_hours: None,
                authentication: UploadAuthentication {
                    bearer_token: None,
                    session_cookie: None,
                },
                recipient: None,
            },
        )
    }

    pub fn start_upload_with_options(
        &self,
        instance: String,
        paths: Vec<String>,
        options: UploadOptions,
    ) -> Result<Arc<TransferJob>> {
        if paths.is_empty() || paths.iter().any(|path| !PathBuf::from(path).is_absolute()) {
            return Err(invalid("Select at least one absolute local file path"));
        }
        Ok(self.start(core::Request::Upload {
            instance: origin(&instance, self.allow_http)?,
            paths: paths.into_iter().map(PathBuf::from).collect(),
            mode: if options.archive {
                uploads::DirectoryMode::Zip
            } else {
                uploads::DirectoryMode::Individual
            },
            options: upload_options(options)?,
        }))
    }

    pub fn start_download(
        &self,
        instance: String,
        link: String,
        output_directory: String,
    ) -> Result<Arc<TransferJob>> {
        let instance = origin(&instance, self.allow_http)?;
        let parsed = protocol::parse_link_for_instance(&link, &instance).map_err(operation)?;
        origin(&parsed.instance, self.allow_http)?;
        if !PathBuf::from(&output_directory).is_absolute() {
            return Err(invalid("The output directory must be absolute"));
        }
        Ok(self.start(core::Request::Download {
            instance,
            link,
            output: output_directory.into(),
        }))
    }

    /// Starts a recipient-authorized inbox download. Credentials are supplied
    /// only for this job and are deliberately absent from its checkpoint.
    pub fn start_inbox_download(
        &self,
        instance: String,
        transfer_id: String,
        working_key: Vec<u8>,
        cookie_context: String,
        output_directory: String,
    ) -> Result<Arc<TransferJob>> {
        let instance = origin(&instance, self.allow_http)?;
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
        Ok(self.start_operation(move |control| {
            protocol::download_inbox(
                &instance,
                &transfer_id,
                &working_key,
                &cookie_context,
                PathBuf::from(&output_directory).as_path(),
                control,
            )
            .map(|paths| {
                paths
                    .into_iter()
                    .map(|path| path.display().to_string())
                    .collect()
            })
        }))
    }

    /// Inbox checkpoints require a renewed same-origin session and key.
    pub fn resume_inbox_download(
        &self,
        checkpoint_id: String,
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
        let instance = origin(&instance, self.allow_http)?;
        Ok(self.start_operation(move |control| {
            protocol::resume_inbox(
                &checkpoint_id,
                &instance,
                &working_key,
                &cookie_context,
                control,
            )
        }))
    }

    /// Starts a bounded provider/path source upload. Provider handles are opened
    /// synchronously by the installed callback and their detached FDs are owned by Rust.
    pub fn start_upload_sources(
        &self,
        instance: String,
        sources: Vec<UploadSource>,
        options: UploadOptions,
    ) -> Result<Arc<TransferJob>> {
        if sources.is_empty() {
            return Err(invalid("Select at least one upload source"));
        }
        let has_provider = sources
            .iter()
            .any(|source| matches!(source.kind, SourceKind::Provider));
        if has_provider && self.settings.source_resolver.is_none() {
            return Err(invalid("Provider uploads require a SourceCallback"));
        }
        let sources = sources
            .into_iter()
            .map(source_spec)
            .collect::<Result<Vec<_>>>()?;
        Ok(self.start(core::Request::UploadSources {
            instance: origin(&instance, self.allow_http)?,
            sources,
            mode: if options.archive {
                uploads::DirectoryMode::Zip
            } else {
                uploads::DirectoryMode::Individual
            },
            options: upload_options(options)?,
        }))
    }

    pub fn saved_transfers(&self) -> Result<Vec<SavedTransfer>> {
        let secrets = self
            .settings
            .checkpoint_secret_store
            .clone()
            .unwrap_or_else(|| {
                filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(
                    &self.settings.state_home,
                )
            });
        protocol::saved_transfers_with_secret_store(&self.settings.state_home, secrets)
            .map_err(operation)
            .map(|jobs| {
                jobs.into_iter()
                    .map(|job| SavedTransfer {
                        id: job.id,
                        direction: job.direction,
                        state: job.state,
                        done: job.done,
                        total: job.total,
                    })
                    .collect()
            })
    }

    /// Authenticated checkpoint details without keys, cookies, or operation tokens.
    pub fn saved_transfer_details(&self, id: String) -> Result<SavedTransferDetails> {
        let secrets = self
            .settings
            .checkpoint_secret_store
            .clone()
            .unwrap_or_else(|| {
                filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(
                    &self.settings.state_home,
                )
            });
        let detail = protocol::saved_transfer_details_with_secret_store(
            &self.settings.state_home,
            &id,
            secrets,
        )
        .map_err(operation)?;
        Ok(SavedTransferDetails {
            id: detail.id,
            direction: detail.direction,
            kind: detail.kind,
            transport: detail.transport,
            state: detail.state,
            done: detail.done,
            total: detail.total,
            verified_privately: detail.verified_privately,
            exported: detail.exported,
            expires_at: detail.expires_at,
            can_resume: detail.can_resume,
            can_retry_save: detail.can_retry_save,
            can_end_live: detail.can_end_live,
            can_revoke_remote: detail.can_revoke_remote,
            can_remove_local: detail.can_remove_local,
        })
    }

    pub fn resume(&self, id: String) -> Arc<TransferJob> {
        self.start(core::Request::Resume { id })
    }

    pub fn revoke_upload(&self, id: String) -> Arc<TransferJob> {
        self.start_operation(move |control| {
            protocol::revoke_upload(&id, control).map(|()| Vec::new())
        })
    }

    pub fn end_live(&self, id: String) -> Arc<TransferJob> {
        self.start_operation(move |control| protocol::end_live(&id, control).map(|()| Vec::new()))
    }

    /// Removes local recovery data. It does not revoke the remote share.
    pub fn discard(&self, id: String) -> Result<()> {
        protocol::discard_transfer(&self.settings.state_home, &id).map_err(operation)
    }
}

pub(crate) fn source_spec(
    source: UploadSource,
) -> Result<filebeam_transfer_native::source::UploadSource> {
    if source.name.is_empty() || source.name.len() > 255 || source.path_or_identity.is_empty() {
        return Err(invalid(
            "Upload sources require a short name and non-empty location",
        ));
    }
    source
        .offset
        .checked_add(source.length)
        .ok_or_else(|| invalid("Upload source bounds overflow"))?;
    let spec = match source.kind {
        SourceKind::Path => {
            if source.mutation_token.is_some()
                || !PathBuf::from(&source.path_or_identity).is_absolute()
            {
                return Err(invalid(
                    "Path sources require an absolute path and no mutation token",
                ));
            }
            filebeam_transfer_native::source::SourceSpec::Path {
                path: source.path_or_identity.into(),
                offset: source.offset,
                length: source.length,
            }
        }
        SourceKind::Provider => filebeam_transfer_native::source::SourceSpec::Provider {
            identity: source.path_or_identity,
            offset: source.offset,
            length: source.length,
            mutation_token: source
                .mutation_token
                .filter(|token| !token.is_empty())
                .ok_or_else(|| invalid("Provider sources require a mutation token"))?,
        },
    };
    Ok(filebeam_transfer_native::source::UploadSource {
        name: source.name,
        spec,
    })
}

pub(crate) fn upload_options(options: UploadOptions) -> Result<protocol::UploadOptions> {
    if options.turbo
        && (!matches!(options.transport, Transport::Http) || options.recipient.is_some())
    {
        return Err(invalid(
            "Turbo requires HTTP file uploads without an inbox recipient",
        ));
    }
    if options.recipient.as_ref().is_some_and(|recipient| {
        options.password
            || !matches!(options.transport, Transport::Http)
            || recipient.username.trim().is_empty()
            || recipient.public_key.is_empty()
    }) {
        return Err(invalid(
            "Inbox delivery requires HTTP, a recipient key, and no transfer password",
        ));
    }
    let authentication = match options.authentication {
        UploadAuthentication {
            bearer_token: Some(token),
            session_cookie: None,
        } if !token.is_empty() => protocol::UploadAuthentication::Bearer(token),
        UploadAuthentication {
            bearer_token: None,
            session_cookie: Some(cookie),
        } if !cookie.is_empty() => protocol::UploadAuthentication::SessionCookie(cookie),
        UploadAuthentication {
            bearer_token: None,
            session_cookie: None,
        } => protocol::UploadAuthentication::Anonymous,
        _ => {
            return Err(invalid(
                "Provide exactly one non-empty upload authentication credential",
            ));
        }
    };
    Ok(protocol::UploadOptions {
        transport: match options.transport {
            Transport::Http => protocol::Transport::Http,
            Transport::WebRtc => protocol::Transport::WebRtc,
        },
        turbo: options.turbo,
        password: options.password,
        retention_hours: options.retention_hours,
        authentication,
        recipient: options
            .recipient
            .map(|recipient| protocol::UploadRecipient {
                username: recipient.username,
                user_id: recipient.user_id,
                account_key_bundle_id: recipient.account_key_bundle_id,
                public_key: recipient.public_key,
            }),
    })
}

impl TransferClient {
    fn start(&self, request: core::Request) -> Arc<TransferJob> {
        let transport = match &request {
            core::Request::Upload { options, .. }
            | core::Request::UploadSources { options, .. } => Some(match options.transport {
                protocol::Transport::Http => "http",
                protocol::Transport::WebRtc => "webrtc",
            }),
            _ => None,
        };
        let job = core::Job::start_in(&self.scheduler, self.settings.clone(), request);
        Arc::new(match transport {
            Some(transport) => TransferJob::new_upload(job, transport),
            None => TransferJob::new(job),
        })
    }

    fn start_operation(
        &self,
        operation: impl FnOnce(&core::control::Control) -> anyhow::Result<Vec<String>> + Send + 'static,
    ) -> Arc<TransferJob> {
        Arc::new(TransferJob::new(core::Job::spawn_in(
            &self.scheduler,
            self.settings.clone(),
            None,
            operation,
        )))
    }
}

pub(crate) fn origin(value: &str, allow_http: bool) -> Result<String> {
    let url = url::Url::parse(value.trim())
        .map_err(|_| invalid("Enter a valid Filebeam instance URL"))?;
    if (url.scheme() != "https" && !(allow_http && url.scheme() == "http"))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(
            "Use an HTTPS instance origin without a path, credentials, query, or fragment",
        ));
    }
    Ok(url.origin().ascii_serialization())
}

/// Formats the same link/key presentation as the web client. The key is caller
/// supplied and never read from a saved transfer checkpoint.
#[uniffi::export]
pub fn present_share_link(
    instance: String,
    share_url: String,
    share_key: String,
    include_key: bool,
) -> Result<ShareLinkPresentation> {
    let presentation =
        core::link_presentation::present_share_link(&instance, &share_url, &share_key, include_key)
            .map_err(operation)?;
    Ok(ShareLinkPresentation {
        link: presentation.link,
        separate_key: presentation.separate_key,
    })
}

/// Splits an existing canonical native link for separate-key presentation.
#[uniffi::export]
pub fn split_share_link(instance: String, link: String) -> Result<ShareLinkPresentation> {
    let presentation =
        core::link_presentation::split_share_link(&instance, &link).map_err(operation)?;
    Ok(ShareLinkPresentation {
        link: presentation.link,
        separate_key: presentation.separate_key,
    })
}

/// Estimates only known individual-file ciphertext bytes using the native v1
/// chunk/tag rules. It does not invent an estimate for ZIP or unknown sources.
#[uniffi::export]
pub fn estimate_upload_ciphertext_bytes(file_sizes: Vec<u64>, chunk_bytes: u64) -> Result<u64> {
    protocol::estimate_upload_ciphertext_bytes(&file_sizes, chunk_bytes).map_err(operation)
}
