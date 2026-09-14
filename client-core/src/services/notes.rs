use anyhow::{Context, Result, bail};
use reqwest::{Url, blocking::Client, header::HeaderValue};
use serde::Deserialize;

use super::client::url;

#[derive(Clone)]
pub struct NotesService {
    instance: Url,
    http: Client,
}
#[derive(Clone, Debug, Deserialize)]
pub struct NoteMetadata {
    pub id: String,
    pub status: String,
    pub burn_on_read: bool,
    pub encrypted_manifest: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BurnResult {
    Consumed,
    AlreadyConsumed,
}
#[derive(Deserialize)]
struct Api<T> {
    data: T,
}

impl NotesService {
    pub(crate) fn new(instance: Url, http: Client) -> Self {
        Self { instance, http }
    }
    pub fn read(&self, transfer_id: &str) -> Result<NoteMetadata> {
        let response = self
            .http
            .get(url(
                &self.instance,
                &format!("api/v1/transfers/{transfer_id}"),
            )?)
            .send()
            .context("read note")?;
        if !response.status().is_success() {
            bail!("note read returned {}", response.status());
        }
        let note = response
            .json::<Api<NoteMetadata>>()
            .context("decode note metadata")?
            .data;
        if note.encrypted_manifest.is_none() {
            bail!("note has no encrypted envelope");
        }
        Ok(note)
    }
    /// Burning is deliberately best effort: a successful read remains valid if the
    /// network drops before this request reaches the service.
    pub fn consume(&self, transfer_id: &str, read_token: &str) -> Result<BurnResult> {
        let token =
            HeaderValue::from_str(read_token).context("read token contains invalid characters")?;
        let response = self
            .http
            .post(url(
                &self.instance,
                &format!("api/v1/transfers/{transfer_id}/consume"),
            )?)
            .header("X-Filebeam-Read-Token", token)
            .json(&serde_json::json!({}))
            .send()
            .context("consume burn note")?;
        match response.status().as_u16() {
            202 => Ok(BurnResult::Consumed),
            404 => Ok(BurnResult::AlreadyConsumed),
            status => bail!("burn note consumption returned {status}"),
        }
    }
}
