use anyhow::{bail, Context, Result};
use reqwest::{blocking::Client, header::HeaderValue, Url};
use serde::{Deserialize, Serialize};

use super::client::url;

#[derive(Clone)]
pub struct TurboService {
    instance: Url,
    http: Client,
}
#[derive(Clone, Debug, Deserialize)]
pub struct TurboAvailability {
    pub status: String,
    pub progress: u8,
    pub uploader_status: String,
    pub expires_at: String,
    pub items: Vec<TurboItemAvailability>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct TurboItemAvailability {
    pub id: String,
    pub ready_chunks: u64,
    pub uploaded_chunks: u64,
}
#[derive(Clone, Debug, Deserialize)]
pub struct DownloadSession {
    pub id: String,
    pub token: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct DownloadSessionUpdate {
    pub sequence: u64,
    pub progress: f64,
    pub status: String,
}
#[derive(Deserialize)]
struct Api<T> {
    data: T,
}
impl TurboService {
    pub(crate) fn new(instance: Url, http: Client) -> Self {
        Self { instance, http }
    }
    pub fn publish_descriptor(
        &self,
        id: &str,
        upload_token: &str,
        encrypted_descriptor: &str,
    ) -> Result<()> {
        self.write(
            id,
            upload_token,
            "descriptor",
            self.http
                .put(url(
                    &self.instance,
                    &format!("api/v1/transfers/{id}/descriptor"),
                )?)
                .json(&serde_json::json!({"encrypted_descriptor": encrypted_descriptor})),
        )
    }
    pub fn availability(&self, id: &str) -> Result<TurboAvailability> {
        let response = self
            .http
            .get(url(
                &self.instance,
                &format!("api/v1/transfers/{id}/progress"),
            )?)
            .send()
            .context("read Turbo availability")?;
        if !response.status().is_success() {
            bail!("Turbo availability returned {}", response.status());
        }
        response
            .json::<Api<TurboAvailability>>()
            .context("decode Turbo availability")
            .map(|value| value.data)
    }
    pub fn heartbeat(&self, id: &str, upload_token: &str) -> Result<()> {
        self.write(
            id,
            upload_token,
            "progress",
            self.http
                .patch(url(
                    &self.instance,
                    &format!("api/v1/transfers/{id}/progress"),
                )?)
                .json(&serde_json::json!({})),
        )
    }
    pub fn create_download_session(
        &self,
        id: &str,
        item_ids: &[String],
    ) -> Result<DownloadSession> {
        let response = self
            .http
            .post(url(
                &self.instance,
                &format!("api/v1/transfers/{id}/download-sessions"),
            )?)
            .json(&serde_json::json!({"item_ids": item_ids}))
            .send()
            .context("create download session")?;
        if !response.status().is_success() {
            bail!("download session returned {}", response.status());
        }
        response
            .json::<Api<DownloadSession>>()
            .context("decode download session")
            .map(|value| value.data)
    }
    pub fn update_download_session(
        &self,
        id: &str,
        session: &str,
        token: &str,
        update: DownloadSessionUpdate,
    ) -> Result<()> {
        let token =
            HeaderValue::from_str(token).context("session token contains invalid characters")?;
        let response = self
            .http
            .patch(url(
                &self.instance,
                &format!("api/v1/transfers/{id}/download-sessions/{session}"),
            )?)
            .header("X-Filebeam-Session-Token", token)
            .json(&update)
            .send()
            .context("update download session")?;
        if response.status().as_u16() != 204 {
            bail!("download session update returned {}", response.status());
        }
        Ok(())
    }
    fn write(
        &self,
        _id: &str,
        token: &str,
        operation: &str,
        request: reqwest::blocking::RequestBuilder,
    ) -> Result<()> {
        let token =
            HeaderValue::from_str(token).context("upload token contains invalid characters")?;
        let response = request
            .header("X-Filebeam-Upload-Token", token)
            .send()
            .with_context(|| format!("{operation} Turbo request"))?;
        if response.status().as_u16() != 204 {
            bail!("Turbo {operation} returned {}", response.status());
        }
        Ok(())
    }
}
