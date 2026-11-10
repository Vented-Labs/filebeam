use crate::{
    DeviceCredentialStoreCallback, DeviceKeySignerCallback, NativeRuntime, Result, client::origin,
    error::registry_operation, invalid, operation,
};
use base64::Engine as _;
use filebeam_client_core::device_registry::{
    AccountId, DeploymentId, DeviceGeneration, DeviceId, DeviceRegistry as DurableDeviceRegistry,
    Origin, RegistryResponse, RegistryScope,
};
use filebeam_client_core::services as core;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

#[derive(Clone, uniffi::Record)]
pub struct NoteMetadata {
    pub id: String,
    pub status: String,
    pub burn_on_read: bool,
    pub encrypted_manifest: Option<String>,
}

#[derive(Clone, Copy, uniffi::Enum)]
pub enum BurnResult {
    Consumed,
    AlreadyConsumed,
}

#[derive(Clone, uniffi::Record)]
pub struct TurboItemAvailability {
    pub id: String,
    pub ready_chunks: u64,
    pub uploaded_chunks: u64,
}

#[derive(Clone, uniffi::Record)]
pub struct TurboAvailability {
    pub status: String,
    pub progress: u8,
    pub uploader_status: String,
    pub expires_at: String,
    pub items: Vec<TurboItemAvailability>,
}

#[derive(Clone, uniffi::Record)]
pub struct TurboDownloadSession {
    pub id: String,
    pub token: String,
}

#[derive(Clone, uniffi::Record)]
pub struct AccountSession {
    pub id: u64,
    pub name: String,
    pub username: Option<String>,
    pub email: String,
    pub email_verified_at: Option<String>,
    pub profile_url: Option<String>,
    pub inbox_enabled: bool,
    pub username_routing_enabled: bool,
    pub notification_channel: String,
}

#[derive(Clone, uniffi::Record)]
pub struct AccountKeyBundle {
    pub id: u64,
    pub user_id: u64,
    pub version: u32,
    pub public_key: String,
    pub fingerprint: String,
    pub custody_mode: String,
    pub encrypted_private_key: Option<String>,
    pub is_active: bool,
}

#[derive(Clone, uniffi::Record)]
pub struct AccountKeySituation {
    pub active_bundle_id: Option<u64>,
    pub active_custody_mode: Option<String>,
    pub historical_bundle_ids: Vec<u64>,
    pub replacement_acknowledgement_required: bool,
}

#[derive(Clone, uniffi::Record)]
pub struct Recipient {
    pub id: u64,
    pub username: String,
    pub public_key: String,
    pub account_key_bundle_id: u64,
    pub version: u32,
    pub fingerprint: String,
}

#[derive(Clone, uniffi::Record)]
pub struct InboxRecipientKey {
    pub bundle: AccountKeyBundle,
    pub encrypted_key: String,
}

#[derive(Clone, uniffi::Record)]
pub struct InboxMetadata {
    pub id: String,
    pub encrypted_manifest: Option<String>,
    pub recipient_key: InboxRecipientKey,
}

#[derive(Clone, uniffi::Record)]
pub struct HistoryFilter {
    pub status: Option<String>,
    pub kind: Option<String>,
    pub driver: Option<String>,
}

#[derive(Clone, uniffi::Record)]
pub struct HistoryEntry {
    pub id: String,
    pub kind: String,
    pub driver: String,
    pub delivery: String,
    pub status: String,
    pub item_count: u64,
    pub ciphertext_bytes: u64,
    pub created_at: String,
    pub expires_at: String,
    pub removed_at: Option<String>,
    pub retention_hours: u64,
    pub maximum_retention_hours: u64,
    pub burn_on_read: bool,
    pub can_delete: bool,
    pub can_extend: bool,
}

#[derive(Clone, uniffi::Record)]
pub struct HistoryPage {
    pub entries: Vec<HistoryEntry>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, uniffi::Record)]
pub struct InboxTransfer {
    pub id: String,
    pub ciphertext_bytes: u64,
    pub item_count: u64,
    pub completed_at: String,
    pub expires_at: String,
}

#[derive(Clone, uniffi::Record)]
pub struct OpenedInbox {
    pub transfer_id: String,
    pub key_bundle_id: u64,
    pub filenames: Vec<String>,
}

#[derive(Clone, uniffi::Record)]
pub struct AccountKeyUpload {
    pub public_key: String,
    pub fingerprint: String,
    pub custody_mode: String,
    pub encrypted_private_key: Option<String>,
    pub current_password: Option<String>,
    pub replace: bool,
}

#[derive(Clone, uniffi::Record)]
pub struct GeneratedAccountKey {
    pub private_key: Vec<u8>,
    pub public_key: String,
    pub fingerprint: String,
}
#[derive(Clone, uniffi::Record)]
pub struct NativeInvitation {
    pub email: Option<String>,
    pub expires_at: Option<String>,
}

/// Account-visible registration fields. `public_key` is an Ed25519 public key;
/// it is included because it is required for candidate approval verification.
#[derive(Clone, uniffi::Record)]
pub struct RegisteredDevice {
    pub id: String,
    pub display_name: String,
    pub platform: String,
    pub public_key: String,
    pub key_version: u32,
    pub status: String,
    pub generation: u64,
    pub last_seen_at: Option<String>,
    pub presence_expires_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Clone, uniffi::Record)]
pub struct DeviceRegistry {
    pub revision: u64,
    pub generation: u64,
    pub devices: Vec<RegisteredDevice>,
}

/// A short-lived, one-use enrollment challenge. It is the only enrollment
/// material supplied to the platform key signer.
#[derive(Clone, uniffi::Record)]
pub struct DeviceEnrollmentChallenge {
    pub device: RegisteredDevice,
    pub challenge: String,
    pub expires_at: Option<String>,
    pub approval_context: DeviceApprovalPayload,
}

/// One-use QR/paste ceremony data from the pending candidate. It is not a peer
/// pin: the backend still checks its current pending registration before use.
#[derive(Clone, uniffi::Record)]
pub struct DeviceApprovalPayload {
    pub instance_origin: String,
    pub deployment_id: String,
    pub account_id: u64,
    pub generation: u64,
    pub candidate_device_id: String,
    pub candidate_key_version: u32,
    pub candidate_public_key: String,
    pub challenge_sha256: String,
    pub expires_at_unix: u64,
    pub approver_device_id: String,
    pub approver_key_version: u32,
}

