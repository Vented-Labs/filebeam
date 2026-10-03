use anyhow::{Context, Result, bail};
use reqwest::{
    Url,
    blocking::Client,
    cookie::{CookieStore, Jar},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::{AccountService, HistoryService, NotesService, TurboService};

#[derive(Clone)]
pub struct ServiceClient {
    instance: Url,
    cookies: Arc<Jar>,
    notes: NotesService,
    turbo: TurboService,
    account: AccountService,
    history: HistoryService,
    account_intent: Arc<AtomicBool>,
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
        let account_intent = Arc::new(AtomicBool::new(context.is_some()));
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
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(120))
            .cookie_provider(cookies.clone())
            .user_agent(concat!("filebeam-native/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("create service HTTP client")?;
        Ok(Self {
            instance: instance.clone(),
            cookies: cookies.clone(),
            notes: NotesService::new(
                instance.clone(),
                http.clone(),
                cookies.clone(),
                account_intent.clone(),
            ),
            turbo: TurboService::new(instance.clone(), http.clone()),
            account: AccountService::new(instance.clone(), http.clone(), account_intent.clone()),
            history: HistoryService::new(instance.clone(), http.clone()),
            account_intent,
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
    pub fn instance(&self) -> String {
        self.instance.origin().ascii_serialization()
    }

    pub fn history(&self) -> &HistoryService {
        &self.history
    }

    /// A saved session must validate before reservation; never downgrade an
    /// expired account cookie into an anonymous upload.
    pub fn upload_authentication(
        &self,
    ) -> Result<filebeam_transfer_native::protocol::UploadAuthentication> {
        if self.account_intent.load(Ordering::Relaxed) {
            self.account
                .session()
                .context("sign in again before sharing")?;
            Ok(
                filebeam_transfer_native::protocol::UploadAuthentication::SessionCookie(
                    self.cookie_context()?,
                ),
            )
        } else {
            Ok(filebeam_transfer_native::protocol::UploadAuthentication::Anonymous)
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use filebeam_transfer_native::protocol::UploadAuthentication;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn a_guest_cookie_does_not_turn_anonymous_sharing_into_an_account_session() {
        let client = ServiceClient::new("http://127.0.0.1:1").unwrap();
        client
            .cookies
            .add_cookie_str("guest_session=guest; Path=/", &client.instance);
        assert!(matches!(
            client.upload_authentication().unwrap(),
            UploadAuthentication::Anonymous
        ));
    }

    #[test]
    fn expired_saved_account_context_never_downgrades_to_anonymous() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut bytes = [0; 4096];
                let count = stream.read(&mut bytes).unwrap();
                let request = String::from_utf8_lossy(&bytes[..count]);
                assert!(request.starts_with("GET /api/native/v1/session "));
                stream.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            }
        });
        let client =
            ServiceClient::new_with_cookie_context(&origin, Some("account_session=expired"))
                .unwrap();
        for _ in 0..2 {
            assert!(client.upload_authentication().is_err());
        }
        server.join().unwrap();
    }
}
