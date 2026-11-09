//! Opt-in LAN discovery and SDP setup for an already verified device pair.
//!
//! Discovery advertises only a random process ID and TCP setup port. Callers
//! must authenticate the setup through a current offline registry session.

use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex,
    time::timeout,
};

use crate::webrtc::{Description, DeviceChannels, NativePeer};

const SERVICE_TYPE: &str = "_filebeam-lan._tcp.local.";
const MAX_SETUP_BYTES: usize = 65_536;
const SETUP_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_ATTEMPTS_PER_MINUTE: usize = 12;
const MAX_DISCOVERED_PEERS: usize = 16;
const MAX_ROUTE_ID_BYTES: usize = 255;

/// A discovered setup endpoint. `id` is never device identity or authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanEndpoint {
    pub id: String,
    pub address: SocketAddr,
}

/// SDP-connected channels before registered-device authentication. The route
/// identifier is supplied by the initiator and is only a lookup hint; callers
/// must still authenticate it through a current registry session.
pub struct LanSetup {
    pub peer: NativePeer,
    pub channels: DeviceChannels,
    pub route_id: String,
}

/// Bounded SDP listener, not an HTTP, IPC, or file-transfer listener.
pub struct LanListener {
    listener: TcpListener,
    attempts: Arc<Mutex<VecDeque<Instant>>>,
    discovery_id: String,
}

impl LanListener {
    pub async fn bind(address: SocketAddr) -> Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(address)
                .await
                .context("bind LAN setup listener")?,
            attempts: Arc::new(Mutex::new(VecDeque::new())),
            discovery_id: random_id()?,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.listener
            .local_addr()
            .context("read LAN setup listener address")
    }
    pub fn discovery_id(&self) -> &str {
        &self.discovery_id
    }

    /// Advertises no TXT properties, device identity, account data, credentials,
    /// offers, or transfer metadata.
    pub fn advertise(&self) -> Result<LanAdvertisement> {
        let daemon = ServiceDaemon::new().context("start LAN mDNS")?;
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            &self.discovery_id,
            "filebeam.local.",
            "",
            self.local_addr()?.port(),
            &[] as &[(&str, &str)],
        )
        .context("create LAN mDNS service")?
        .enable_addr_auto();
        let fullname = info.get_fullname().to_owned();
        daemon
            .register(info)
            .context("advertise LAN mDNS service")?;
        Ok(LanAdvertisement { daemon, fullname })
    }

    /// Accept one bounded SDP setup and return its untrusted route hint. This
    /// lets a registry owner select a pinned peer before its normal proof flow.
    pub async fn accept_setup(&self) -> Result<LanSetup> {
        let (mut stream, _) = timeout(SETUP_TIMEOUT, self.listener.accept())
            .await
            .context("timed out waiting for LAN setup")?
            .context("accept LAN setup")?;
        self.record_attempt().await?;
        timeout(SETUP_TIMEOUT, async {
            let route_id = read_route_id(&mut stream).await?;
            let peer = NativePeer::new(&[], false).await?;
            match complete_responder_unverified(&mut stream, &peer).await {
                Ok(channels) => Ok(LanSetup {
                    peer,
                    channels,
                    route_id,
                }),
                Err(error) => {
                    peer.close().await;
                    Err(error)
                }
            }
        })
        .await
        .context("LAN setup timed out")?
    }

    async fn record_attempt(&self) -> Result<()> {
        let mut attempts = self.attempts.lock().await;
        let cutoff = Instant::now() - Duration::from_secs(60);
        while attempts.front().is_some_and(|attempt| *attempt < cutoff) {
            attempts.pop_front();
        }
        if attempts.len() >= MAX_ATTEMPTS_PER_MINUTE {
            bail!("LAN setup rate limit exceeded");
        }
        attempts.push_back(Instant::now());
        Ok(())
    }
}