#[derive(Clone, uniffi::Record)]
pub struct DeviceApprovalEvidence {
    pub version: u8,
    pub approver_device_id: String,
    pub approver_key_version: u32,
    pub approver_public_key: String,
    pub approval_signature: String,
}

#[derive(Clone, uniffi::Record)]
pub struct DeviceCredentialRedemption {
    pub device: RegisteredDevice,
    pub approval_evidence: Option<DeviceApprovalEvidence>,
}

/// Short display value for an already decoded Ed25519 public key. It is for
/// human comparison only and never substitutes for the full signed key.
#[uniffi::export]
pub fn device_key_fingerprint(public_key: String) -> Result<String> {
    let key = base64::engine::general_purpose::STANDARD
        .decode(public_key)
        .map_err(|_| invalid("Device public key is not base64"))?;
    if key.len() != 32 {
        return Err(invalid("Device public key must be 32 bytes"));
    }
    Ok(Sha256::digest(key)[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":"))
}

#[derive(uniffi::Object)]
pub struct NativeServices {
    pub(crate) inner: core::ServiceClient,
    pub(crate) runtime: Option<Arc<NativeRuntime>>,
    automatic: std::sync::Mutex<Option<filebeam_client_core::control::Control>>,
    account_id: std::sync::atomic::AtomicU64,
    device_signer: Option<Arc<dyn DeviceKeySignerCallback>>,
    pub(crate) device_credentials: Option<Arc<dyn DeviceCredentialStoreCallback>>,
}

#[uniffi::export]
impl NativeServices {
    #[uniffi::constructor]
    pub fn new(instance: String, allow_http: bool) -> Result<Self> {
        Ok(Self {
            inner: core::ServiceClient::new(&origin(&instance, allow_http)?).map_err(operation)?,
            runtime: None,
            automatic: std::sync::Mutex::new(None),
            account_id: std::sync::atomic::AtomicU64::new(0),
            device_signer: None,
            device_credentials: None,
        })
    }

    #[uniffi::constructor]
    pub fn new_with_cookie_context(
        instance: String,
        allow_http: bool,
        cookie_context: String,
    ) -> Result<Self> {
        let instance = origin(&instance, allow_http)?;
        Ok(Self {
            inner: core::ServiceClient::new_with_cookie_context(&instance, Some(&cookie_context))
                .map_err(operation)?,
            runtime: None,
            automatic: std::sync::Mutex::new(None),
            account_id: std::sync::atomic::AtomicU64::new(0),
            device_signer: None,
            device_credentials: None,
        })
    }

    #[uniffi::constructor]
    pub fn new_with_runtime(
        instance: String,
        allow_http: bool,
        runtime: Arc<NativeRuntime>,
    ) -> Result<Self> {
        Ok(Self {
            inner: core::ServiceClient::new(&origin(&instance, allow_http)?).map_err(operation)?,
            runtime: Some(runtime),
            automatic: std::sync::Mutex::new(None),
            account_id: std::sync::atomic::AtomicU64::new(0),
            device_signer: None,
            device_credentials: None,
        })
    }

    #[uniffi::constructor]
    pub fn new_with_runtime_cookie_context(
        instance: String,
        allow_http: bool,
        cookie_context: String,
        runtime: Arc<NativeRuntime>,
    ) -> Result<Self> {
        let instance = origin(&instance, allow_http)?;
        Ok(Self {
            inner: core::ServiceClient::new_with_cookie_context(&instance, Some(&cookie_context))
                .map_err(operation)?,
            runtime: Some(runtime),
            automatic: std::sync::Mutex::new(None),
            account_id: std::sync::atomic::AtomicU64::new(0),
            device_signer: None,
            device_credentials: None,
        })
    }

    /// Creates a cookie-backed account client with typed device signing and
    /// platform credential custody for registry operations.
    #[uniffi::constructor]
    pub fn new_with_device_callbacks(
        instance: String,
        allow_http: bool,
        cookie_context: String,
        device_signer: Arc<dyn DeviceKeySignerCallback>,
        device_credentials: Arc<dyn DeviceCredentialStoreCallback>,
    ) -> Result<Self> {
        let instance = origin(&instance, allow_http)?;
        Ok(Self {
            inner: core::ServiceClient::new_with_cookie_context(&instance, Some(&cookie_context))
                .map_err(operation)?,
            runtime: None,
            automatic: std::sync::Mutex::new(None),
            account_id: std::sync::atomic::AtomicU64::new(0),
            device_signer: Some(device_signer),
            device_credentials: Some(device_credentials),
        })
    }

    /// Cookie-backed registry control with the application's existing encrypted
    /// checkpoint store. This is required for durable local device pins.
    #[uniffi::constructor]
    pub fn new_with_runtime_device_callbacks(
        instance: String,
        allow_http: bool,
        cookie_context: String,
        runtime: Arc<NativeRuntime>,
        device_signer: Arc<dyn DeviceKeySignerCallback>,
        device_credentials: Arc<dyn DeviceCredentialStoreCallback>,
    ) -> Result<Self> {
        let instance = origin(&instance, allow_http)?;
        Ok(Self {
            inner: core::ServiceClient::new_with_cookie_context(&instance, Some(&cookie_context))
                .map_err(operation)?,
            runtime: Some(runtime),
            automatic: std::sync::Mutex::new(None),
            account_id: std::sync::atomic::AtomicU64::new(0),
            device_signer: Some(device_signer),
            device_credentials: Some(device_credentials),
        })
    }

    pub fn read_note(&self, transfer_id: String) -> Result<NoteMetadata> {
        let note = self.inner.notes().read(&transfer_id).map_err(operation)?;
        Ok(NoteMetadata {
            id: note.id,
            status: note.status,
            burn_on_read: note.burn_on_read,
            encrypted_manifest: note.encrypted_manifest,
        })
    }

    pub fn consume_note(&self, transfer_id: String, read_token: String) -> Result<BurnResult> {
        match self
            .inner
            .notes()
            .consume(&transfer_id, &read_token)
            .map_err(operation)?
        {
            core::BurnResult::Consumed => Ok(BurnResult::Consumed),
            core::BurnResult::AlreadyConsumed => Ok(BurnResult::AlreadyConsumed),
        }
    }

