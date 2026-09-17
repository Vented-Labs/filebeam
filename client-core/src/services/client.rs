use anyhow::{Context, Result, bail};
use reqwest::{
    Url,
    blocking::Client,
    cookie::{CookieStore, Jar},
};
use std::sync::Arc;

use super::{AccountService, NotesService, TurboService};

#[derive(Clone)]
pub struct ServiceClient {
    instance: Url,
    cookies: Arc<Jar>,
    notes: NotesService,
    turbo: TurboService,
    account: AccountService,
}

impl ServiceClient {
    pub fn new(instance: &str) -> Result<Self> {
        Self::new_with_cookie_context(instance, None)
    }

    /// Restores an opaque Cookie header only into the same validated origin.
    /// This intentionally accepts no URL from persisted state.
    pub fn new_with_cookie_context(instance: &str, context: Option<&str>) -> Result<Self> {
        let mut instance = Url::parse(instance).context("instance URL is invalid")?;
        if !matches!(instance.scheme(), "http" | "https")
            || instance.host_str().is_none()
            || instance.query().is_some()
            || instance.fragment().is_some()
        {
            bail!("instance must be an HTTP origin");
        }
        instance.set_path("");
        let cookies = Arc::new(Jar::default());
        if let Some(context) = context {
            if context.len() > 16 * 1024 || context.contains(['\r', '\n']) {
                bail!("invalid persisted session cookie context");
            }
            for pair in context
                .split(';')
                .map(str::trim)
                .filter(|pair| !pair.is_empty())
            {
                if pair.split_once('=').is_none() {
                    bail!("invalid persisted session cookie context");
                }
                cookies.add_cookie_str(&format!("{pair}; Path=/"), &instance);
            }
        }
        let http = Client::builder()
            .cookie_provider(cookies.clone())
            .user_agent(concat!("filebeam-native/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("create service HTTP client")?;
        Ok(Self {
            instance: instance.clone(),
            cookies: cookies.clone(),
            notes: NotesService::new(instance.clone(), http.clone(), cookies.clone()),
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

    /// Returns cookies only for this validated instance origin. The host treats
    /// the value as a secret and supplies it only to same-origin upload requests.
    pub fn cookie_context(&self) -> Result<String> {
        self.cookies
            .cookies(&self.instance)
            .ok_or_else(|| anyhow::anyhow!("no authenticated session cookies are available"))?
            .to_str()
            .map(str::to_owned)
            .context("session cookie header is not valid text")
    }
}

pub(crate) fn url(instance: &Url, path: &str) -> Result<Url> {
    instance
        .join(path.trim_start_matches('/'))
        .context("build service URL")
}
