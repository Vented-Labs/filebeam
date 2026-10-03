//! Native service boundary. The GPUI thread only submits commands and clones state.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use filebeam_client_config::{Config, normalize_server_url};
use filebeam_client_core::{
    ApplicationSettings, ClientRuntime, ExportState, JobAction, JobId, JobKind, PromptIdentity,
    PromptResponse, RUNTIME_ALLOWANCE_BYTES, Request, SchedulerLimits,
    TRANSIENT_MEMORY_ALLOWANCE_BYTES,
    control::{Control, TransferSettings},
    protocol::{self, Transport, UploadAuthentication, UploadOptions, UploadRecipient},
    services::local_state::{LocalState, PendingKey},
    services::note_management::NoteManagementStore,
    services::{
        AccountKeyUpload, export_self_key, generate_self_keypair, import_self_key,
        open_recipient_key, unwrap_password_key, validate_self_key,
    },
    services::{NoteCreate, NoteReceiveOptions, PendingBurn, ServiceClient},
    uploads::DirectoryMode as NativeDirectoryMode,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::model::*;

const COMMAND_QUEUE_CAPACITY: usize = 32;
const MESSAGE_HISTORY_CAPACITY: usize = 256;

pub struct DesktopClient {
    commands: SyncSender<ClientCommand>,
    snapshot: Arc<Mutex<DesktopSnapshot>>,
    note_memory: Arc<Mutex<NoteMemory>>,
}

impl DesktopClient {
    pub fn new(home: Option<PathBuf>) -> Result<Self> {
        let config = Config::load(home).context("load desktop configuration")?;
        let snapshot = Arc::new(Mutex::new(initial_snapshot(&config)));
        let note_memory = Arc::new(Mutex::new(NoteMemory::default()));
        let (commands, receiver) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        let worker = Worker::new(config, snapshot.clone(), note_memory.clone())?;
        thread::Builder::new()
            .name("filebeam-desktop-client".into())
            .spawn(move || worker.run(receiver))
            .context("start desktop worker")?;
        Ok(Self {
            commands,
            snapshot,
            note_memory,
        })
    }

    pub fn snapshot(&self) -> DesktopSnapshot {
        lock(&self.snapshot).clone()
    }
    pub fn dispatch(&self, command: ClientCommand) -> Result<()> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                TrySendError::Full(_) => anyhow::anyhow!("desktop service queue is full"),
                TrySendError::Disconnected(_) => anyhow::anyhow!("desktop service stopped"),
            })
    }
    /// Explicit one-time receipt accessor. Note text is removed before returning.
    pub fn take_opened_note(&self, id: &str) -> Option<String> {
        lock(&self.note_memory)
            .opened
            .remove(id)
            .map(|text| text.to_string())
    }

    /// Snapshot-only client for tests and the explicitly feature-gated visual harness.
    /// It never starts a worker; callers may observe commands through the receiver.
    #[cfg(any(test, feature = "visual-test"))]
    pub fn test_client(snapshot: DesktopSnapshot) -> (Self, Receiver<ClientCommand>) {
        let (commands, receiver) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        (
            Self {
                commands,
                snapshot: Arc::new(Mutex::new(snapshot)),
                note_memory: Arc::new(Mutex::new(NoteMemory::default())),
            },
            receiver,
        )
    }

    /// Replaces the complete deterministic visual snapshot without running a
    /// worker. Kept out of normal builds so fixtures cannot affect services.
    #[cfg(feature = "visual-test")]
    pub fn replace_visual_snapshot(&self, snapshot: DesktopSnapshot) {
        *lock(&self.snapshot) = snapshot;
    }
}

#[derive(Default)]
struct NoteMemory {
    opened: HashMap<String, Zeroizing<String>>,
    burns: HashMap<String, PendingBurn>,
    snapshots: Vec<NoteSnapshot>,
}

struct Worker {
    config: Config,
    state: LocalState,
    runtime: ClientRuntime,
    snapshot: Arc<Mutex<DesktopSnapshot>>,
    note_memory: Arc<Mutex<NoteMemory>>,
    service: Option<ServiceClient>,
    receive_roots: HashMap<JobId, PathBuf>,
    include_keys: HashMap<JobId, bool>,
    /// Titles derived from an authenticated prepared receive manifest. They are
    /// kept separately from progress, whose name changes as items are processed.
    prepared_receive_titles: HashMap<JobId, String>,
    note_jobs: HashMap<JobId, NoteJob>,
    prepared_receives: Arc<Mutex<HashMap<String, PreparedReceive>>>,
    journal: DesktopJournal,
    next_message_id: u64,
    inbox_receiver: Option<(Control, thread::JoinHandle<()>)>,
    account_id: std::cell::Cell<u64>,
}

struct PreparedReceive {
    attached_note: Option<protocol::AttachedNote>,
    link: String,
    output: PathBuf,
    source: PreparedReceiveSource,
    items: Vec<ReceiveItem>,
}

enum PreparedReceiveSource {
    Public,
}

struct NoteJob {
    id: Arc<Mutex<Option<String>>>,
    end_requested: Arc<AtomicBool>,
    include_key: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct DesktopJournal {
    #[serde(default)]
    exports: BTreeMap<String, JournalExport>,
}

#[derive(Serialize, Deserialize)]
struct JournalExport {
    destination: PathBuf,
    paths: Vec<String>,
}

impl Worker {
    fn new(
        config: Config,
        snapshot: Arc<Mutex<DesktopSnapshot>>,
        note_memory: Arc<Mutex<NoteMemory>>,
    ) -> Result<Self> {
        let transfer = transfer_settings(&config);
        let limits = scheduler_limits(&config)?;
        let runtime = ClientRuntime::process_global(
            ApplicationSettings::new(transfer, limits)
                .with_ice_override(config.native_ice_override()),
        )?;
        let state = LocalState::new(config.home.clone(), &config.server.url)?;
        let service = state.load_client()?;
        let journal = load_journal(&config.home)?;
        let mut worker = Self {
            config,
            state,
            runtime,
            snapshot,
            note_memory,
            service: Some(service),
            receive_roots: HashMap::new(),
            include_keys: HashMap::new(),
            prepared_receive_titles: HashMap::new(),
            note_jobs: HashMap::new(),
            prepared_receives: Arc::new(Mutex::new(HashMap::new())),
            journal,
            next_message_id: 0,
            inbox_receiver: None,
            account_id: std::cell::Cell::new(0),
        };
        worker.restore_saved()?;
        Ok(worker)
    }