    pub fn turbo_availability(&self, transfer_id: String) -> Result<TurboAvailability> {
        let availability = self
            .inner
            .turbo()
            .availability(&transfer_id)
            .map_err(operation)?;
        Ok(TurboAvailability {
            status: availability.status,
            progress: availability.progress,
            uploader_status: availability.uploader_status,
            expires_at: availability.expires_at,
            items: availability
                .items
                .into_iter()
                .map(|item| TurboItemAvailability {
                    id: item.id,
                    ready_chunks: item.ready_chunks,
                    uploaded_chunks: item.uploaded_chunks,
                })
                .collect(),
        })
    }

    pub fn turbo_publish_descriptor(
        &self,
        transfer_id: String,
        upload_token: String,
        encrypted_descriptor: String,
    ) -> Result<()> {
        self.inner
            .turbo()
            .publish_descriptor(&transfer_id, &upload_token, &encrypted_descriptor)
            .map_err(operation)
    }

    pub fn turbo_heartbeat(&self, transfer_id: String, upload_token: String) -> Result<()> {
        self.inner
            .turbo()
            .heartbeat(&transfer_id, &upload_token)
            .map_err(operation)
    }

    pub fn turbo_create_download_session(
        &self,
        transfer_id: String,
        item_ids: Vec<String>,
    ) -> Result<TurboDownloadSession> {
        let session = self
            .inner
            .turbo()
            .create_download_session(&transfer_id, &item_ids)
            .map_err(operation)?;
        Ok(TurboDownloadSession {
            id: session.id,
            token: session.token,
        })
    }

    pub fn turbo_update_download_session(
        &self,
        transfer_id: String,
        session_id: String,
        token: String,
        sequence: u64,
        progress: f64,
        status: String,
    ) -> Result<()> {
        if !progress.is_finite()
            || !(0.0..=1.0).contains(&progress)
            || status.is_empty()
            || status.len() > 64
        {
            return Err(crate::invalid(
                "Use a finite 0-1 download progress and a short status",
            ));
        }
        self.inner
            .turbo()
            .update_download_session(
                &transfer_id,
                &session_id,
                &token,
                core::DownloadSessionUpdate {
                    sequence,
                    progress,
                    status,
                },
            )
            .map_err(operation)
    }

    pub fn account_login(
        &self,
        email: String,
        password: String,
        remember: bool,
    ) -> Result<AccountSession> {
        self.inner
            .account()
            .login(&email, &password, remember)
            .map(|session| self.remember_session(session))
            .map_err(operation)
    }

    pub fn account_register(
        &self,
        username: String,
        name: Option<String>,
        email: String,
        password: String,
    ) -> Result<AccountSession> {
        self.inner
            .account()
            .register(&username, name.as_deref(), &email, &password)
            .map(|session| self.remember_session(session))
            .map_err(operation)
    }

    /// Origin-scoped Cookie header for `UploadAuthentication.session_cookie`.
    /// The platform must store it in Keystore-backed custody and never log it.
    pub fn account_cookie_context(&self) -> Result<String> {
        self.inner.cookie_context().map_err(operation)
    }

    pub fn account_devices(&self) -> Result<DeviceRegistry> {
        self.inner
            .devices()
            .account_devices()
            .map(device_registry)
            .map_err(registry_operation)
    }

    pub fn account_device_challenge(
        &self,
        device_id: String,
        display_name: String,
        platform: String,
        public_key: String,
        key_version: u32,
        capabilities: Vec<String>,
    ) -> Result<DeviceEnrollmentChallenge> {
        self.inner
            .devices()
            .account_device_challenge(
                &device_id,
                &display_name,
                &platform,
                &public_key,
                key_version,
                &capabilities,
            )
            .map(device_enrollment_challenge)
            .map_err(registry_operation)
    }

    /// Submits the typed possession proof. A first device is activated and its
    /// credential is stored immediately; a later device remains pending.
    pub fn account_device_enroll(
        &self,
        challenge: DeviceEnrollmentChallenge,
    ) -> Result<RegisteredDevice> {
        let challenge = core_challenge(challenge)?;
        let signature = self.enrollment_signature(&challenge)?;
        let result = self
            .inner
            .devices()
            .account_device_enroll(&challenge, &signature)
            .map_err(registry_operation)?;
        if let Some(credential) = result.credential {
            self.store_credential(&result.device.id, credential)?;
        }
        Ok(registered_device(result.device))
    }

    /// Redeems an approved candidate's original proof without exposing its
    /// credential to the application layer.
    pub fn account_device_complete_enrollment(
        &self,
        challenge: DeviceEnrollmentChallenge,
    ) -> Result<DeviceCredentialRedemption> {
        let challenge = core_challenge(challenge)?;
        let signature = self.enrollment_signature(&challenge)?;
        let redemption = self
            .inner
            .devices()
            .account_device_complete_enrollment(&challenge, &signature)
            .map_err(registry_operation)?;
        self.store_credential(&challenge.device.id, redemption.credential)?;
        Ok(DeviceCredentialRedemption {
            device: registered_device(challenge.device),
            approval_evidence: redemption.approval_evidence.map(approval_evidence),
        })
    }

    pub fn device_approve_enrollment(
        &self,
        local_device_id: String,
        approval: DeviceApprovalPayload,
    ) -> Result<RegisteredDevice> {
        let approval = core_approval(approval)?;
        core::approval_transcript(&approval).map_err(registry_operation)?;
        let signer = self
            .device_signer
            .as_ref()
            .ok_or_else(|| invalid("Device enrollment requires a DeviceKeySignerCallback"))?;
        let signature = signer
            .sign_approval(ffi_approval(&approval))
            .map_err(operation)?;
        let signature: [u8; 64] = signature
            .try_into()
            .map_err(|_| invalid("Device approval signature must be 64 bytes"))?;
        let credential = self.load_credential(&local_device_id)?;
        let activated = self
            .inner
            .devices()
            .approve_device_enrollment(
                &core::AccountDevice {
                    id: approval.candidate_device_id.clone(),
                    display_name: String::new(),
                    platform: String::new(),
                    public_key: approval.candidate_public_key.clone(),
                    key_version: approval.candidate_key_version,
                    status: "pending".into(),
                    generation: approval.generation,
                    last_seen_at: None,
                    presence_expires_at: None,
                    revoked_at: None,
                },
                &credential,
                &signature,
            )
            .map_err(registry_operation)?;
        self.sync_and_pin(&approval, &local_device_id, &approval.candidate_device_id)?;
        Ok(registered_device(activated))
    }

