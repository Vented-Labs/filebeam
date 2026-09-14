use anyhow::{Context, Result, bail};
use reqwest::{Url, blocking::Client};

use super::{AccountService, NotesService, TurboService};

#[derive(Clone)]
pub struct ServiceClient {
    notes: NotesService,
    turbo: TurboService,
    account: AccountService,
}

impl ServiceClient {
    pub fn new(instance: &str) -> Result<Self> {
        let mut instance = Url::parse(instance).context("instance URL is invalid")?;
        if !matches!(instance.scheme(), "http" | "https")
            || instance.host_str().is_none()
            || instance.query().is_some()
            || instance.fragment().is_some()
        {
            bail!("instance must be an HTTP origin");
        }
        instance.set_path("");
        let http = Client::builder()
            .cookie_store(true)
            .user_agent(concat!("filebeam-native/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("create service HTTP client")?;
        Ok(Self {
            notes: NotesService::new(instance.clone(), http.clone()),
            turbo: TurboService::new(instance.clone(), http.clone()),
            account: AccountService::new(instance.clone(), http.clone()),
        })
    }

    pub fn notes(&self) -> &NotesService {
        &self.notes
    }
    pub fn turbo(&self) -> &TurboService {
        &self.turbo
    }
    pub fn account(&self) -> &AccountService {
        &self.account
    }
}

pub(crate) fn url(instance: &Url, path: &str) -> Result<Url> {
    instance
        .join(path.trim_start_matches('/'))
        .context("build service URL")
}
