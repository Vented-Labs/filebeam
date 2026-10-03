//! Native control-plane operations that are intentionally separate from transfer jobs.

mod account;
mod client;
pub mod contacts;
mod history;
pub mod inbox_receiver;
pub mod local_state;
pub mod note_management;
mod notes;
mod turbo;

pub use account::{
    AccountKeyBundle, AccountKeyMaterial, AccountKeySituation, AccountKeyUpload, AccountService,
    AccountSession, InboxMetadata, InboxTransfer, OpenedInbox, Recipient, RecipientKey,
    export_self_key, generate_self_keypair, import_self_key, open_recipient_key,
    seal_recipient_key, unwrap_password_key, validate_self_key, wrap_password_key,
};
pub use client::ServiceClient;
pub use contacts::{Contact, ContactIdentity, Contacts, ReceivingDefaults};
pub use history::{HistoryEntry, HistoryFilter, HistoryPage, HistoryService, RetentionUpdate};
pub use notes::{
    BurnResult, CreatedNote, NoteCreate, NoteInspection, NoteMetadata, NoteReceiveOptions,
    NoteTransport, NotesService, OpenedNote, PendingBurn, ReceivedNote,
};
pub use turbo::{DownloadSession, DownloadSessionUpdate, TurboAvailability, TurboService};