    pub fn device_confirm_approver(
        &self,
        local_device_id: String,
        approval: DeviceApprovalPayload,
        evidence: DeviceApprovalEvidence,
        confirmed_fingerprint: String,
    ) -> Result<()> {
        let mut approval = core_approval(approval)?;
        if approval.candidate_device_id != local_device_id
            || evidence.version != 2
            || device_key_fingerprint(evidence.approver_public_key.clone())?
                != confirmed_fingerprint
        {
            return Err(invalid(
                "Approval evidence does not match the confirmed approver",
            ));
        }
        approval.approver_device_id = evidence.approver_device_id.clone();
        approval.approver_key_version = evidence.approver_key_version;
        let key = base64::engine::general_purpose::STANDARD
            .decode(&evidence.approver_public_key)
            .map_err(|_| invalid("Approval evidence has an invalid approver key"))?;
        let signature = base64::engine::general_purpose::STANDARD
            .decode(&evidence.approval_signature)
            .map_err(|_| invalid("Approval evidence has an invalid signature"))?;
        let key: [u8; 32] = key
            .try_into()
            .map_err(|_| invalid("Approval evidence has an invalid approver key"))?;
        let signature: [u8; 64] = signature
            .try_into()
            .map_err(|_| invalid("Approval evidence has an invalid signature"))?;
        ed25519_dalek::VerifyingKey::from_bytes(&key)
            .map_err(|_| invalid("Approval evidence has an invalid approver key"))?
            .verify_strict(
                &core::approval_transcript(&approval).map_err(registry_operation)?,
                &ed25519_dalek::Signature::from_bytes(&signature),
            )
            .map_err(|_| invalid("Approval evidence signature is invalid"))?;
        self.sync_and_pin(&approval, &local_device_id, &evidence.approver_device_id)
    }

    pub fn device_sync(&self, local_device_id: String) -> Result<DeviceRegistry> {
        let credential = self.load_credential(&local_device_id)?;
        self.inner
            .devices()
            .device_sync(&credential)
            .map(device_registry)
            .map_err(registry_operation)
    }

    /// This is called by an active app-owned receiver loop, never by directory
    /// rendering or one-shot discovery.
    pub fn device_advertise_presence(
        &self,
        local_device_id: String,
        ready: bool,
        capabilities: Vec<String>,
    ) -> Result<RegisteredDevice> {
        let credential = self.load_credential(&local_device_id)?;
        self.inner
            .devices()
            .device_presence(&credential, ready, &capabilities)
            .map(registered_device)
            .map_err(registry_operation)
    }

    pub fn account_device_rename(
        &self,
        device_id: String,
        display_name: String,
    ) -> Result<RegisteredDevice> {
        self.inner
            .devices()
            .account_device_rename(&device_id, &display_name)
            .map(registered_device)
            .map_err(registry_operation)
    }

    pub fn account_device_revoke(
        &self,
        device_id: String,
        current_password: String,
    ) -> Result<RegisteredDevice> {
        let (device, _) = self
            .inner
            .devices()
            .account_device_revoke(&device_id, &current_password)
            .map_err(registry_operation)?;
        if let Some(store) = &self.device_credentials {
            store
                .remove(self.inner.instance(), device_id)
                .map_err(operation)?;
        }
        Ok(registered_device(device))
    }

    pub fn account_devices_reset(&self, current_password: String) -> Result<DeviceRegistry> {
        let (revision, generation) = self
            .inner
            .devices()
            .account_devices_reset(&current_password)
            .map_err(registry_operation)?;
        Ok(DeviceRegistry {
            revision,
            generation,
            devices: Vec::new(),
        })
    }

