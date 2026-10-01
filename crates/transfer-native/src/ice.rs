//! Per-transfer ICE policy and host-backed TURN credential resolution.

use std::sync::Arc;

use anyhow::{Result, bail};

use crate::webrtc::IceServer;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IceSource {
    #[default]
    Server,
    Merge,
    Replace,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ConfiguredIceServer {
    pub url: String,
    pub credential_ref: Option<String>,
}

pub struct IceCredential {
    pub username: String,
    pub password: String,
}

/// Resolves an opaque configured credential reference from host secure storage.
/// Implementations must not persist or log the returned credential.
pub trait IceCredentialResolver: Send + Sync {
    fn resolve(&self, reference: &str) -> Result<IceCredential>;
}

#[derive(Clone)]
pub struct IceOverride {
    source: IceSource,
    servers: Vec<ConfiguredIceServer>,
    resolver: Option<Arc<dyn IceCredentialResolver>>,
}

impl IceOverride {
    pub fn new(source: IceSource, servers: Vec<ConfiguredIceServer>) -> Self {
        Self {
            source,
            servers,
            resolver: None,
        }
    }

    pub fn with_credential_resolver(mut self, resolver: Arc<dyn IceCredentialResolver>) -> Self {
        self.resolver = Some(resolver);
        self
    }

    pub fn resolve(&self, server_servers: &[IceServer]) -> Result<Vec<IceServer>> {
        let mut servers = match self.source {
            IceSource::Server => return Ok(server_servers.to_vec()),
            IceSource::Merge => server_servers.to_vec(),
            IceSource::Replace => Vec::new(),
        };
        for configured in &self.servers {
            let (scheme, authority) = configured
                .url
                .split_once(':')
                .ok_or_else(|| anyhow::anyhow!("configured ICE URL is invalid"))?;
            if !matches!(
                scheme.to_ascii_lowercase().as_str(),
                "stun" | "stuns" | "turn" | "turns"
            ) || authority
                .trim_start_matches('/')
                .split(['/', '?', ':'])
                .next()
                .is_none_or(str::is_empty)
            {
                bail!("configured ICE URL must use stun, stuns, turn, or turns with a host");
            }
            let is_turn = matches!(scheme.to_ascii_lowercase().as_str(), "turn" | "turns");
            let credential = match (&configured.credential_ref, is_turn) {
                (Some(reference), _) if reference.trim().is_empty() => {
                    bail!("configured ICE credential_ref must not be empty")
                }
                (Some(reference), _) => Some(
                    self.resolver
                        .as_ref()
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "configured ICE credential_ref requires a host credential resolver"
                            )
                        })?
                        .resolve(reference)?,
                ),
                (None, true) => bail!("configured TURN ICE server requires credential_ref"),
                (None, false) => None,
            };
            servers.push(IceServer {
                urls: vec![configured.url.clone()],
                username: credential.as_ref().map(|value| value.username.clone()),
                credential: credential.map(|value| value.password),
            });
        }
        Ok(servers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Credentials;
    impl IceCredentialResolver for Credentials {
        fn resolve(&self, reference: &str) -> Result<IceCredential> {
            if reference != "secure:turn" {
                bail!("missing credential")
            }
            Ok(IceCredential {
                username: "user".into(),
                password: "secret".into(),
            })
        }
    }

    fn server() -> IceServer {
        IceServer {
            urls: vec!["turn:server.example?transport=udp".into()],
            username: Some("ephemeral".into()),
            credential: Some("ephemeral-secret".into()),
        }
    }
    fn local() -> ConfiguredIceServer {
        ConfiguredIceServer {
            url: "stun:local.example".into(),
            credential_ref: None,
        }
    }

    #[test]
    fn source_precedence_preserves_server_credentials_only_in_memory() {
        assert_eq!(
            IceOverride::new(IceSource::Server, vec![local()])
                .resolve(&[server()])
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            IceOverride::new(IceSource::Merge, vec![local()])
                .resolve(&[server()])
                .unwrap()
                .len(),
            2
        );
        let resolved = IceOverride::new(
            IceSource::Replace,
            vec![ConfiguredIceServer {
                url: "turn:local.example?transport=udp".into(),
                credential_ref: Some("secure:turn".into()),
            }],
        )
        .with_credential_resolver(Arc::new(Credentials))
        .resolve(&[server()])
        .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].username.as_deref(), Some("user"));
    }

    #[test]
    fn turn_requires_a_reference_and_host_resolver() {
        assert!(
            IceOverride::new(
                IceSource::Replace,
                vec![ConfiguredIceServer {
                    url: "turn:local.example".into(),
                    credential_ref: None
                }]
            )
            .resolve(&[])
            .is_err()
        );
        assert!(
            IceOverride::new(
                IceSource::Replace,
                vec![ConfiguredIceServer {
                    url: "turn:local.example".into(),
                    credential_ref: Some("secure:turn".into())
                }]
            )
            .resolve(&[])
            .is_err()
        );
    }
}
