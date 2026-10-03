//! Account/origin-scoped discovery and deduplicated automatic ciphertext receiving.

use super::ServiceClient;
use anyhow::{Context, Result, bail};
use filebeam_transfer_native::{
    checkpoint::{SecretStore, Store},
    control::Control,
    protocol::inbox_staging::{self, Staging},
};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ReceiverState {
    pub enabled: bool,
    pub entries: Vec<StagedInbox>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct StagedInbox {
    pub id: String,
    pub bytes: u64,
    pub state: String,
    #[serde(default)]
    pub key_bundle_id: u64,
}

pub struct InboxReceiver {
    store: Store,
    root: std::path::PathBuf,
    instance: String,
    account_id: u64,
    secrets: Arc<dyn SecretStore>,
}

impl InboxReceiver {
    pub fn open(
        root: &Path,
        instance: &str,
        account_id: u64,
        secrets: Arc<dyn SecretStore>,
    ) -> Result<Self> {
        let instance = reqwest::Url::parse(instance)?
            .origin()
            .ascii_serialization();
        let store = inbox_staging::open(root, &instance, account_id, "receiver", secrets.clone())?;
        Ok(Self {
            store,
            root: root.into(),
            instance,
            account_id,
            secrets,
        })
    }
    pub fn state(&self) -> Result<ReceiverState> {
        Ok(self.store.load()?.unwrap_or_default())
    }
    pub fn set_enabled(&self, enabled: bool) -> Result<ReceiverState> {
        let mut state = self.state()?;
        state.enabled = enabled;
        if !enabled {
            for entry in &mut state.entries {
                if entry.state == "downloading" {
                    entry.state = "paused-local".into();
                }
            }
        }
        self.store.save(&state)?;
        Ok(state)
    }
    pub fn sweep(&self, client: &ServiceClient, control: &Control) -> Result<ReceiverState> {
        let mut state = self.state()?;
        if !state.enabled {
            return Ok(state);
        }
        if client.account().session()?.id != self.account_id {
            bail!("inbox receiver account changed");
        }
        if client.instance() != self.instance {
            bail!("inbox receiver instance changed");
        }
        let mut after = None;
        let mut available = std::collections::HashSet::new();
        let cookie = client.cookie_context()?;
        loop {
            control.check()?;
            let page = client.account().inbox_sync(after.as_deref())?;
            for item in page.transfers {
                available.insert(item.id.clone());
                if !item.auto_download {
                    if let Some(entry) = state.entries.iter_mut().find(|entry| {
                        entry.id == item.id
                            && entry.state != "staged-locked"
                            && entry.state != "dismissed"
                    }) {
                        entry.state = "paused-policy".into();
                    }
                    continue;
                }
                let existing = state.entries.iter().position(|entry| entry.id == item.id);
                if existing.is_some_and(|index| {
                    matches!(
                        state.entries[index].state.as_str(),
                        "staged-locked" | "dismissed"
                    )
                }) {
                    continue;
                }
                let used = state
                    .entries
                    .iter()
                    .filter(|entry| entry.state != "dismissed")
                    .try_fold(0u64, |total, entry| {
                        total
                            .checked_add(entry.bytes)
                            .context("inbox cache size overflow")
                    })?;
                if existing.is_none()
                    && used.saturating_add(item.ciphertext_bytes) > 2 * 1024 * 1024 * 1024
                {
                    continue;
                }
                let store = inbox_staging::open(
                    &self.root,
                    &self.instance,
                    self.account_id,
                    &item.id,
                    self.secrets.clone(),
                )?;
                let metadata = client
                    .account()
                    .get(&format!("api/native/v1/inbox/{}/staging", item.id))?;
                let prepared =
                    inbox_staging::prepare(&store, &self.instance, self.account_id, metadata)?;
                let index = existing.unwrap_or_else(|| {
                    state.entries.push(StagedInbox {
                        id: item.id.clone(),
                        bytes: item.ciphertext_bytes,
                        state: "downloading".into(),
                        key_bundle_id: prepared.metadata.recipient_key.bundle.id,
                    });
                    state.entries.len() - 1
                });
                // Publish the job association before fetching, so restarts and a
                // CLI/desktop sharing this profile resolve the same checkpoint.
                self.store.save(&state)?;
                let result = inbox_staging::fetch(&store, &cookie, control);
                state.entries[index].state = store
                    .load::<Staging>()?
                    .context("missing inbox staging checkpoint")?
                    .state;
                self.store.save(&state)?;
                result?;
            }
            after = page.next;
            if after.is_none() {
                break;
            }
        }
        for entry in &mut state.entries {
            if !available.contains(&entry.id)
                && !matches!(entry.state.as_str(), "staged-locked" | "dismissed")
            {
                entry.state = "unavailable".into();
            }
        }
        self.store.save(&state)?;
        Ok(state)
    }
    pub fn export(
        &self,
        id: &str,
        private_key: &[u8],
        output: &Path,
        control: &Control,
    ) -> Result<Vec<String>> {
        if !self
            .state()?
            .entries
            .iter()
            .any(|entry| entry.id == id && entry.state == "staged-locked")
        {
            bail!("delivery is not staged locally");
        }
        let store = inbox_staging::open(
            &self.root,
            &self.instance,
            self.account_id,
            id,
            self.secrets.clone(),
        )?;
        inbox_staging::export(&store, private_key, output, control)
    }
    pub fn dismiss(&self, id: &str) -> Result<()> {
        let mut state = self.state()?;
        let entry = state
            .entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .context("delivery is not staged locally")?;
        entry.state = "dismissed".into();
        self.store.save(&state)?;
        let identity = inbox_staging::identity(&self.instance, self.account_id, id);
        if self.root.join(&identity).exists() {
            Store::discard(&self.root, &identity)?;
        }
        Ok(())
    }
}