    pub fn account_logout(&self) -> Result<()> {
        self.account_cancel_automatic_receiving();
        self.inner.account().logout().map_err(operation)?;
        self.account_id
            .store(0, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    pub fn account_session(&self) -> Result<AccountSession> {
        self.inner
            .account()
            .session()
            .map(|session| self.remember_session(session))
            .map_err(operation)
    }

    pub fn account_contacts_json(&self) -> Result<String> {
        self.inner
            .account()
            .contacts()
            .and_then(|value| serde_json::to_string(&value).map_err(Into::into))
            .map_err(operation)
    }
    pub fn account_contact_action_json(
        &self,
        username: String,
        action: String,
        can_send: Option<bool>,
        auto_download: Option<bool>,
    ) -> Result<String> {
        self.inner
            .account()
            .contact_action(&username, &action, can_send, auto_download)
            .and_then(|value| serde_json::to_string(&value).map_err(Into::into))
            .map_err(operation)
    }
    pub fn account_receiving_defaults(&self, policy: String, auto_download: bool) -> Result<()> {
        self.inner
            .account()
            .set_receiving_defaults(&policy, auto_download)
            .map(|_| ())
            .map_err(operation)
    }
    pub fn account_receiver_state_json(&self, enabled: Option<bool>) -> Result<String> {
        if enabled == Some(false) {
            self.account_cancel_automatic_receiving();
        }
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| crate::invalid("Inbox receiving requires a native runtime"))?;
        let account_id = self.local_account_id()?;
        let root = runtime.settings.state_home.join("inbox-staging");
        let secrets = runtime
            .settings
            .checkpoint_secret_store
            .clone()
            .unwrap_or_else(|| {
                filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(&root)
            });
        let receiver = core::inbox_receiver::InboxReceiver::open(
            &root,
            &self.inner.instance(),
            account_id,
            secrets,
        )
        .map_err(operation)?;
        let state = match enabled {
            Some(value) => receiver.set_enabled(value),
            None => receiver.state(),
        }
        .map_err(operation)?;
        serde_json::to_string(&state).map_err(operation)
    }
    pub fn account_receive_automatically(&self) -> Result<String> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| crate::invalid("Inbox receiving requires a native runtime"))?;
        let (sender, _prompts) = std::sync::mpsc::channel();
        let control = filebeam_client_core::control::Control::new(runtime.settings.clone(), sender);
        {
            let mut active = self
                .automatic
                .lock()
                .map_err(|_| crate::invalid("Inbox receiving worker is unavailable"))?;
            if active.is_some() {
                return Err(crate::invalid("Inbox receiving is already running"));
            }
            *active = Some(control.clone());
        }
        let result = (|| {
            let _memory = runtime.reserve_service_memory(32 * 1024 * 1024)?;
            let session = self.inner.account().session().map_err(operation)?;
            self.account_id
                .store(session.id, std::sync::atomic::Ordering::Relaxed);
            control.check().map_err(operation)?;
            let root = runtime.settings.state_home.join("inbox-staging");
            let secrets = runtime
                .settings
                .checkpoint_secret_store
                .clone()
                .unwrap_or_else(|| {
                    filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(
                        &root,
                    )
                });
            let receiver = core::inbox_receiver::InboxReceiver::open(
                &root,
                &self.inner.instance(),
                session.id,
                secrets,
            )
            .map_err(operation)?;
            receiver
                .sweep(&self.inner, &control)
                .and_then(|value| serde_json::to_string(&value).map_err(Into::into))
                .map_err(operation)
        })();
        if let Ok(mut active) = self.automatic.lock() {
            *active = None;
        }
        result
    }

    pub fn account_cancel_automatic_receiving(&self) {
        if let Ok(active) = self.automatic.lock()
            && let Some(control) = &*active
        {
            control.cancel();
        }
    }

    pub fn account_export_staged_inbox(
        &self,
        transfer_id: String,
        private_key: Vec<u8>,
        destination: String,
    ) -> Result<Vec<String>> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| crate::invalid("Inbox receiving requires a native runtime"))?;
        let _memory = runtime.reserve_service_memory(64 * 1024 * 1024)?;
        let account_id = self.local_account_id()?;
        let root = runtime.settings.state_home.join("inbox-staging");
        let secrets = runtime
            .settings
            .checkpoint_secret_store
            .clone()
            .unwrap_or_else(|| {
                filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(&root)
            });
        let receiver = core::inbox_receiver::InboxReceiver::open(
            &root,
            &self.inner.instance(),
            account_id,
            secrets,
        )
        .map_err(operation)?;
        let (sender, _) = std::sync::mpsc::channel();
        let control = filebeam_client_core::control::Control::new(runtime.settings.clone(), sender);
        let private_key = Zeroizing::new(private_key);
        receiver
            .export(
                &transfer_id,
                &private_key,
                std::path::Path::new(&destination),
                &control,
            )
            .map_err(operation)
    }

    pub fn account_dismiss_staged_inbox(&self, transfer_id: String) -> Result<()> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| crate::invalid("Inbox receiving requires a native runtime"))?;
        let root = runtime.settings.state_home.join("inbox-staging");
        let secrets = runtime
            .settings
            .checkpoint_secret_store
            .clone()
            .unwrap_or_else(|| {
                filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(&root)
            });
        core::inbox_receiver::InboxReceiver::open(
            &root,
            &self.inner.instance(),
            self.local_account_id()?,
            secrets,
        )
        .map_err(operation)?
        .dismiss(&transfer_id)
        .map_err(operation)
    }

    pub fn account_inbox(&self) -> Result<Vec<InboxTransfer>> {
        self.inner
            .account()
            .inbox()
            .map_err(operation)
            .map(|items| {
                items
                    .into_iter()
                    .map(|item| InboxTransfer {
                        id: item.id,
                        ciphertext_bytes: item.ciphertext_bytes,
                        item_count: item.item_count,
                        completed_at: item.completed_at,
                        expires_at: item.expires_at,
                    })
                    .collect()
            })
    }
    pub fn account_inbox_unread_count(&self) -> Result<u64> {
        self.inner
            .account()
            .inbox_unread_count()
            .map(|count| count.count)
            .map_err(operation)
    }
    pub fn account_delete_inbox_item(&self, transfer_id: String) -> Result<()> {
        self.inner
            .account()
            .delete_inbox_item(&transfer_id)
            .map_err(operation)
    }
    /// One page of the signed-in account's sent transfers. Filters are the
    /// server's stable identifiers; filenames and keys are never part of history.
    pub fn account_history(
        &self,
        filter: HistoryFilter,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<HistoryPage> {
        let page = self
            .inner
            .history()
            .list(
                &filebeam_client_core::services::HistoryFilter {
                    status: filter.status,
                    kind: filter.kind,
                    driver: filter.driver,
                },
                cursor.as_deref(),
                limit,
            )
            .map_err(operation)?;
        Ok(HistoryPage {
            entries: page
                .data
                .into_iter()
                .map(|entry| HistoryEntry {
                    id: entry.id,
                    kind: entry.kind,
                    driver: entry.driver,
                    delivery: entry.delivery,
                    status: entry.status,
                    item_count: entry.item_count,
                    ciphertext_bytes: entry.ciphertext_bytes,
                    created_at: entry.created_at,
                    expires_at: entry.expires_at,
                    removed_at: entry.removed_at,
                    retention_hours: entry.retention_hours,
                    maximum_retention_hours: entry.maximum_retention_hours,
                    burn_on_read: entry.burn_on_read,
                    can_delete: entry.can_delete,
                    can_extend: entry.can_extend,
                })
                .collect(),
            next_cursor: page.next_cursor,
        })
    }

    /// Removes a sent transfer for everyone.
    pub fn account_history_delete(&self, transfer_id: String) -> Result<()> {
        self.inner.history().delete(&transfer_id).map_err(operation)
    }

    /// Sets a longer total retention and returns the new expiry timestamp.
    pub fn account_history_extend(
        &self,
        transfer_id: String,
        retention_hours: u64,
    ) -> Result<String> {
        self.inner
            .history()
            .extend(&transfer_id, retention_hours)
            .map(|update| update.expires_at)
            .map_err(operation)
    }

    pub fn account_policy_json(&self) -> Result<String> {
        self.inner
            .account()
            .policy()
            .and_then(|value| serde_json::to_string(&value).map_err(Into::into))
            .map_err(operation)
    }
    pub fn account_inspect_invitation(&self, token: String) -> Result<NativeInvitation> {
        self.inner
            .account()
            .invitation(&token)
            .map(|value| NativeInvitation {
                email: value.email,
                expires_at: value.expires_at,
            })
            .map_err(operation)
    }
    pub fn account_accept_invitation(
        &self,
        token: String,
        username: String,
        name: Option<String>,
        email: String,
        password: String,
    ) -> Result<AccountSession> {
        self.inner
            .account()
            .accept_invitation(&token, &username, name.as_deref(), &email, &password)
            .map(|session| self.remember_session(session))
            .map_err(operation)
    }
    pub fn account_report(
        &self,
        transfer_id: String,
        category: String,
        description: String,
        email: Option<String>,
    ) -> Result<()> {
        self.inner
            .account()
            .report(&transfer_id, &category, &description, email.as_deref())
            .map_err(operation)
    }
    pub fn account_delete(&self, current_password: String, confirmation: String) -> Result<String> {
        self.inner
            .account()
            .delete_account(&current_password, &confirmation)
            .map(|result| result.status)
            .map_err(operation)
    }

    pub fn account_keys(&self) -> Result<Vec<AccountKeyBundle>> {
        self.inner
            .account()
            .account_keys()
            .map_err(operation)
            .map(|keys| {
                keys.into_iter()
                    .map(|key| AccountKeyBundle {
                        id: key.id,
                        user_id: key.user_id,
                        version: key.version,
                        public_key: key.public_key,
                        fingerprint: key.fingerprint,
                        custody_mode: key.custody_mode,
                        encrypted_private_key: key.encrypted_private_key,
                        is_active: key.is_active,
                    })
                    .collect()
            })
    }

    /// UI-safe custody state; it contains no private or encrypted key material.
    pub fn account_key_situation(&self) -> Result<AccountKeySituation> {
        self.inner
            .account()
            .key_situation()
            .map_err(operation)
            .map(|situation| AccountKeySituation {
                replacement_acknowledgement_required: situation.active.is_some(),
                active_bundle_id: situation.active.as_ref().map(|key| key.id),
                active_custody_mode: situation.active.map(|key| key.custody_mode),
                historical_bundle_ids: situation.historical_bundle_ids,
            })
    }

    pub fn account_upload_key(&self, key: AccountKeyUpload) -> Result<AccountKeyBundle> {
        if !matches!(key.custody_mode.as_str(), "password" | "self") {
            return Err(crate::invalid(
                "Account key custody must be password or self",
            ));
        }
        self.inner
            .account()
            .upload_key(&core::AccountKeyUpload {
                public_key: key.public_key,
                fingerprint: key.fingerprint,
                custody_mode: key.custody_mode,
                encrypted_private_key: key.encrypted_private_key,
                current_password: key.current_password,
                replace: key.replace,
            })
            .map(account_key_bundle)
            .map_err(operation)
    }

    pub fn account_set_inbox_enabled(&self, enabled: bool) -> Result<()> {
        self.inner
            .account()
            .set_inbox_enabled(enabled)
            .map_err(operation)
    }

    /// Explicit acknowledgement only; listing an inbox never changes read state.
    pub fn account_mark_inbox_notifications_read(&self) -> Result<()> {
        self.inner
            .account()
            .mark_inbox_notifications_read()
            .map_err(operation)
    }

    pub fn account_set_notification_channel(&self, channel: String) -> Result<()> {
        self.inner
            .account()
            .set_notification_channel(&channel)
            .map_err(operation)
    }

    pub fn account_resend_verification(&self) -> Result<()> {
        self.inner
            .account()
            .resend_verification()
            .map_err(operation)
    }

    pub fn account_verify_email_link(&self, link: String) -> Result<()> {
        self.inner
            .account()
            .verify_email_link(&link)
            .map_err(operation)
    }

    pub fn account_request_password_reset(&self, email: String) -> Result<()> {
        self.inner
            .account()
            .request_password_reset(&email)
            .map_err(operation)
    }

    pub fn account_reset_password(
        &self,
        email: String,
        token: String,
        password: String,
    ) -> Result<()> {
        self.inner
            .account()
            .reset_password(&email, &token, &password)
            .map_err(operation)
    }

    /// Exports a self-custody key without converting it to a printable diagnostic.
    pub fn account_export_self_key(&self, private_key: Vec<u8>) -> Result<String> {
        core::export_self_key(&private_key).map_err(operation)
    }

    pub fn account_import_self_key(&self, value: String) -> Result<Vec<u8>> {
        core::import_self_key(&value)
            .map(|key| key.to_vec())
            .map_err(operation)
    }

    pub fn account_generate_self_key(&self) -> Result<GeneratedAccountKey> {
        let key = core::generate_self_keypair().map_err(operation)?;
        Ok(GeneratedAccountKey {
            private_key: key.private_key.to_vec(),
            public_key: key.public_key,
            fingerprint: key.fingerprint,
        })
    }

    pub fn account_wrap_password_key(
        &self,
        private_key: Vec<u8>,
        password: String,
        user_id: u64,
        public_key: String,
    ) -> Result<String> {
        let _memory = self
            .runtime
            .as_ref()
            .map(|runtime| {
                runtime
                    .reserve_service_memory(filebeam_client_core::TRANSIENT_MEMORY_ALLOWANCE_BYTES)
            })
            .transpose()?;
        let password = Zeroizing::new(password);
        core::wrap_password_key(&private_key, password.as_bytes(), user_id, &public_key)
            .map_err(operation)
    }

    pub fn account_unwrap_password_key(
        &self,
        envelope: String,
        password: String,
        user_id: u64,
        public_key: String,
    ) -> Result<Vec<u8>> {
        let _memory = self
            .runtime
            .as_ref()
            .map(|runtime| {
                runtime
                    .reserve_service_memory(filebeam_client_core::TRANSIENT_MEMORY_ALLOWANCE_BYTES)
            })
            .transpose()?;
        let password = Zeroizing::new(password);
        core::unwrap_password_key(&envelope, password.as_bytes(), user_id, &public_key)
            .map(|key| key.to_vec())
            .map_err(operation)
    }

    /// Decrypts an inbox key with the logged-in account's recipient identity.
    pub fn account_open_inbox_key(
        &self,
        transfer_id: String,
        private_key: Vec<u8>,
    ) -> Result<Vec<u8>> {
        let metadata = self
            .inner
            .account()
            .inbox_metadata(&transfer_id)
            .map_err(operation)?;
        core::open_recipient_key(&private_key, &metadata.recipient_key, &metadata.id)
            .map(|key| key.to_vec())
            .map_err(operation)
    }

    pub fn account_open_inbox(
        &self,
        transfer_id: String,
        private_key: Vec<u8>,
    ) -> Result<OpenedInbox> {
        let private_key = Zeroizing::new(private_key);
        self.inner
            .account()
            .open_inbox(&transfer_id, &private_key)
            .map_err(operation)
            .map(|opened| OpenedInbox {
                transfer_id: opened.transfer_id,
                key_bundle_id: opened.key_bundle_id,
                filenames: opened.filenames,
            })
    }

    pub fn account_inbox_metadata(&self, transfer_id: String) -> Result<InboxMetadata> {
        self.inner
            .account()
            .inbox_metadata(&transfer_id)
            .map_err(operation)
            .map(|metadata| InboxMetadata {
                id: metadata.id,
                encrypted_manifest: metadata.encrypted_manifest,
                recipient_key: InboxRecipientKey {
                    bundle: account_key_bundle(metadata.recipient_key.bundle),
                    encrypted_key: metadata.recipient_key.encrypted_key,
                },
            })
    }

    pub fn account_recipient(&self, username: String) -> Result<Recipient> {
        self.inner
            .account()
            .recipient(&username)
            .map_err(operation)
            .map(|recipient| Recipient {
                id: recipient.id,
                username: recipient.username,
                public_key: recipient.public_key,
                account_key_bundle_id: recipient.account_key_bundle_id,
                version: recipient.version,
                fingerprint: recipient.fingerprint,
            })
    }
}

