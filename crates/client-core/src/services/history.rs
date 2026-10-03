use anyhow::{Context, Result, bail};
use reqwest::{Url, blocking::Client};
use serde::Deserialize;

use super::client::url;

#[derive(Clone)]
pub struct HistoryService {
    instance: Url,
    http: Client,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub kind: String,
    pub driver: String,
    pub delivery: String,
    pub status: String,
    pub item_count: u64,
    pub ciphertext_bytes: u64,
    pub declared_ciphertext_bytes: u64,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub published_at: Option<String>,
    pub expires_at: String,
    pub removed_at: Option<String>,
    pub retention_hours: u64,
    pub maximum_retention_hours: u64,
    pub maximum_expires_at: Option<String>,
    pub burn_on_read: bool,
    pub can_delete: bool,
    pub can_extend: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct HistoryPage {
    pub data: Vec<HistoryEntry>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct HistoryFilter {
    pub status: Option<String>,
    pub kind: Option<String>,
    pub driver: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RetentionUpdate {
    pub id: String,
    pub retention_hours: u64,
    pub expires_at: String,
}

impl HistoryService {
    pub(crate) fn new(instance: Url, http: Client) -> Self {
        Self { instance, http }
    }

    pub fn list(
        &self,
        filter: &HistoryFilter,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<HistoryPage> {
        let mut endpoint = url(&self.instance, "api/native/v1/history")?;
        {
            let mut query = endpoint.query_pairs_mut();
            query.append_pair("limit", &limit.to_string());
            for (name, value) in [
                ("status", filter.status.as_deref()),
                ("kind", filter.kind.as_deref()),
                ("driver", filter.driver.as_deref()),
                ("cursor", cursor),
            ] {
                if let Some(value) = value {
                    query.append_pair(name, value);
                }
            }
        }
        let response = self
            .http
            .get(endpoint)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .context("load account history")?;
        checked(response)?.json().context("decode account history")
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        validate_id(id)?;
        checked(
            self.http
                .delete(url(&self.instance, &format!("api/native/v1/history/{id}"))?)
                .header("Sec-Fetch-Site", "same-origin")
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .context("delete history transfer")?,
        )?;
        Ok(())
    }

    pub fn extend(&self, id: &str, retention_hours: u64) -> Result<RetentionUpdate> {
        validate_id(id)?;
        #[derive(Deserialize)]
        struct Response {
            data: RetentionUpdate,
        }
        checked(
            self.http
                .patch(url(
                    &self.instance,
                    &format!("api/native/v1/history/{id}/retention"),
                )?)
                .header("Sec-Fetch-Site", "same-origin")
                .header(reqwest::header::ACCEPT, "application/json")
                .json(&serde_json::json!({"retention_hours": retention_hours}))
                .send()
                .context("extend history transfer")?,
        )?
        .json::<Response>()
        .map(|response| response.data)
        .context("decode updated retention")
    }
}

fn validate_id(id: &str) -> Result<()> {
    if id.len() != 26 || !id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        bail!("provide a server transfer ID from beam history list");
    }
    Ok(())
}

fn checked(response: reqwest::blocking::Response) -> Result<reqwest::blocking::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    if status.as_u16() == 401 {
        bail!("sign in again to manage your account history");
    }
    let message = response
        .json::<serde_json::Value>()
        .ok()
        .and_then(|body| body["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "history operation failed".into());
    bail!("{message} ({status})")
}