    fn run(mut self, receiver: Receiver<ClientCommand>) {
        self.publish();
        // Discovery is public and must not depend on a saved account session.
        self.refresh_policy();
        let _ = self.refresh_account();
        loop {
            match receiver.recv_timeout(Duration::from_millis(75)) {
                Ok(command) => self.handle(command),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            };
            self.publish();
        }
    }
    fn handle(&mut self, command: ClientCommand) {
        let operation = command.operation();
        match self.handle_result(command) {
            Ok(()) => self.complete(operation),
            Err(error) => self.message(operation, error),
        }
    }
    fn handle_result(&mut self, command: ClientCommand) -> Result<()> {
        match command {
            ClientCommand::ReloadSettings => self.reload_settings(),
            ClientCommand::Refresh => self.refresh_account(),
            ClientCommand::RefreshInbox => self.refresh_inbox(),
            ClientCommand::RefreshHistory { filter, cursor } => {
                self.refresh_history(filter, cursor.as_deref())
            }
            ClientCommand::DeleteHistory { id } => {
                let result = self.service()?.history().delete(&id);
                self.finish_history_action(result)
            }
            ClientCommand::ExtendHistory {
                id,
                retention_hours,
            } => {
                let result = self
                    .service()?
                    .history()
                    .extend(&id, retention_hours)
                    .map(|_| ());
                self.finish_history_action(result)
            }
            ClientCommand::RefreshContacts => self.refresh_contacts(),
            ClientCommand::ContactAction {
                username,
                action,
                can_send,
                auto_download,
            } => {
                let contacts = self.service()?.account().contact_action(
                    &username,
                    &action,
                    can_send,
                    auto_download,
                )?;
                lock(&self.snapshot).contacts = Some(contacts);
                Ok(())
            }
            ClientCommand::ReceivingDefaults {
                policy,
                auto_download,
            } => {
                self.service()?
                    .account()
                    .set_receiving_defaults(&policy, auto_download)?;
                self.refresh_contacts()
            }
            ClientCommand::AutomaticReceiving(enabled) => {
                if let Some((control, _)) = self.inbox_receiver.take() {
                    control.cancel();
                }
                let service = self.service()?;
                let id = self.local_account_id()?;
                let root = self.config.home.join("transfers/inbox-staging");
                let receiver = filebeam_client_core::services::inbox_receiver::InboxReceiver::open(
                    &root,
                    &service.instance(),
                    id,
                    filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(
                        &root,
                    ),
                )?;
                receiver.set_enabled(enabled)?;
                lock(&self.snapshot).auto_receiving = enabled;
                drop(receiver);
                self.start_inbox_receiver()
            }
            ClientCommand::SaveStagedInbox { id, destination } => {
                let _memory = self
                    .runtime
                    .scheduler()
                    .try_reserve_service_memory(64 * 1024 * 1024)?;
                let service = self.service()?;
                let account_id = self.local_account_id()?;
                let key = self.state.load_private_key()?;
                let root = self.config.home.join("transfers/inbox-staging");
                let receiver = filebeam_client_core::services::inbox_receiver::InboxReceiver::open(
                    &root,
                    &service.instance(),
                    account_id,
                    filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(
                        &root,
                    ),
                )?;
                let (sender, _) = mpsc::channel();
                let control = Control::new(self.runtime.transfer_settings(), sender);
                receiver.export(&id, &key, &destination, &control)?;
                Ok(())
            }
            ClientCommand::DismissStagedInbox { id } => {
                let root = self.config.home.join("transfers/inbox-staging");
                let receiver = filebeam_client_core::services::inbox_receiver::InboxReceiver::open(
                    &root,
                    &self.service()?.instance(),
                    self.local_account_id()?,
                    filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(
                        &root,
                    ),
                )?;
                receiver.dismiss(&id)?;
                lock(&self.snapshot).staged_inbox = receiver.state()?.entries;
                Ok(())
            }
            ClientCommand::GenerateReceivingKey {
                password,
                replace,
                acknowledge_old_key_loss,
            } => self.generate_receiving_key(password, replace, acknowledge_old_key_loss),
            ClientCommand::ImportReceivingKey { export } => self.import_receiving_key(export),
            ClientCommand::UnlockReceivingKey { password } => self.unlock_receiving_key(password),
            ClientCommand::ExportReceivingKey { destination } => {
                self.export_receiving_key(destination)
            }
            ClientCommand::ConfirmReceivingKeyBackup { generation_id } => {
                self.confirm_receiving_key_backup(generation_id)
            }
            ClientCommand::ReceiveInbox { id, item_ids } => self.receive_inbox(id, item_ids),
            ClientCommand::ChangeInstance { url } => self.change_instance(url),
            ClientCommand::SendFiles(request) => self.start_send(request),
            ClientCommand::ReceiveLink { link, private_dir } => {
                self.start_receive(link, private_dir)
            }
            ClientCommand::InspectReceive { link } => self.inspect_receive(link),
            ClientCommand::SelectReceiveItems {
                operation_id,
                item_ids,
            } => self.select_receive_items(&operation_id, item_ids),
            ClientCommand::StartPreparedReceive { operation_id } => {
                self.start_prepared_receive(&operation_id)
            }
            ClientCommand::Resume { id } => {
                self.runtime
                    .dispatch(self.runtime_job_id(&id)?, JobAction::Resume)?;
                Ok(())
            }
            ClientCommand::Pause { id } => {
                self.runtime
                    .dispatch(self.runtime_job_id(&id)?, JobAction::Pause)?;
                Ok(())
            }
            ClientCommand::Discard { id } => {
                let job = self.runtime_job_id(&id)?;
                let checkpoint = self
                    .runtime
                    .snapshot(job)
                    .and_then(|entry| entry.job.checkpoint_id);
                self.runtime.dispatch(job, JobAction::Discard)?;
                if let Some(checkpoint) = checkpoint {
                    self.journal.exports.remove(&checkpoint);
                    save_journal(&self.config.home, &self.journal)?;
                }
                Ok(())
            }
            ClientCommand::EndLive { id } => self.start_management(self.runtime_job_id(&id)?, true),
            ClientCommand::Revoke { id } => self.start_management(self.runtime_job_id(&id)?, false),
            ClientCommand::AnswerPrompt {
                transfer_id,
                prompt_id,
                answer,
            } => self.answer_prompt(&transfer_id, prompt_id, answer),
            ClientCommand::ExportVerified {
                transfer_id,
                destination,
            } => self.export(self.runtime_job_id(&transfer_id)?, destination),
            ClientCommand::CreateNote(draft) => self.start_note(draft),
            ClientCommand::OpenNote {
                link,
                password,
                burn_acknowledged,
            } => self.open_note(link, password, burn_acknowledged),
            ClientCommand::RetryBurn { id } => self.retry_burn(id),
            ClientCommand::Login {
                email,
                password,
                remember,
            } => self.login(email, password, remember),
            ClientCommand::Logout => self.logout(),
            ClientCommand::Register {
                username,
                name,
                email,
                password,
            } => self.register(username, name, email, password),
            ClientCommand::RequestPasswordReset { email } => {
                self.service()?.account().request_password_reset(&email)
            }
            ClientCommand::ResetPassword {
                email,
                token,
                password,
            } => {
                self.service()?
                    .account()
                    .reset_password(&email, &token, &Zeroizing::new(password))
            }
            ClientCommand::ResendVerification => self.service()?.account().resend_verification(),
            ClientCommand::VerifyEmailLink { link } => {
                self.service()?.account().verify_email_link(&link)
            }
            ClientCommand::SetInboxEnabled(enabled) => {
                self.service()?.account().set_inbox_enabled(enabled)
            }
            ClientCommand::SetNotificationChannel(channel) => self
                .service()?
                .account()
                .set_notification_channel(if channel == NotificationChannel::Mail {
                    "mail"
                } else {
                    "database"
                }),
            ClientCommand::DeleteInboxItem { id } => {
                self.service()?.account().delete_inbox_item(&id)
            }
            ClientCommand::InspectInbox { id } => self.inspect_inbox(&id),
            ClientCommand::InspectInvitation { token } => self.inspect_invitation(&token),
            ClientCommand::AcceptInvitation {
                token,
                username,
                name,
                email,
                password,
            } => self.accept_invitation(token, username, name, email, password),
            ClientCommand::Report {
                transfer_id,
                category,
                description,
                email,
            } => self.service()?.account().report(
                &transfer_id,
                &category,
                &description,
                email.as_deref(),
            ),
            ClientCommand::DeleteAccount {
                current_password,
                confirmation,
            } => self
                .service()?
                .account()
                .delete_account(&Zeroizing::new(current_password), &confirmation)
                .map(|_| ()),
            ClientCommand::Shutdown { wait_ms } => {
                if let Some((control, _)) = self.inbox_receiver.take() {
                    control.cancel();
                }
                self.runtime.shutdown(Duration::from_millis(wait_ms));
                Ok(())
            }
        }
    }