impl NativeServices {
    fn remember_session(&self, session: core::AccountSession) -> AccountSession {
        self.account_id
            .store(session.id, std::sync::atomic::Ordering::Relaxed);
        account_session(session)
    }
    fn local_account_id(&self) -> Result<u64> {
        let id = self.account_id.load(std::sync::atomic::Ordering::Relaxed);
        if id != 0 {
            return Ok(id);
        }
        let session = self.inner.account().session().map_err(operation)?;
        let id = session.id;
        self.remember_session(session);
        Ok(id)
    }

    fn enrollment_signature(
        &self,
        challenge: &core::DeviceEnrollmentChallenge,
    ) -> Result<[u8; 64]> {
        let signer = self
            .device_signer
            .as_ref()
            .ok_or_else(|| invalid("Device enrollment requires a DeviceKeySignerCallback"))?;
        let signature = signer
            .sign_enrollment(
                challenge.device.id.clone(),
                challenge.device.generation,
                challenge.challenge.clone(),
            )
            .map_err(operation)?;
        signature
            .try_into()
            .map_err(|_| invalid("Device enrollment signature must be 64 bytes"))
    }

    fn store_credential(&self, device_id: &str, credential: String) -> Result<()> {
        self.device_credentials
            .as_ref()
            .ok_or_else(|| invalid("Device enrollment requires a DeviceCredentialStoreCallback"))?
            .store(self.inner.instance(), device_id.into(), credential)
            .map_err(operation)
    }

