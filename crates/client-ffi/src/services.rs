use crate::{NativeRuntime, Result, client::origin, operation};
use filebeam_client_core::services as core;
use std::sync::Arc;
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

#[derive(uniffi::Object)]
pub struct NativeServices {
    pub(crate) inner: core::ServiceClient,
    pub(crate) runtime: Option<Arc<NativeRuntime>>,
}

#[uniffi::export]
impl NativeServices {
    #[uniffi::constructor]
    pub fn new(instance: String, allow_http: bool) -> Result<Self> {
        Ok(Self {
            inner: core::ServiceClient::new(&origin(&instance, allow_http)?).map_err(operation)?,
            runtime: None,
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
            .map(account_session)
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
            .map(account_session)
            .map_err(operation)
    }

    /// Origin-scoped Cookie header for `UploadAuthentication.session_cookie`.
    /// The platform must store it in Keystore-backed custody and never log it.
    pub fn account_cookie_context(&self) -> Result<String> {
        self.inner.cookie_context().map_err(operation)
    }

    pub fn account_logout(&self) -> Result<()> {
        self.inner.account().logout().map_err(operation)
    }

    pub fn account_session(&self) -> Result<AccountSession> {
        self.inner
            .account()
            .session()
            .map(account_session)
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
            .map(account_session)
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