/// Keeps the opt-in service alive and unregisters it on drop.
pub struct LanAdvertisement {
    daemon: ServiceDaemon,
    fullname: String,
}
impl Drop for LanAdvertisement {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

/// Discover no more than 16 opaque endpoint hints over a bounded period.
pub async fn discover(wait: Duration) -> Result<Vec<LanEndpoint>> {
    if wait.is_zero() || wait > SETUP_TIMEOUT {
        bail!("invalid LAN discovery duration");
    }
    let daemon = ServiceDaemon::new().context("start LAN mDNS browser")?;
    let receiver = daemon.browse(SERVICE_TYPE).context("browse LAN mDNS")?;
    let deadline = tokio::time::Instant::now() + wait;
    let result = async {
        let mut endpoints = Vec::new();
        while endpoints.len() < MAX_DISCOVERED_PEERS {
            let event = match tokio::time::timeout_at(deadline, receiver.recv_async()).await {
                Ok(event) => event.context("receive LAN mDNS event")?,
                Err(_) => break,
            };
            if let ServiceEvent::ServiceResolved(info) = event {
                let Some(id) = discovery_id(&info.fullname) else {
                    continue;
                };
                for address in &info.addresses {
                    let endpoint = LanEndpoint {
                        id: id.clone(),
                        address: SocketAddr::new(address.to_ip_addr(), info.port),
                    };
                    if !endpoints.contains(&endpoint) {
                        endpoints.push(endpoint);
                    }
                    if endpoints.len() == MAX_DISCOVERED_PEERS {
                        break;
                    }
                }
            }
        }
        Ok::<_, anyhow::Error>(endpoints)
    }
    .await;
    let _ = daemon.shutdown();
    result
}

/// Connect using an untrusted route hint. The returned channels cannot carry
/// transfer metadata until the caller runs its registry-created authentication.
pub async fn connect_setup(endpoint: SocketAddr, route_id: &str) -> Result<LanSetup> {
    timeout(SETUP_TIMEOUT, async {
        let mut stream = TcpStream::connect(endpoint)
            .await
            .context("connect to LAN setup listener")?;
        write_route_id(&mut stream, route_id).await?;
        let peer = NativePeer::new(&[], false).await?;
        match complete_initiator_unverified(&mut stream, &peer).await {
            Ok(channels) => Ok(LanSetup {
                peer,
                channels,
                route_id: route_id.to_owned(),
            }),
            Err(error) => {
                peer.close().await;
                Err(error)
            }
        }
    })
    .await
    .context("LAN setup timed out")?
}

async fn complete_responder_unverified(
    stream: &mut TcpStream,
    peer: &NativePeer,
) -> Result<DeviceChannels> {
    let offer = read_description(stream, "offer").await?;
    let answer = peer.answer(&offer).await?;
    write_description(stream, &answer).await?;
    peer.device_receiver_channels().await
}

async fn complete_initiator_unverified(
    stream: &mut TcpStream,
    peer: &NativePeer,
) -> Result<DeviceChannels> {
    let (offer, channels) = peer.offer_device_channels().await?;
    write_description(stream, &offer).await?;
    let answer = read_description(stream, "answer").await?;
    peer.accept_answer(&answer).await?;
    Ok(channels)
}

async fn write_route_id(stream: &mut TcpStream, route_id: &str) -> Result<()> {
    if route_id.is_empty() || route_id.len() > MAX_ROUTE_ID_BYTES || !route_id.is_ascii() {
        bail!("invalid LAN route identifier");
    }
    stream.write_u16(route_id.len() as u16).await?;
    stream.write_all(route_id.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_route_id(stream: &mut TcpStream) -> Result<String> {
    let length = stream.read_u16().await? as usize;
    if length == 0 || length > MAX_ROUTE_ID_BYTES {
        bail!("invalid LAN route identifier length");
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    let route_id = String::from_utf8(bytes).context("LAN route identifier is not UTF-8")?;
    if !route_id.is_ascii() {
        bail!("invalid LAN route identifier");
    }
    Ok(route_id)
}

async fn write_description(stream: &mut TcpStream, description: &Description) -> Result<()> {
    description.validate(&description.kind)?;
    let bytes = serde_json::to_vec(description).context("serialize LAN SDP")?;
    if bytes.len() > MAX_SETUP_BYTES {
        bail!("LAN SDP DTO is too large");
    }
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_description(stream: &mut TcpStream, kind: &str) -> Result<Description> {
    let length = stream.read_u32().await? as usize;
    if length == 0 || length > MAX_SETUP_BYTES {
        bail!("invalid LAN SDP DTO length");
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    let description: Description = serde_json::from_slice(&bytes).context("parse LAN SDP DTO")?;
    description.validate(kind)?;
    Ok(description)
}

fn random_id() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).context("generate LAN discovery ID")?;
    Ok(hex::encode(bytes))
}

fn discovery_id(fullname: &str) -> Option<String> {
    let id = fullname.strip_suffix(SERVICE_TYPE)?.strip_suffix('.')?;
    (id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())).then(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    const SENDER: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    #[tokio::test]
    async fn discovery_identifier_is_opaque_and_not_an_identity() -> Result<()> {
        let listener = LanListener::bind("127.0.0.1:0".parse()?).await?;
        assert_eq!(listener.discovery_id().len(), 32);
        assert!(
            listener
                .discovery_id()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
        Ok(())
    }

    #[tokio::test]
    async fn setup_route_hint_is_not_an_advertisement_identity() -> Result<()> {
        let listener = LanListener::bind("127.0.0.1:0".parse()?).await?;
        let (sender, receiver) = tokio::join!(
            connect_setup(listener.local_addr()?, SENDER),
            listener.accept_setup(),
        );
        let sender = sender?;
        let receiver = receiver?;
        assert_eq!(receiver.route_id, SENDER);
        assert_ne!(listener.discovery_id(), receiver.route_id);
        sender.peer.close().await;
        receiver.peer.close().await;
        Ok(())
    }
}