    fn load_credential(&self, device_id: &str) -> Result<String> {
        self.device_credentials
            .as_ref()
            .ok_or_else(|| invalid("Device operations require a DeviceCredentialStoreCallback"))?
            .load(self.inner.instance(), device_id.into())
            .map_err(operation)
    }

    /// A local pin is written only after backend activation and an authenticated
    /// registry response prove the candidate's current active key.
    fn sync_and_pin(
        &self,
        approval: &core::DeviceApprovalContext,
        local_device_id: &str,
        peer_device_id: &str,
    ) -> Result<()> {
        if self.inner.instance() != approval.instance_origin
            || self.local_account_id()? != approval.account_id
        {
            return Err(invalid(
                "Device approval ceremony does not match this account or instance",
            ));
        }
        let runtime = self.runtime.as_ref().ok_or_else(|| {
            invalid("Device approval requires a NativeRuntime with encrypted checkpoint storage")
        })?;
        let local_device = DeviceId::parse(local_device_id).map_err(registry_operation)?;
        let scope = RegistryScope {
            origin: Origin::parse(approval.instance_origin.clone()).map_err(registry_operation)?,
            deployment: DeploymentId::parse(approval.deployment_id.clone())
                .map_err(registry_operation)?,
            account: AccountId(approval.account_id),
            generation: DeviceGeneration(approval.generation),
            local_device,
        };
        let mut digest = Sha256::new();
        digest.update(approval.instance_origin.as_bytes());
        digest.update([0]);
        digest.update(approval.deployment_id.as_bytes());
        digest.update([0]);
        digest.update(approval.account_id.to_be_bytes());
        digest.update(approval.generation.to_be_bytes());
        let id = format!(
            "device-registry-{}",
            digest
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let store = runtime.registry_store(&id).map_err(registry_operation)?;
        let mut registry =
            DurableDeviceRegistry::open(&store, scope, Duration::from_secs(7 * 24 * 60 * 60))
                .map_err(registry_operation)?;
        let credential = self.load_credential(local_device_id)?;
        let snapshot = self
            .inner
            .devices()
            .device_sync(&credential)
            .map_err(registry_operation)?;
        let freshness = snapshot.offline_freshness_seconds.ok_or_else(|| {
            invalid("Authenticated device registry response is missing its freshness policy")
        })?;
        registry
            .ingest(
                RegistryResponse {
                    revision: snapshot.revision,
                    generation: DeviceGeneration(snapshot.generation),
                    offline_freshness_seconds: freshness,
                    devices: snapshot
                        .devices
                        .into_iter()
                        .map(registry_device)
                        .collect::<Result<Vec<_>>>()?,
                },
                unix_seconds()?,
            )
            .map_err(registry_operation)?;
        registry
            .approve_pairing(
                &DeviceId::parse(peer_device_id).map_err(registry_operation)?,
                unix_seconds()?,
            )
            .map_err(registry_operation)
    }
}

fn registry_device(
    device: core::AccountDevice,
) -> Result<filebeam_client_core::device_registry::RegistryDevice> {
    Ok(filebeam_client_core::device_registry::RegistryDevice {
        id: DeviceId::parse(device.id).map_err(registry_operation)?,
        key_version: device.key_version,
        public_key: device.public_key,
        status: match device.status.as_str() {
            "pending" => filebeam_client_core::device_registry::DeviceStatus::Pending,
            "active" => filebeam_client_core::device_registry::DeviceStatus::Active,
            "revoked" => filebeam_client_core::device_registry::DeviceStatus::Revoked,
            _ => {
                return Err(invalid(
                    "Authenticated device registry contains an invalid status",
                ));
            }
        },
        generation: DeviceGeneration(device.generation),
    })
}

fn unix_seconds() -> Result<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("System clock is before the Unix epoch"))
        .and_then(|value| {
            i64::try_from(value.as_secs()).map_err(|_| invalid("System clock is out of range"))
        })
}

