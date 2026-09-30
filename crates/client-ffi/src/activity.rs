use crate::{Result, TransferClient, operation};

#[derive(Clone, uniffi::Record)]
pub struct TransferActivity {
    pub records: Vec<TransferActivityRecord>,
    pub unavailable: bool,
    pub retry: bool,
}

#[derive(Clone, uniffi::Record)]
pub struct TransferActivityRecord {
    pub id: String,
    pub number: Option<u64>,
    pub progress: Option<f64>,
    pub state: String,
    pub selection_count: Option<u64>,
    pub all_files: Option<bool>,
}

#[uniffi::export]
impl TransferClient {
    /// Receiver activity for a checkpoint owned by this client. Tokens and
    /// transport capabilities remain inside the native protocol layer.
    pub fn transfer_activity(&self, checkpoint_id: String) -> Result<TransferActivity> {
        let secrets = self
            .settings
            .checkpoint_secret_store
            .clone()
            .unwrap_or_else(|| {
                filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(
                    &self.settings.state_home,
                )
            });
        let activity =
            filebeam_transfer_native::protocol::activity::sender_activity_with_secret_store(
                &self.settings.state_home,
                &checkpoint_id,
                secrets,
            )
            .map_err(operation)?;
        Ok(TransferActivity {
            records: activity
                .records
                .into_iter()
                .map(|record| TransferActivityRecord {
                    id: record.id,
                    number: record.number,
                    progress: record.progress,
                    state: record.state,
                    selection_count: record.selection_count,
                    all_files: record.all_files,
                })
                .collect(),
            unavailable: activity.unavailable,
            retry: activity.retry,
        })
    }
}