    fn service(&self) -> Result<ServiceClient> {
        self.service
            .clone()
            .map(Ok)
            .unwrap_or_else(|| self.state.load_client())
    }
    fn local_account_id(&self) -> Result<u64> {
        let id = self.account_id.get();
        if id != 0 {
            return Ok(id);
        }
        self.state
            .cached_account_id()?
            .context("Sign in before receiving files")
    }
    fn runtime_job_id(&self, id: &str) -> Result<JobId> {
        id.parse::<u64>()
            .map(JobId)
            .ok()
            .or_else(|| {
                self.runtime
                    .snapshots()
                    .into_iter()
                    .find(|entry| entry.job.checkpoint_id.as_deref() == Some(id))
                    .map(|entry| entry.id)
            })
            .context("unknown desktop transfer id")
    }
    fn restore_saved(&mut self) -> Result<()> {
        for observation in self.runtime.restore_saved()? {
            let snapshot = observation
                .snapshot()
                .expect("recovered transfer was registered");
            let checkpoint = snapshot
                .job
                .checkpoint_id
                .clone()
                .expect("recovery requires checkpoint");
            if snapshot.kind == JobKind::Download
                && let Some(path) = snapshot.job.results.first().map(PathBuf::from)
                && let Some(root) = path.parent()
            {
                self.receive_roots.insert(snapshot.id, root.to_owned());
            }
            if let Some(saved) = self.journal.exports.get(&checkpoint)
                && saved.paths.iter().all(|path| {
                    Path::new(path)
                        .file_name()
                        .is_some_and(|name| saved.destination.join(name).is_file())
                })
            {
                self.runtime.restore_export_state(
                    snapshot.id,
                    ExportState::Exported {
                        destination: saved.destination.clone(),
                        paths: saved.paths.clone(),
                    },
                )?;
            }
        }
        Ok(())
    }
    fn start_send(&mut self, request: SendFiles) -> Result<()> {
        if request.paths.is_empty() {
            bail!("select at least one file or directory");
        }
        self.validate_send_policy(&request)?;
        let recipient = match request.recipient {
            Some(username) => {
                let recipient = self.service()?.account().recipient(&username)?;
                if request
                    .expected_recipient_id
                    .is_some_and(|id| id != recipient.id)
                {
                    bail!("The saved contact account changed. Refresh contacts before sending.");
                }
                Some(UploadRecipient {
                    username: recipient.username,
                    user_id: recipient.id,
                    account_key_bundle_id: recipient.account_key_bundle_id,
                    public_key: recipient.public_key,
                })
            }
            None => None,
        };
        let authentication = self.service()?.upload_authentication()?;
        let password = Zeroizing::new(request.password);
        let options = UploadOptions {
            attached_note: request.attached_note,
            snapshot_paths: Vec::new(),
            transport: if request.transport == SendTransport::Live {
                Transport::WebRtc
            } else {
                Transport::Http
            },
            turbo: request.turbo,
            password: password.is_some(),
            retention_hours: request.retention_hours,
            authentication,
            recipient,
        };
        let mode = if request.directory_mode == DirectoryMode::Zip {
            NativeDirectoryMode::Zip
        } else {
            NativeDirectoryMode::Individual
        };
        let include_key = request.include_key;
        let job = self.runtime.start_upload(Request::Upload {
            instance: self.config.server.url.clone(),
            paths: request.paths,
            mode,
            options,
        })?;
        self.include_keys.insert(job.id(), include_key);
        Ok(())
    }
    fn start_receive(&mut self, link: String, private_dir: PathBuf) -> Result<()> {
        ensure_private_dir(&private_dir)?;
        let inspection = protocol::inspect_link_for_instance(&link, &self.config.server.url)?;
        if inspection.kind == "note" {
            self.inspect_note(link, inspection.password_required)?;
            bail!("this link is a note; open it with explicit burn-on-read confirmation")
        }
        let job = self.runtime.start_download(Request::Download {
            instance: self.config.server.url.clone(),
            link,
            output: private_dir.clone(),
        })?;
        self.receive_roots.insert(job.id(), private_dir);
        Ok(())
    }
    fn inspect_receive(&mut self, link: String) -> Result<()> {
        let inspection = protocol::inspect_link_for_instance(&link, &self.config.server.url)?;
        if inspection.kind == "note" {
            return self.inspect_note(link, inspection.password_required);
        }
        if inspection.kind != "files" {
            bail!("unsupported receive link kind")
        }
        let output = self
            .config
            .home
            .join("private-receive")
            .join(&inspection.id);
        ensure_private_dir(&output)?;
        let instance = self.config.server.url.clone();
        let prepared = self.prepared_receives.clone();
        let state_home = self.runtime.transfer_settings().state_home;
        self.runtime
            .start_service_job(JobKind::Service, move |control| {
                let id =
                    protocol::background::prepare_download(&instance, &link, &output, control)?;
                let secrets =
                    filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(
                        &state_home,
                    );
                let items = protocol::background::download_items(&state_home, &id, secrets)?
                    .into_iter()
                    .map(|item| ReceiveItem {
                        id: item.id,
                        name: item.name,
                        size: item.size,
                    })
                    .collect();
                lock(&prepared).insert(
                    id.clone(),
                    PreparedReceive {
                        attached_note: control.attached_note(),
                        link,
                        output,
                        source: PreparedReceiveSource::Public,
                        items,
                    },
                );
                Ok(vec![id])
            });
        Ok(())
    }
    fn inspect_note(&mut self, link: String, password_required: bool) -> Result<()> {
        let note = self.service()?.notes().inspect(&link)?;
        lock(&self.snapshot)
            .receive_previews
            .retain(|preview| preview.link != link);
        lock(&self.snapshot).receive_previews.push(ReceivePreview {
            attached_note: None,
            operation_id: None,
            link,
            kind: ReceiveKind::Note,
            password_required,
            burn_on_read: note.burn_on_read,
            items: Vec::new(),
        });
        Ok(())
    }
    fn select_receive_items(&mut self, operation_id: &str, item_ids: Vec<String>) -> Result<()> {
        if !lock(&self.prepared_receives).contains_key(operation_id) {
            bail!("prepared receive operation is unavailable")
        }
        let settings = self.runtime.transfer_settings();
        protocol::background::select_download_items(
            &settings.state_home,
            operation_id,
            &item_ids,
            filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(
                &settings.state_home,
            ),
        )?;
        let selected = item_ids
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        lock(&self.prepared_receives)
            .get_mut(operation_id)
            .expect("prepared receive was checked above")
            .items
            .retain(|item| selected.contains(&item.id));
        Ok(())
    }
    fn start_prepared_receive(&mut self, operation_id: &str) -> Result<()> {
        let PreparedReceive {
            output,
            source,
            items,
            ..
        } = lock(&self.prepared_receives)
            .remove(operation_id)
            .context("prepared receive operation is unavailable")?;
        let checkpoint_id = operation_id.to_owned();
        let job = match source {
            PreparedReceiveSource::Public => self.runtime.resume(checkpoint_id)?,
        };
        self.receive_roots.insert(job.id(), output);
        self.prepared_receive_titles.insert(
            job.id(),
            transfer_title(
                None,
                items.first().map(|item| item.name.as_str()),
                items.len(),
            ),
        );
        Ok(())
    }
    fn start_management(&mut self, id: JobId, end_live: bool) -> Result<()> {
        if let Some(note) = self.note_jobs.get(&id) {
            let note_id = lock(&note.id)
                .clone()
                .context("note management capability is not ready")?;
            let management =
                NoteManagementStore::for_root(self.runtime.transfer_settings().state_home);
            let availability = management.availability(&note_id);
            if end_live {
                if !availability.can_end_live {
                    bail!("end live is not available for this note")
                }
                // The owning live-note job performs the authenticated remote end after
                // its serving loop observes this request.
                note.end_requested.store(true, Ordering::Relaxed);
                return Ok(());
            }
            if !availability.can_revoke {
                bail!("revoke is not available for this note")
            }
            let root = self.runtime.transfer_settings().state_home;
            self.runtime.start_revoke(move |_| {
                NoteManagementStore::for_root(root).action(
                    &note_id,
                    filebeam_client_core::services::note_management::NoteManagementAction::Revoke,
                )?;
                Ok(vec![note_id])
            });
            return Ok(());
        }
        let checkpoint = self
            .runtime
            .snapshot(id)
            .context("transfer is no longer available")?
            .job
            .checkpoint_id
            .context("transfer has no remote capability")?;
        let root = self.runtime.transfer_settings().state_home;
        if end_live {
            self.runtime.start_end_live(move |control| { let notes = NoteManagementStore::for_root(&root); if notes.contains(&checkpoint) { notes.action(&checkpoint, filebeam_client_core::services::note_management::NoteManagementAction::EndLive)?; } else { protocol::end_live(&checkpoint, control)?; } Ok(vec![checkpoint]) });
        } else {
            self.runtime.start_revoke(move |control| { let notes = NoteManagementStore::for_root(&root); if notes.contains(&checkpoint) { notes.action(&checkpoint, filebeam_client_core::services::note_management::NoteManagementAction::Revoke)?; } else { protocol::revoke_upload(&checkpoint, control)?; } Ok(vec![checkpoint]) });
        };
        Ok(())
    }
    fn answer_prompt(&self, id: &str, prompt: u64, answer: PromptAnswer) -> Result<()> {
        let response = match answer {
            PromptAnswer::Secret(value) => {
                PromptResponse::Secret(Zeroizing::new(value).to_string())
            }
            PromptAnswer::AllowPeer(allowed) => PromptResponse::Consent { allowed },
            PromptAnswer::Directory(mode) => PromptResponse::Directory {
                zip: mode == DirectoryMode::Zip,
            },
        };
        self.runtime.respond(
            PromptIdentity {
                job: self.runtime_job_id(id)?,
                prompt,
            },
            response,
        )
    }
    fn start_note(&mut self, draft: NoteDraft) -> Result<()> {
        let service = self.service()?;
        let root = self.runtime.transfer_settings().state_home;
        let memory = self.note_memory.clone();
        let instance = self.config.server.url.clone();
        let note_id = Arc::new(Mutex::new(None));
        let end_requested = Arc::new(AtomicBool::new(false));
        let created_id = note_id.clone();
        let ending = end_requested.clone();
        let kind = if draft.transport == NoteTransport::Live {
            JobKind::LiveNote
        } else {
            JobKind::Note
        };
        let job = self.runtime.start_service_job(kind, move |control| {
            let management = NoteManagementStore::for_root(root);
            let transport = draft.transport;
            let burn_on_read = draft.burn_on_read;
            let request = NoteCreate {
                text: draft.text,
                title: draft.title,
                language: draft.language,
                password: draft.password,
                burn_on_read,
                retention_hours: draft.retention_hours,
            };
            let created = match transport {
                NoteTransport::Http => {
                    let created = service.notes().create_with_control(request, control)?;
                    service.notes().save_management(&created, &management)?;
                    *lock(&created_id) = Some(created.id.clone());
                    created
                }
                NoteTransport::Live => service.notes().create_live_with_management_id(
                    request,
                    control,
                    ending,
                    Some(management),
                    Some(created_id.clone()),
                )?,
            };
            if transport == NoteTransport::Http {
                control.emit(
                    filebeam_transfer_native::control::TransferEvent::ShareReady(
                        filebeam_transfer_native::control::ShareReady {
                            share_url: created.link.clone(),
                        },
                    ),
                );
            }
            let (link, separate_key) = if draft.include_key {
                (created.link.clone(), None)
            } else {
                let presentation = filebeam_client_core::link_presentation::split_share_link(
                    &instance,
                    &created.link,
                )?;
                (presentation.link, Some(presentation.separate_key))
            };
            lock(&memory).snapshots.push(NoteSnapshot {
                id: created.id.clone(),
                link: Some(link.clone()),
                separate_key,
                transport,
                state: if transport == NoteTransport::Live {
                    NoteState::WaitingForReader
                } else {
                    NoteState::Complete
                },
                burn_on_read,
                retry_burn: false,
                text_available: false,
                error: None,
            });
            Ok(vec![link])
        });
        self.note_jobs.insert(
            job.id(),
            NoteJob {
                id: note_id,
                end_requested,
                include_key: draft.include_key,
            },
        );
        Ok(())
    }
    fn open_note(
        &mut self,
        link: String,
        password: Option<String>,
        burn_acknowledged: bool,
    ) -> Result<()> {
        let service = self.service()?;
        let memory = self.note_memory.clone();
        self.runtime
            .start_service_job(JobKind::Service, move |control| {
                let received = service.notes().receive(
                    &link,
                    password.as_deref(),
                    NoteReceiveOptions {
                        burn_acknowledged,
                        control: Some(control),
                    },
                )?;
                let id = received.note.id.clone();
                let mut memory = lock(&memory);
                let retry_burn = received.pending_burn.is_some();
                memory
                    .opened
                    .insert(id.clone(), Zeroizing::new(received.note.text));
                if let Some(burn) = received.pending_burn {
                    memory.burns.insert(id.clone(), burn);
                }
                memory.snapshots.retain(|note| note.id != id);
                memory.snapshots.push(NoteSnapshot {
                    id: id.clone(),
                    link: None,
                    separate_key: None,
                    transport: NoteTransport::Http,
                    state: if retry_burn {
                        NoteState::BurnPending
                    } else {
                        NoteState::Opened
                    },
                    burn_on_read: retry_burn,
                    retry_burn,
                    text_available: true,
                    error: None,
                });
                Ok(vec![id])
            });
        Ok(())
    }
    fn retry_burn(&mut self, id: String) -> Result<()> {
        let service = self.service()?;
        let memory = self.note_memory.clone();
        self.runtime.start_service_job(JobKind::Service, move |_| {
            let pending = lock(&memory)
                .burns
                .remove(&id)
                .context("no pending burn retry")?;
            service.notes().retry_burn(&pending)?;
            if let Some(note) = lock(&memory)
                .snapshots
                .iter_mut()
                .find(|note| note.id == id)
            {
                note.retry_burn = false;
                note.state = NoteState::Opened;
            }
            Ok(vec![id])
        });
        Ok(())
    }
    fn export(&mut self, id: JobId, destination: PathBuf) -> Result<()> {
        let root = self
            .receive_roots
            .get(&id)
            .cloned()
            .context("verified receive root is unavailable")?;
        self.runtime
            .commit_export(id, destination, move |paths, destination| {
                export_verified(paths, &root, destination)
            })?;
        let snapshot = self
            .runtime
            .snapshot(id)
            .context("transfer was discarded during export")?;
        if let (Some(checkpoint), ExportState::Exported { destination, paths }) =
            (snapshot.job.checkpoint_id, snapshot.export)
        {
            self.journal
                .exports
                .insert(checkpoint, JournalExport { destination, paths });
            save_journal(&self.config.home, &self.journal)?;
        }
        Ok(())
    }
    fn refresh_account(&mut self) -> Result<()> {
        self.refresh_policy();
        match self.service()?.account().session() {
            Ok(session) => {
                self.set_account(session);
                self.refresh_key_custody()?;
                let _ = self.refresh_contacts();
                self.start_inbox_receiver()
            }
            // A public discovery remains useful when an old account cookie expires.
            Err(error) if is_auth_error(&error) => {
                let mut snapshot = lock(&self.snapshot);
                snapshot.account = AccountSnapshot::default();
                snapshot.history = HistorySnapshot::default();
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
    fn refresh_policy(&self) {
        let origin = self.config.server.url.clone();
        match protocol::instance_info(&origin) {
            Ok(info) => {
                let mut snapshot = lock(&self.snapshot);
                snapshot.instance.connected = true;
                snapshot.instance.detail = None;
                snapshot.policy = policy_snapshot(&origin, info);
            }
            Err(error) => {
                let mut snapshot = lock(&self.snapshot);
                snapshot.instance.connected = false;
                snapshot.instance.detail = Some(sanitize(&error));
                if snapshot.policy.origin == origin
                    && snapshot.policy.availability != PolicyAvailability::Unavailable
                {
                    snapshot.policy.availability = PolicyAvailability::Cached;
                    snapshot.policy.reason = snapshot.instance.detail.clone();
                } else {
                    snapshot.policy = PolicySnapshot {
                        origin,
                        availability: PolicyAvailability::Unavailable,
                        reason: snapshot.instance.detail.clone(),
                        ..PolicySnapshot::default()
                    };
                }
            }
        }
    }
    fn validate_send_policy(&self, request: &SendFiles) -> Result<()> {
        let policy = &lock(&self.snapshot).policy;
        if policy.origin != self.config.server.url
            || policy.availability == PolicyAvailability::Unavailable
        {
            bail!("instance policy is unavailable; refresh before sharing")
        }
        let driver = if request.transport == SendTransport::Http {
            &policy.http
        } else {
            &policy.webrtc
        };
        if !driver.enabled {
            bail!("the selected transport is not enabled by this instance")
        }
        if !policy.anonymous_uploads
            && matches!(
                self.service()?.upload_authentication()?,
                UploadAuthentication::Anonymous
            )
        {
            bail!("this instance requires an authenticated account for sharing")
        }
        if let Some(hours) = request.retention_hours
            && !policy.retention_options_hours.contains(&hours)
        {
            bail!("the selected retention is not available on this instance")
        }
        Ok(())
    }
    fn refresh_key_custody(&self) -> Result<()> {
        let custody = if let Some(pending) = self.state.load_pending_key()? {
            KeyCustody::PendingBackup {
                generation_id: pending.id,
                fingerprint: pending.fingerprint,
            }
        } else if let Some(active) = self.service()?.account().key_situation()?.active {
            let configured = self.state.load_private_key().is_ok();
            if configured {
                KeyCustody::Configured {
                    version: active.version,
                    fingerprint: active.fingerprint,
                    custody_mode: active.custody_mode,
                }
            } else {
                KeyCustody::Locked {
                    version: active.version,
                    fingerprint: active.fingerprint,
                    custody_mode: active.custody_mode,
                }
            }
        } else {
            KeyCustody::Unknown
        };
        lock(&self.snapshot).account.key_custody = custody;
        Ok(())
    }
    fn generate_receiving_key(
        &mut self,
        password: Option<String>,
        replace: bool,
        acknowledge_old_key_loss: bool,
    ) -> Result<()> {
        let situation = self.service()?.account().key_situation()?;
        if replace && situation.active.is_some() && !acknowledge_old_key_loss {
            bail!(
                "replacing a receiving key can make old inbox deliveries unrecoverable; acknowledge old key loss"
            );
        }
        if situation.active.is_some() && !replace {
            bail!("an active receiving key already exists; use replace");
        }
        let material = generate_self_keypair()?;
        if let Some(password) = password {
            let password = Zeroizing::new(password);
            let session = self.service()?.account().session()?;
            let envelope = filebeam_client_core::services::wrap_password_key(
                &material.private_key,
                password.as_bytes(),
                session.id,
                &material.public_key,
            )?;
            self.service()?.account().upload_key(&AccountKeyUpload {
                public_key: material.public_key,
                fingerprint: material.fingerprint,
                custody_mode: "password".into(),
                encrypted_private_key: Some(envelope),
                current_password: Some(password.to_string()),
                replace,
            })?;
            self.state.clear_private_key()?;
        } else {
            let pending = PendingKey {
                id: material.fingerprint.clone(),
                private_key: material.private_key,
                public_key: material.public_key,
                fingerprint: material.fingerprint,
                backup_exported: false,
                replace,
            };
            self.state.save_pending_key(&pending)?;
        }
        self.refresh_key_custody()
    }
    fn import_receiving_key(&mut self, export: String) -> Result<()> {
        let export = Zeroizing::new(export);
        let key = import_self_key(&export)?;
        let active = self
            .service()?
            .account()
            .key_situation()?
            .active
            .context("account has no active receiving key to validate")?;
        validate_self_key(&key, &active.public_key)?;
        self.state.save_private_key(&key)?;
        self.refresh_key_custody()
    }
    fn unlock_receiving_key(&mut self, password: String) -> Result<()> {
        let password = Zeroizing::new(password);
        let session = self.service()?.account().session()?;
        let active = self
            .service()?
            .account()
            .key_situation()?
            .active
            .context("account has no active receiving key")?;
        let envelope = active
            .encrypted_private_key
            .as_deref()
            .context("active receiving key cannot be unlocked with a password")?;
        let key = unwrap_password_key(
            envelope,
            password.as_bytes(),
            session.id,
            &active.public_key,
        )?;
        validate_self_key(&key, &active.public_key)?;
        self.state.save_private_key(&key)?;
        self.refresh_key_custody()
    }
    fn export_receiving_key(&mut self, destination: PathBuf) -> Result<()> {
        let pending = self.state.load_pending_key()?;
        let key = match &pending {
            Some(key) => key.private_key.clone(),
            None => self.state.load_private_key()?,
        };
        write_key_export(&destination, &export_self_key(&key)?)?;
        if let Some(pending) = pending {
            self.state.mark_pending_key_exported(&pending.id)?;
        }
        self.refresh_key_custody()
    }
    fn confirm_receiving_key_backup(&mut self, generation_id: String) -> Result<()> {
        let pending = self
            .state
            .load_pending_key()?
            .context("no pending receiving key")?;
        if pending.id != generation_id || !pending.backup_exported {
            bail!(
                "export the pending receiving key and confirm its backup before uploading its public bundle"
            );
        }
        self.service()?.account().upload_key(&AccountKeyUpload {
            public_key: pending.public_key,
            fingerprint: pending.fingerprint,
            custody_mode: "self".into(),
            encrypted_private_key: None,
            current_password: None,
            replace: pending.replace,
        })?;
        self.state.save_private_key(&pending.private_key)?;
        self.state.clear_pending_key()?;
        self.refresh_key_custody()
    }
    fn receive_inbox(&mut self, id: String, item_ids: Option<Vec<String>>) -> Result<()> {
        let service = self.service()?;
        let private = self
            .state
            .load_private_key()
            .context("unlock or import the receiving key before receiving inbox deliveries")?;
        let situation = service.account().key_situation()?;
        let opened = service.account().open_inbox(&id, &private)?;
        let known_bundle = situation
            .active
            .as_ref()
            .is_some_and(|bundle| bundle.id == opened.key_bundle_id)
            || situation
                .historical_bundle_ids
                .contains(&opened.key_bundle_id);
        if !known_bundle {
            bail!("inbox delivery is not addressed to a known account key bundle");
        }
        let metadata = service.account().inbox_metadata(&id)?;
        if metadata.recipient_key.bundle.id != opened.key_bundle_id {
            bail!("inbox delivery metadata changed during authentication");
        }
        let working_key = open_recipient_key(&private, &metadata.recipient_key, &id)?;
        let cookie = Zeroizing::new(
            service
                .cookie_context()
                .context("inbox receive requires an authenticated session")?,
        );
        let output = self.config.home.join("inbox-receive").join(&id);
        ensure_private_dir(&output)?;
        let instance = self.config.server.url.clone();
        let receive_output = output.clone();
        let state_home = self.runtime.transfer_settings().state_home;
        let job = self
            .runtime
            .start_service_job(JobKind::Service, move |control| {
                let paths = if let Some(item_ids) = item_ids {
                    let checkpoint_id = protocol::background::prepare_inbox_download(
                        &instance,
                        &id,
                        &working_key,
                        &cookie,
                        &receive_output,
                        control,
                    )?;
                    protocol::background::select_download_items(
                        &state_home,
                        &checkpoint_id,
                        &item_ids,
                        filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(
                            &state_home,
                        ),
                    )?;
                    protocol::resume_inbox(
                        &checkpoint_id,
                        &instance,
                        &working_key,
                        &cookie,
                        control,
                    )?
                } else {
                    protocol::download_inbox(
                        &instance,
                        &id,
                        &working_key,
                        &cookie,
                        &receive_output,
                        control,
                    )?
                    .into_iter()
                    .map(|path| path.display().to_string())
                    .collect()
                };
                Ok(paths
                    .into_iter()
                    .map(|path| path.to_string())
                    .collect::<Vec<_>>())
            });
        self.receive_roots.insert(job.id(), output);
        Ok(())
    }
    fn change_instance(&mut self, url: String) -> Result<()> {
        self.account_id.set(0);
        if let Some((control, _)) = self.inbox_receiver.take() {
            control.cancel();
        }
        let url = normalize_server_url(&url)?;
        let config = Config::update(Some(self.config.home.clone()), |config| {
            config.server.url = url
        })?;
        self.state = LocalState::new(config.home.clone(), &config.server.url)?;
        self.service = Some(self.state.load_client()?);
        self.config = config;
        let mut snapshot = lock(&self.snapshot);
        snapshot.instance = InstanceSnapshot {
            url: self.config.server.url.clone(),
            connected: false,
            detail: None,
        };
        snapshot.policy = PolicySnapshot {
            origin: self.config.server.url.clone(),
            ..PolicySnapshot::default()
        };
        snapshot.account = AccountSnapshot::default();
        snapshot.inbox.clear();
        snapshot.history = HistorySnapshot::default();
        snapshot.contacts = None;
        snapshot.auto_receiving = false;
        snapshot.staged_inbox.clear();
        Ok(())
    }
    fn reload_settings(&mut self) -> Result<()> {
        // Load again under the configuration layer so malformed external edits
        // never replace the live future-job policy.
        let config = Config::load(Some(self.config.home.clone()))?;
        let reload = self.runtime.reload_settings(
            ApplicationSettings::new(transfer_settings(&config), scheduler_limits(&config)?)
                .with_ice_override(config.native_ice_override()),
        )?;
        self.config = config;
        self.restore_saved()?;
        let mut snapshot = lock(&self.snapshot);
        snapshot.settings.relay_only = self.config.webrtc.relay_only;
        snapshot.settings.restart_required = reload.restart_required;
        if !reload.restart_required {
            snapshot.settings.memory_limit_mib = self.config.transfers.memory_limit_mib;
            snapshot.settings.max_concurrency = self.config.transfers.max_concurrency;
        }
        Ok(())
    }
    fn refresh_inbox(&mut self) -> Result<()> {
        let inbox = self.service()?.account().inbox()?;
        lock(&self.snapshot).inbox = inbox
            .into_iter()
            .map(|item| InboxItem {
                id: item.id,
                ciphertext_bytes: item.ciphertext_bytes,
                item_count: item.item_count,
                completed_at: item.completed_at,
                expires_at: item.expires_at,
                item_ids: Vec::new(),
            })
            .collect();
        Ok(())
    }

    fn refresh_history(
        &mut self,
        filter: filebeam_client_core::services::HistoryFilter,
        cursor: Option<&str>,
    ) -> Result<()> {
        let service = self.service()?;
        let result = service.account().session().and_then(|session| {
            self.set_account(session);
            service.history().list(&filter, cursor, 25)
        });
        let mut snapshot = lock(&self.snapshot);
        snapshot.history.filter = filter;
        snapshot.history.revision += 1;
        match result {
            Ok(page) => {
                snapshot.history.page = page;
                snapshot.history.error = None;
                Ok(())
            }
            Err(error) => {
                snapshot.history.page = Default::default();
                snapshot.history.error = Some(format!("{error:#}"));
                Err(error)
            }
        }
    }

    fn finish_history_action(&mut self, result: Result<()>) -> Result<()> {
        if let Err(error) = result {
            let mut snapshot = lock(&self.snapshot);
            snapshot.history.error = Some(format!("{error:#}"));
            snapshot.history.revision += 1;
            return Err(error);
        }
        let filter = lock(&self.snapshot).history.filter.clone();
        self.refresh_history(filter, None)
    }
    fn refresh_contacts(&mut self) -> Result<()> {
        let contacts = self.service()?.account().contacts()?;
        lock(&self.snapshot).contacts = Some(contacts);
        Ok(())
    }
    fn start_inbox_receiver(&mut self) -> Result<()> {
        if self
            .inbox_receiver
            .as_ref()
            .is_some_and(|(_, worker)| !worker.is_finished())
        {
            return Ok(());
        }
        let service = self.service()?.clone();
        let account_id = service.account().session()?.id;
        let root = self.config.home.join("transfers/inbox-staging");
        let secrets =
            filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(&root);
        let receiver = filebeam_client_core::services::inbox_receiver::InboxReceiver::open(
            &root,
            &service.instance(),
            account_id,
            secrets.clone(),
        )?;
        let state = receiver.state()?;
        {
            let mut snapshot = lock(&self.snapshot);
            snapshot.auto_receiving = state.enabled;
            snapshot.staged_inbox = state.entries;
        }
        drop(receiver);
        if !state.enabled {
            return Ok(());
        }
        let (sender, _) = mpsc::channel();
        let control = Control::new(self.runtime.transfer_settings(), sender);
        let worker_control = control.clone();
        let snapshot = self.snapshot.clone();
        let worker = thread::spawn(move || {
            while worker_control.check().is_ok() {
                let result = filebeam_client_core::services::inbox_receiver::InboxReceiver::open(
                    &root,
                    &service.instance(),
                    account_id,
                    secrets.clone(),
                )
                .and_then(|receiver| receiver.sweep(&service, &worker_control));
                if let Ok(state) = result {
                    let mut snapshot = lock(&snapshot);
                    if worker_control.check().is_ok()
                        && snapshot.instance.url == service.instance()
                        && snapshot.account.authenticated
                    {
                        snapshot.staged_inbox = state.entries;
                    }
                }
                for _ in 0..60 {
                    if worker_control.check().is_err() {
                        return;
                    }
                    thread::sleep(Duration::from_secs(1));
                }
            }
        });
        self.inbox_receiver = Some((control, worker));
        Ok(())
    }
    fn inspect_inbox(&mut self, id: &str) -> Result<()> {
        let metadata = self.service()?.account().inbox_metadata(id)?;
        let mut snapshot = lock(&self.snapshot);
        let item = snapshot
            .inbox
            .iter_mut()
            .find(|item| item.id == id)
            .context("inbox item is no longer available")?;
        item.item_ids = metadata.items.into_iter().map(|item| item.id).collect();
        Ok(())
    }
    fn login(&mut self, email: String, password: String, remember: bool) -> Result<()> {
        if let Some((control, _)) = self.inbox_receiver.take() {
            control.cancel();
        }
        let service = ServiceClient::new(&self.config.server.url)?;
        let session = service
            .account()
            .login(&email, &Zeroizing::new(password), remember)?;
        if remember {
            self.state.save_session(&service)?;
        } else {
            self.state.clear_session()?;
        }
        self.service = Some(service);
        self.set_account(session);
        self.refresh_policy();
        let _ = self.refresh_contacts();
        self.start_inbox_receiver()?;
        Ok(())
    }
    fn register(
        &mut self,
        username: String,
        name: Option<String>,
        email: String,
        password: String,
    ) -> Result<()> {
        if let Some((control, _)) = self.inbox_receiver.take() {
            control.cancel();
        }
        let service = ServiceClient::new(&self.config.server.url)?;
        let session = service.account().register(
            &username,
            name.as_deref(),
            &email,
            &Zeroizing::new(password),
        )?;
        self.service = Some(service);
        self.set_account(session);
        self.refresh_policy();
        Ok(())
    }
    fn inspect_invitation(&mut self, token: &str) -> Result<()> {
        let invitation = self.service()?.account().invitation(token)?;
        lock(&self.snapshot).invitation = Some(InvitationSnapshot {
            token: token.into(),
            email: invitation.email,
            expires_at: invitation.expires_at,
        });
        Ok(())
    }
    fn accept_invitation(
        &mut self,
        token: String,
        username: String,
        name: Option<String>,
        email: String,
        password: String,
    ) -> Result<()> {
        let service = ServiceClient::new(&self.config.server.url)?;
        let session = service.account().accept_invitation(
            &token,
            &username,
            name.as_deref(),
            &email,
            &Zeroizing::new(password),
        )?;
        self.state.save_session(&service)?;
        self.service = Some(service);
        self.set_account(session);
        self.refresh_policy();
        lock(&self.snapshot).invitation = None;
        Ok(())
    }
    fn logout(&mut self) -> Result<()> {
        self.account_id.set(0);
        if let Some((control, _)) = self.inbox_receiver.take() {
            control.cancel();
        }
        self.service()?.account().logout()?;
        self.state.clear_session()?;
        self.service = None;
        let mut snapshot = lock(&self.snapshot);
        snapshot.history = HistorySnapshot::default();
        snapshot.account = AccountSnapshot::default();
        snapshot.contacts = None;
        snapshot.auto_receiving = false;
        snapshot.staged_inbox.clear();
        Ok(())
    }
    fn set_account(&self, session: filebeam_client_core::services::AccountSession) {
        self.account_id.set(session.id);
        let mut snapshot = lock(&self.snapshot);
        if snapshot.account.email.as_deref() != Some(session.email.as_str()) {
            snapshot.history = HistorySnapshot::default();
        }
        snapshot.account = AccountSnapshot {
            authenticated: true,
            email: Some(session.email),
            username: session.username,
            profile_url: session.profile_url,
            email_verified: session.email_verified_at.is_some(),
            inbox_enabled: session.inbox_enabled,
            notification_channel: Some(if session.notification_channel == "mail" {
                NotificationChannel::Mail
            } else {
                NotificationChannel::Database
            }),
            key_custody: KeyCustody::Unknown,
        };
    }
    fn publish(&mut self) {
        let snapshots = self.runtime.snapshots();
        let active_job_ids = snapshots
            .iter()
            .map(|entry| entry.id)
            .collect::<std::collections::HashSet<_>>();
        let mut jobs = Vec::new();
        let mut prompts = Vec::new();
        let mut results = Vec::new();
        for mut entry in snapshots {
            // A preparation and its download can share a checkpoint. Actions and
            // retained selections address the process-owned job, not that record.
            let id = entry.id.0.to_string();
            let separate_key = if self.include_keys.get(&entry.id) == Some(&false)
                || self
                    .note_jobs
                    .get(&entry.id)
                    .is_some_and(|note| !note.include_key)
            {
                entry.job.share_url.clone().and_then(|link| {
                    filebeam_client_core::link_presentation::split_share_link(
                        &self.config.server.url,
                        &link,
                    )
                    .ok()
                    .map(|presentation| {
                        entry.job.share_url = Some(presentation.link);
                        presentation.separate_key
                    })
                })
            } else {
                None
            };
            if let Some(prompt) = entry.job.prompt.clone() {
                prompts.push(PendingPrompt {
                    transfer_id: id.clone(),
                    id: prompt.id,
                    kind: match prompt.kind {
                        filebeam_client_core::PromptType::ShareKey => PromptKind::ShareKey,
                        filebeam_client_core::PromptType::Password => PromptKind::Password,
                        filebeam_client_core::PromptType::PeerConsent => PromptKind::PeerConsent,
                        filebeam_client_core::PromptType::Directory => PromptKind::Directory,
                        filebeam_client_core::PromptType::ShareReady => PromptKind::ShareReady,
                    },
                    peer: prompt.peer,
                    directory: prompt.directory.map(|d| DirectoryChoice {
                        files: d.files,
                        bytes: d.bytes,
                        maximum_files: d.maximum_files,
                    }),
                });
            }
            if let Some(root) = self.receive_roots.get(&entry.id)
                && let Some(result) = verified_result(id.clone(), root, &entry.export)
            {
                results.push(result);
            }
            let note_actions = self.note_jobs.get(&entry.id).and_then(|note| {
                lock(&note.id).as_deref().map(|note_id| {
                    NoteManagementStore::for_root(self.runtime.transfer_settings().state_home)
                        .availability(note_id)
                })
            });
            let title = self
                .prepared_receive_titles
                .get(&entry.id)
                .map(String::as_str);
            let mut job = map_job(id, entry, title);
            job.separate_key = separate_key;
            job.cli_command = job.share_url.as_deref().and_then(|link| {
                filebeam_client_core::link_presentation::format_download_with_cli(link).ok()
            });
            if let Some(availability) = note_actions {
                job.direction = TransferDirection::Note;
                job.capabilities.can_revoke = availability.can_revoke;
                job.capabilities.can_end_live =
                    availability.can_end_live && job.state == TransferState::Running;
            }
            jobs.push(job);
        }
        let mut snapshot = lock(&self.snapshot);
        snapshot.jobs = jobs;
        snapshot.pending_prompts = prompts;
        snapshot.verified_results = results;
        snapshot.notes = lock(&self.note_memory).snapshots.clone();
        let notes = snapshot
            .receive_previews
            .iter()
            .filter(|preview| preview.kind == ReceiveKind::Note)
            .cloned()
            .collect::<Vec<_>>();
        snapshot.receive_previews = notes;
        snapshot
            .receive_previews
            .extend(
                lock(&self.prepared_receives)
                    .iter()
                    .map(|(operation_id, prepared)| ReceivePreview {
                        attached_note: prepared.attached_note.clone(),
                        operation_id: Some(operation_id.clone()),
                        link: prepared.link.clone(),
                        kind: ReceiveKind::File,
                        password_required: false,
                        burn_on_read: false,
                        items: prepared.items.clone(),
                    }),
            );
        self.prepared_receive_titles
            .retain(|id, _| active_job_ids.contains(id));
    }
    fn message(&mut self, operation: &str, error: anyhow::Error) {
        let mut snapshot = lock(&self.snapshot);
        record_message(&mut snapshot, &mut self.next_message_id, operation, error);
    }
    fn complete(&self, operation: &str) {
        if operation != "command" {
            lock(&self.snapshot).operations.push(OperationStatus {
                operation: operation.into(),
                detail: "completed".into(),
            });
        }
    }
}

fn record_message(
    snapshot: &mut DesktopSnapshot,
    next_message_id: &mut u64,
    operation: &str,
    error: anyhow::Error,
) {
    *next_message_id += 1;
    snapshot.messages.push(ClientMessage {
        id: *next_message_id,
        operation: operation.into(),
        error: ClientError {
            code: classify(&error),
            detail: sanitize(&error),
        },
    });
    if snapshot.messages.len() > MESSAGE_HISTORY_CAPACITY {
        snapshot.messages.remove(0);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some((control, _)) = self.inbox_receiver.take() {
            control.cancel();
        }
    }
}

fn initial_snapshot(config: &Config) -> DesktopSnapshot {
    DesktopSnapshot {
        instance: InstanceSnapshot {
            url: config.server.url.clone(),
            connected: false,
            detail: None,
        },
        policy: PolicySnapshot {
            origin: config.server.url.clone(),
            ..PolicySnapshot::default()
        },
        settings: SettingsSnapshot {
            memory_limit_mib: config.transfers.memory_limit_mib,
            max_concurrency: config.transfers.max_concurrency,
            relay_only: config.webrtc.relay_only,
            auto_update: config.updates.auto_update,
            restart_required: false,
        },
        ..DesktopSnapshot::default()
    }
}
fn policy_snapshot(origin: &str, info: protocol::Info) -> PolicySnapshot {
    let driver = |name: &str| {
        let limits = filebeam_transfer::capabilities::select_driver_limits(
            name,
            &info.transport_limits,
            info.maximum_transfer_bytes,
            info.maximum_file_count,
        );
        DriverPolicy {
            enabled: info.enabled_drivers.iter().any(|enabled| enabled == name),
            maximum_ciphertext_bytes: limits.maximum_transfer_bytes,
            maximum_file_count: limits.maximum_file_count,
            maximum_note_bytes: limits.maximum_note_bytes,
        }
    };
    PolicySnapshot {
        origin: origin.into(),
        availability: PolicyAvailability::Available,
        reason: None,
        anonymous_uploads: info.anonymous_uploads_enabled,
        default_transport: if info.default_driver == "webrtc" {
            SendTransport::Live
        } else {
            SendTransport::Http
        },
        http: driver("http"),
        webrtc: driver("webrtc"),
        default_retention_hours: Some(info.file_retention_hours),
        retention_options_hours: info.file_retention_options,
    }
}
fn is_auth_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.to_string().contains("401") || cause.to_string().contains("unauthorized")
    })
}
fn journal_store(
    home: &Path,
    create: bool,
) -> Result<Option<filebeam_transfer_native::checkpoint::Store>> {
    let root = home.join("desktop-journal");
    let id = "transfer-center";
    if create {
        if root.join(id).exists() {
            Ok(Some(filebeam_transfer_native::checkpoint::Store::open(
                &root, id,
            )?))
        } else {
            Ok(Some(filebeam_transfer_native::checkpoint::Store::create(
                &root, id,
            )?))
        }
    } else if root.join(id).exists() {
        Ok(Some(filebeam_transfer_native::checkpoint::Store::open(
            &root, id,
        )?))
    } else {
        Ok(None)
    }
}
fn load_journal(home: &Path) -> Result<DesktopJournal> {
    journal_store(home, false)?
        .map(|store| Ok(store.load_named("transfer-center")?.unwrap_or_default()))
        .unwrap_or_else(|| Ok(DesktopJournal::default()))
}
fn save_journal(home: &Path, journal: &DesktopJournal) -> Result<()> {
    journal_store(home, true)?
        .context("open desktop transfer journal")?
        .save_named("transfer-center", journal)
}
fn transfer_settings(config: &Config) -> TransferSettings {
    TransferSettings {
        state_home: config.home.join("transfers"),
        max_concurrency: config.transfers.max_concurrency,
        memory_budget: config.transfers.memory_limit_mib * 1024 * 1024,
        client_user_agent: Some(format!("filebeam-desktop/{}", env!("CARGO_PKG_VERSION"))),
        webrtc_relay_only: config.webrtc.relay_only,
        checkpoint_secret_store: None,
        source_resolver: None,
    }
}
fn scheduler_limits(config: &Config) -> Result<SchedulerLimits> {
    let workers = config.transfers.max_concurrency.unwrap_or(4) as usize;
    let memory_bytes = config
        .transfers
        .memory_limit_mib
        .checked_mul(1024 * 1024)
        .and_then(|buffers| buffers.checked_add(RUNTIME_ALLOWANCE_BYTES))
        .and_then(|per_job| per_job.checked_mul(workers as u64))
        .and_then(|active| active.checked_add(TRANSIENT_MEMORY_ALLOWANCE_BYTES))
        .context("desktop scheduler memory budget is too large")?;
    Ok(SchedulerLimits {
        workers,
        memory_bytes,
    })
}
fn lock<T>(value: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    value.lock().unwrap_or_else(|error| error.into_inner())
}
fn ensure_private_dir(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        bail!("private receive directory must be absolute");
    }
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
    Ok(())
}
fn write_key_export(destination: &Path, export: &str) -> Result<()> {
    if !destination.is_absolute() {
        bail!("receiving-key export destination must be absolute");
    }
    let parent = destination
        .parent()
        .context("receiving-key export destination has no parent")?;
    if fs::symlink_metadata(parent)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        bail!("receiving-key export parent must not be a symlink");
    }
    if fs::symlink_metadata(destination).is_ok() {
        bail!("receiving-key export destination already exists");
    }
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true).read(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(destination)
        .with_context(|| format!("create {}", destination.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(export.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
fn verified_result(
    id: String,
    private_path: &Path,
    export: &ExportState,
) -> Option<VerifiedResult> {
    let (paths, destination, export_error) = match export {
        ExportState::NotVerified => return None,
        ExportState::AwaitingDestination { paths } => (paths, None, None),
        ExportState::ExportFailed { paths, message } => (
            paths,
            None,
            Some(ClientError {
                code: ClientErrorCode::Storage,
                detail: message.clone(),
            }),
        ),
        ExportState::Exported { destination, paths } => (paths, Some(destination), None),
    };
    let verified_paths = paths.iter().map(PathBuf::from).collect::<Vec<_>>();
    Some(VerifiedResult {
        transfer_id: id,
        private_path: private_path.to_owned(),
        exported_paths: destination
            .into_iter()
            .flat_map(|destination| {
                paths.iter().filter_map(move |path| {
                    Path::new(path)
                        .file_name()
                        .map(|name| destination.join(name))
                })
            })
            .collect(),
        verified_paths,
        export_error,
    })
}
fn export_verified(paths: &[String], private_root: &Path, destination: &Path) -> Result<()> {
    let private_root = private_root
        .canonicalize()
        .context("validate private receive root")?;
    let destination = destination
        .canonicalize()
        .context("validate export destination")?;
    let staging = destination.join(format!(".filebeam-export-{}", std::process::id()));
    if staging.exists() {
        bail!("export staging directory already exists");
    }
    fs::create_dir(&staging)?;
    let mut committed = Vec::new();
    let result = (|| {
        for source in paths {
            let source = PathBuf::from(source)
                .canonicalize()
                .context("validate verified source")?;
            if !source.starts_with(&private_root) || !source.is_file() {
                bail!("verified source is outside the private receive directory");
            }
            let name = source
                .file_name()
                .context("verified source has no file name")?;
            let target = destination.join(name);
            if target.exists() {
                bail!("export target already exists: {}", target.display());
            }
            fs::copy(&source, staging.join(name))?;
        }
        for entry in fs::read_dir(&staging)? {
            let entry = entry?;
            let target = destination.join(entry.file_name());
            fs::rename(entry.path(), &target)?;
            committed.push(target);
        }
        Ok(())
    })();
    if result.is_err() {
        for target in committed {
            let _ = fs::remove_file(target);
        }
    }
    let _ = fs::remove_dir_all(&staging);
    result
}
fn classify(error: &anyhow::Error) -> ClientErrorCode {
    let text = error.to_string().to_ascii_lowercase();
    if text.contains("queue") || text.contains("scheduler") || text.contains("busy") {
        ClientErrorCode::Busy
    } else if text.contains("invalid") || text.contains("must ") || text.contains("select") {
        ClientErrorCode::InvalidInput
    } else if text.contains("network") || text.contains("connection") || text.contains("timeout") {
        ClientErrorCode::Network
    } else if text.contains("decrypt") || text.contains("encrypt") || text.contains("key") {
        ClientErrorCode::Crypto
    } else if text.contains("read ")
        || text.contains("write ")
        || text.contains("directory")
        || text.contains("export")
    {
        ClientErrorCode::Storage
    } else {
        ClientErrorCode::Internal
    }
}
fn sanitize(error: &anyhow::Error) -> String {
    let detail = error.to_string();
    if detail.to_ascii_lowercase().contains("password")
        || detail.to_ascii_lowercase().contains("cookie")
        || detail.to_ascii_lowercase().contains("token")
    {
        "operation failed; sensitive detail was withheld".into()
    } else {
        detail
    }
}
fn map_job(
    id: String,
    entry: filebeam_client_core::ApplicationJobSnapshot,
    explicit_title: Option<&str>,
) -> JobSnapshot {
    let state = match entry.job.state {
        filebeam_client_core::JobState::Running => TransferState::Running,
        filebeam_client_core::JobState::Pausing => TransferState::PauseRequested,
        filebeam_client_core::JobState::Paused => TransferState::Paused,
        filebeam_client_core::JobState::Complete => TransferState::Complete,
        filebeam_client_core::JobState::Failed => TransferState::Failed,
    };
    JobSnapshot {
        id,
        direction: match entry.kind {
            JobKind::Upload => TransferDirection::Send,
            JobKind::Download | JobKind::Resume => TransferDirection::Receive,
            JobKind::Note | JobKind::LiveNote => TransferDirection::Note,
            JobKind::Service | JobKind::Revoke | JobKind::EndLive => TransferDirection::Service,
        },
        state,
        progress: TransferProgress {
            phase: filebeam_client_core::phase_name(entry.job.progress.phase).into(),
            name: transfer_title(
                explicit_title,
                Some(&entry.job.progress.name),
                entry.job.progress.files,
            ),
            item_index: entry.job.progress.index,
            item_count: entry.job.progress.files,
            total_bytes: entry.job.progress.total,
            completed_bytes: entry.job.progress.done,
            committed_bytes: entry.job.progress.committed,
            wire_bytes: entry.job.progress.wire_bytes,
        },
        share_url: entry.job.share_url,
        separate_key: None,
        cli_command: None,
        checkpoint_id: entry.job.checkpoint_id,
        origin: entry.origin,
        error: entry.job.error.map(|detail| ClientError {
            code: ClientErrorCode::Internal,
            detail: sanitize(&anyhow::anyhow!(detail)),
        }),
        peer_warning: entry.job.peer_warning,
        capabilities: TransferCapabilities {
            can_pause: entry.actions.pause,
            can_resume: entry.actions.resume,
            can_end_live: entry.actions.end_live,
            can_revoke: entry.actions.revoke_remote,
            can_discard: entry.actions.discard,
            can_export: entry.actions.export,
            can_retry_export: entry.actions.retry_export,
        },
    }
}

/// Progress names identify the active item, rather than the transfer as a whole.
/// Keep an explicit title intact; otherwise derive a stable transfer title only
/// from manifest data the worker has actually resolved.
fn transfer_title(explicit: Option<&str>, item_name: Option<&str>, item_count: usize) -> String {
    if let Some(name) = explicit.filter(|name| !name.trim().is_empty()) {
        return name.into();
    }
    match item_count {
        0 => "Preparing transfer".into(),
        1 => item_name
            .and_then(|name| name.rsplit(['/', '\\']).next())
            .filter(|name| !name.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| "Transfer".into()),
        count => format!("{count} files"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[test]
    fn policy_uses_driver_specific_caps_and_real_retention_choices() {
        let policy = policy_snapshot(
            "https://example.test",
            protocol::Info {
                name: "test".into(),
                file_retention_hours: 24,
                file_retention_options: vec![6, 24],
                anonymous_uploads_enabled: false,
                enabled_drivers: vec!["http".into(), "webrtc".into()],
                default_driver: "webrtc".into(),
                chunk_bytes: 1024,
                maximum_transfer_bytes: Some(2 * 1024 * 1024 * 1024),
                maximum_file_count: Some(20),
                transport_limits: HashMap::from([
                    (
                        "http".into(),
                        filebeam_transfer::capabilities::DriverLimits {
                            maximum_transfer_bytes: Some(100),
                            maximum_file_count: Some(2),
                            maximum_note_bytes: Some(10),
                        },
                    ),
                    (
                        "webrtc".into(),
                        filebeam_transfer::capabilities::DriverLimits {
                            maximum_transfer_bytes: Some(200),
                            maximum_file_count: Some(3),
                            maximum_note_bytes: Some(20),
                        },
                    ),
                ]),
            },
        );
        assert!(!policy.anonymous_uploads);
        assert!(policy.default_transport == SendTransport::Live);
        assert_eq!(policy.http.maximum_ciphertext_bytes, Some(100));
        assert_eq!(policy.webrtc.maximum_file_count, Some(3));
        assert_eq!(policy.http.maximum_note_bytes, Some(10));
        assert_eq!(policy.retention_options_hours, vec![6, 24]);
    }

    #[test]
    fn empty_retention_options_do_not_create_custom_choices() {
        let mut info = protocol::Info::fixture();
        info.file_retention_options.clear();
        let policy = policy_snapshot("https://example.test", info);
        assert!(policy.retention_options_hours.is_empty());
        assert_eq!(policy.default_retention_hours, Some(24));
    }
    #[test]
    fn rejects_relative_private_receive_root() {
        assert!(ensure_private_dir(Path::new("relative")).is_err());
    }
    #[test]
    fn secret_errors_are_sanitized() {
        assert_eq!(
            sanitize(&anyhow::anyhow!("password abc")),
            "operation failed; sensitive detail was withheld"
        );
    }
    #[test]
    fn message_events_are_monotonic_and_history_is_bounded() {
        let mut snapshot = DesktopSnapshot::default();
        let mut next_id = 0;
        for _ in 0..=MESSAGE_HISTORY_CAPACITY {
            record_message(
                &mut snapshot,
                &mut next_id,
                "request",
                anyhow::anyhow!("native account request returned 404"),
            );
        }

        assert_eq!(snapshot.messages.len(), MESSAGE_HISTORY_CAPACITY);
        assert_eq!(snapshot.messages.first().unwrap().id, 2);
        assert_eq!(snapshot.messages.last().unwrap().id, 257);
        assert_eq!(
            snapshot.messages[0].error.detail,
            snapshot.messages[1].error.detail
        );
    }
    #[test]
    fn split_receipt_keeps_the_link_keyless_and_the_key_copyable() {
        let presentation = filebeam_client_core::link_presentation::split_share_link(
            "https://files.example",
            "https://files.example/01ARZ3NDEKTSV4RRFFQ69G5FAV#k=v1.AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE",
        )
        .unwrap();
        assert!(!presentation.link.contains('#'));
        assert_eq!(
            presentation.separate_key,
            "v1.AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE"
        );
    }
    #[test]
    fn transfer_titles_preserve_explicit_names_and_fallback_to_manifest_data() {
        assert_eq!(
            transfer_title(Some("  Quarterly report  "), Some("ignored.txt"), 3),
            "  Quarterly report  "
        );
        assert_eq!(
            transfer_title(Some(" \t "), Some("ignored.txt"), 1),
            "ignored.txt"
        );
        let filename = "na\u{00ef}ve-\u{03c0}.txt";
        let source = format!("folder/{filename}");
        assert_eq!(transfer_title(None, Some(&source), 1), filename);
        assert_eq!(transfer_title(None, Some("first.txt"), 4), "4 files");
        assert_eq!(transfer_title(None, None, 0), "Preparing transfer");
    }
    #[test]
    fn job_projection_uses_the_resolved_manifest_count() {
        let entry = filebeam_client_core::ApplicationJobSnapshot {
            id: JobId(7),
            kind: JobKind::Upload,
            origin: None,
            job: filebeam_client_core::JobSnapshot {
                state: filebeam_client_core::JobState::Running,
                checkpoint_id: None,
                progress: filebeam_client_core::control::Progress {
                    name: "folder/first.txt".into(),
                    files: 3,
                    ..Default::default()
                },
                prompt: None,
                share_url: None,
                results: Vec::new(),
                error: None,
                error_kind: None,
                peer_warning: None,
                secret_retry: None,
            },
            export: ExportState::NotVerified,
            actions: filebeam_client_core::ActionCapabilities::default(),
        };

        assert_eq!(map_job("7".into(), entry, None).progress.name, "3 files");
    }
    #[test]
    fn verified_result_keeps_private_and_exported_paths_distinct() {
        let private = Path::new("/private");
        let paths = vec!["/private/alpha.bin".into()];
        let awaiting = verified_result(
            "1".into(),
            private,
            &ExportState::AwaitingDestination {
                paths: paths.clone(),
            },
        )
        .unwrap();
        assert_eq!(
            awaiting.verified_paths,
            vec![PathBuf::from("/private/alpha.bin")]
        );
        assert!(awaiting.exported_paths.is_empty());
        let failed = verified_result(
            "1".into(),
            private,
            &ExportState::ExportFailed {
                paths: paths.clone(),
                message: "destination is unavailable".into(),
            },
        )
        .unwrap();
        assert!(failed.exported_paths.is_empty());
        assert!(failed.export_error.is_some());
        let exported = verified_result(
            "1".into(),
            private,
            &ExportState::Exported {
                destination: PathBuf::from("/destination"),
                paths,
            },
        )
        .unwrap();
        assert_eq!(
            exported.exported_paths,
            vec![PathBuf::from("/destination/alpha.bin")]
        );
        assert!(exported.export_error.is_none());
    }
    #[test]
    fn desktop_client_reopens_authenticated_paused_checkpoint() {
        let home = tempfile::tempdir().unwrap();
        let config = Config::load(Some(home.path().to_owned())).unwrap();
        let checkpoint = "11111111-1111-4111-8111-111111111111".to_owned();
        let store = filebeam_transfer_native::checkpoint::Store::create(
            &config.home.join("transfers"),
            &checkpoint,
        )
        .unwrap();
        store
            .save(&serde_json::json!({
                "version": 1, "id": checkpoint, "direction": "download", "state": "paused",
                "done": 0, "total": 1, "driver": "http", "items": []
            }))
            .unwrap();
        drop(store);
        for _ in 0..2 {
            let client = DesktopClient::new(Some(config.home.clone())).unwrap();
            let until = Instant::now() + Duration::from_secs(2);
            loop {
                if let Some(job) = client
                    .snapshot()
                    .jobs
                    .into_iter()
                    .find(|job| job.checkpoint_id.as_deref() == Some(&checkpoint))
                {
                    assert!(matches!(job.state, TransferState::Paused));
                    assert!(job.capabilities.can_resume);
                    break;
                }
                assert!(Instant::now() < until, "recovered row was not published");
                std::thread::sleep(Duration::from_millis(10));
            }
            drop(client);
        }
    }
}