fn registered_device(device: core::AccountDevice) -> RegisteredDevice {
    RegisteredDevice {
        id: device.id,
        display_name: device.display_name,
        platform: device.platform,
        public_key: device.public_key,
        key_version: device.key_version,
        status: device.status,
        generation: device.generation,
        last_seen_at: device.last_seen_at,
        presence_expires_at: device.presence_expires_at,
        revoked_at: device.revoked_at,
    }
}

fn core_device(device: RegisteredDevice) -> Result<core::AccountDevice> {
    if device.id.is_empty() || device.public_key.is_empty() || device.key_version == 0 {
        return Err(invalid("Device identity is incomplete"));
    }
    Ok(core::AccountDevice {
        id: device.id,
        display_name: device.display_name,
        platform: device.platform,
        public_key: device.public_key,
        key_version: device.key_version,
        status: device.status,
        generation: device.generation,
        last_seen_at: device.last_seen_at,
        presence_expires_at: device.presence_expires_at,
        revoked_at: device.revoked_at,
    })
}

fn device_registry(snapshot: core::DeviceRegistrySnapshot) -> DeviceRegistry {
    DeviceRegistry {
        revision: snapshot.revision,
        generation: snapshot.generation,
        devices: snapshot
            .devices
            .into_iter()
            .map(registered_device)
            .collect(),
    }
}

fn device_enrollment_challenge(
    challenge: core::DeviceEnrollmentChallenge,
) -> DeviceEnrollmentChallenge {
    let approval_context = core::DeviceApprovalContext {
        candidate_public_key: challenge.device.public_key.clone(),
        ..challenge.approval_context
    };
    DeviceEnrollmentChallenge {
        device: registered_device(challenge.device),
        challenge: challenge.challenge,
        expires_at: challenge.expires_at,
        approval_context: ffi_approval(&approval_context),
    }
}

fn core_challenge(challenge: DeviceEnrollmentChallenge) -> Result<core::DeviceEnrollmentChallenge> {
    let device = core_device(challenge.device)?;
    let approval_context = core_approval(challenge.approval_context)?;
    if approval_context.candidate_device_id != device.id
        || approval_context.candidate_key_version != device.key_version
        || approval_context.candidate_public_key != device.public_key
        || approval_context.generation != device.generation
    {
        return Err(invalid(
            "Enrollment approval context does not match its candidate",
        ));
    }
    Ok(core::DeviceEnrollmentChallenge {
        device,
        challenge: challenge.challenge,
        expires_at: challenge.expires_at,
        approval_context,
    })
}

fn core_approval(value: DeviceApprovalPayload) -> Result<core::DeviceApprovalContext> {
    if value.instance_origin.is_empty() || value.deployment_id.is_empty() || value.account_id == 0 {
        return Err(invalid("Device approval context is incomplete"));
    }
    Ok(core::DeviceApprovalContext {
        instance_origin: value.instance_origin,
        deployment_id: value.deployment_id,
        account_id: value.account_id,
        generation: value.generation,
        candidate_device_id: value.candidate_device_id,
        candidate_key_version: value.candidate_key_version,
        candidate_public_key: value.candidate_public_key,
        challenge_sha256: value.challenge_sha256,
        expires_at_unix: value.expires_at_unix,
        approver_device_id: value.approver_device_id,
        approver_key_version: value.approver_key_version,
    })
}

fn ffi_approval(value: &core::DeviceApprovalContext) -> DeviceApprovalPayload {
    DeviceApprovalPayload {
        instance_origin: value.instance_origin.clone(),
        deployment_id: value.deployment_id.clone(),
        account_id: value.account_id,
        generation: value.generation,
        candidate_device_id: value.candidate_device_id.clone(),
        candidate_key_version: value.candidate_key_version,
        candidate_public_key: value.candidate_public_key.clone(),
        challenge_sha256: value.challenge_sha256.clone(),
        expires_at_unix: value.expires_at_unix,
        approver_device_id: value.approver_device_id.clone(),
        approver_key_version: value.approver_key_version,
    }
}

fn approval_evidence(value: core::DeviceApprovalEvidence) -> DeviceApprovalEvidence {
    DeviceApprovalEvidence {
        version: value.version,
        approver_device_id: value.approver_device_id,
        approver_key_version: value.approver_key_version,
        approver_public_key: value.approver_public_key,
        approval_signature: value.approval_signature,
    }
}

fn account_session(session: core::AccountSession) -> AccountSession {
    AccountSession {
        id: session.id,
        name: session.name,
        username: session.username,
        email: session.email,
        email_verified_at: session.email_verified_at,
        profile_url: session.profile_url,
        inbox_enabled: session.inbox_enabled,
        username_routing_enabled: session.username_routing_enabled,
        notification_channel: session.notification_channel,
    }
}

fn account_key_bundle(key: core::AccountKeyBundle) -> AccountKeyBundle {
    AccountKeyBundle {
        id: key.id,
        user_id: key.user_id,
        version: key.version,
        public_key: key.public_key,
        fingerprint: key.fingerprint,
        custody_mode: key.custody_mode,
        encrypted_private_key: key.encrypted_private_key,
        is_active: key.is_active,
    }
}
