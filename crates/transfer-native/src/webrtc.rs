//! Native WebRTC orchestration shared by the CLI sender and receiver.
//!
//! Signaling is intentionally non-trickle: the server persists exactly one SDP
//! offer and answer, so local candidate gathering must finish before either is
//! published. Payload records retain Filebeam's application framing above the
//! reliable ordered SCTP channel.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use bytes::BytesMut;
use filebeam_transfer::{
    WebRtcControl, WebRtcFrame, encode_webrtc_control, encode_webrtc_frame, parse_webrtc_control,
    parse_webrtc_frame,
};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, de::DeserializeOwned};
use tokio::{
    sync::{Mutex, Notify, mpsc},
    time::timeout,
};
use webrtc::{
    data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit},
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
        RTCIceGatheringState, RTCIceServer, RTCIceTransportPolicy, RTCSessionDescription,
        RTCStatsReportEntry, StatsSelector,
    },
};

pub const CHANNEL_LABEL: &str = "filebeam";
pub const METADATA_CHANNEL_LABEL: &str = "filebeam.metadata";
pub const CONTROL_LIMIT: usize = 1024;
pub const FRAME_PAYLOAD_BYTES: usize = 16 * 1024;
pub const METADATA_MESSAGE_BYTES: usize = 16 * 1024;
pub const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;
pub const SDP_TIMEOUT: Duration = Duration::from_secs(10);
/// The signaling protocol has no abort endpoint for a receiver registration,
/// so bound the whole receiver handshake rather than only its poll interval.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// A peer cannot keep a metadata operation alive indefinitely by dribbling
/// valid fragments just before the per-frame timeout.
pub const METADATA_DEADLINE: Duration = Duration::from_secs(30);
pub const SIGNALING_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct Signaling {
    client: Client,
    instance: String,
    transfer_id: String,
}

/// Registered-device rendezvous. Its requests carry only connection identity
/// and SDP; offers, manifests, keys, progress, and receipts never leave the
/// authenticated peer channels.
#[derive(Clone)]
pub struct DeviceSignaling {
    client: Client,
    instance: String,
    credential: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DeviceConnection {
    pub id: String,
    pub source_device_id: String,
    pub target_device_id: String,
    pub offer: Option<Description>,
    pub answer: Option<Description>,
    pub phase: String,
    pub expires_at: String,
}

#[derive(Deserialize)]
struct DeviceConnectionResponse {
    connection: DeviceConnection,
}

#[derive(Clone, Debug)]
pub struct DeviceInboundConnections {
    pub connections: Vec<DeviceConnection>,
    pub poll_after_seconds: u32,
}

#[derive(Deserialize)]
struct DeviceInboundConnectionsResponse {
    connections: Vec<DeviceConnection>,
    poll_after_seconds: u32,
}

#[derive(Deserialize)]
struct DeviceIceResponse {
    ice_servers: Vec<IceServer>,
}

impl DeviceSignaling {
    pub fn new(client: Client, instance: &str, credential: &str) -> Result<Self> {
        if instance.is_empty()
            || credential.len() != 64
            || !credential.bytes().all(|byte| byte.is_ascii_alphanumeric())
        {
            bail!("invalid device signaling authority")
        }
        Ok(Self {
            client,
            instance: instance.trim_end_matches('/').into(),
            credential: credential.into(),
        })
    }

    fn endpoint(&self, suffix: &str) -> String {
        format!(
            "{}/api/native/v1/devices/connections{suffix}",
            self.instance
        )
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        timeout(SIGNALING_TIMEOUT, request.timeout(SIGNALING_TIMEOUT).send())
            .await
            .context("device rendezvous request timed out")?
            .context("send device rendezvous request")
    }

    async fn connection(
        &self,
        response: reqwest::Response,
        expected: StatusCode,
    ) -> Result<DeviceConnection> {
        if response.status() != expected {
            bail!("device rendezvous returned {}", response.status())
        }
        timeout(
            SIGNALING_TIMEOUT,
            response.json::<Api<DeviceConnectionResponse>>(),
        )
        .await
        .context("device rendezvous response body timed out")?
        .context("read device rendezvous response body")
        .map(|value| value.data.connection)
    }

    pub async fn create(&self, target_device_id: &str) -> Result<DeviceConnection> {
        self.connection(
            self.send(
                self.client
                    .post(self.endpoint(""))
                    .bearer_auth(&self.credential)
                    .json(&serde_json::json!({"target_device_id": target_device_id})),
            )
            .await?,
            StatusCode::CREATED,
        )
        .await
    }

    pub async fn publish_offer(&self, id: &str, offer: &Description) -> Result<()> {
        offer.validate("offer")?;
        let response = self
            .send(
                self.client
                    .put(self.endpoint(&format!("/{id}/offer")))
                    .bearer_auth(&self.credential)
                    .json(&serde_json::json!({"description": offer})),
            )
            .await?;
        if response.status() != StatusCode::NO_CONTENT {
            bail!("could not publish device offer: {}", response.status())
        }
        Ok(())
    }

    pub async fn publish_answer(&self, id: &str, answer: &Description) -> Result<()> {
        answer.validate("answer")?;
        let response = self
            .send(
                self.client
                    .put(self.endpoint(&format!("/{id}/answer")))
                    .bearer_auth(&self.credential)
                    .json(&serde_json::json!({"description": answer})),
            )
            .await?;
        if response.status() != StatusCode::NO_CONTENT {
            bail!("could not publish device answer: {}", response.status())
        }
        Ok(())
    }

    pub async fn poll(&self, id: &str) -> Result<DeviceConnection> {
        self.connection(
            self.send(
                self.client
                    .get(self.endpoint(&format!("/{id}")))
                    .bearer_auth(&self.credential),
            )
            .await?,
            StatusCode::OK,
        )
        .await
    }

    /// Poll only rendezvous records addressed to this active device. The
    /// backend response contains no file or offer metadata beyond SDP state.
    pub async fn incoming(&self) -> Result<DeviceInboundConnections> {
        let response = self
            .send(
                self.client
                    .get(self.endpoint(""))
                    .bearer_auth(&self.credential),
            )
            .await?;
        if response.status() != StatusCode::OK {
            bail!(
                "could not poll incoming device rendezvous: {}",
                response.status()
            )
        }
        let response: Api<DeviceInboundConnectionsResponse> =
            timeout(SIGNALING_TIMEOUT, response.json())
                .await
                .context("device rendezvous response body timed out")?
                .context("read device rendezvous response body")?;
        if response.data.connections.len() > 32
            || response.data.poll_after_seconds == 0
            || response.data.poll_after_seconds > 30
        {
            bail!("invalid incoming device rendezvous response")
        }
        Ok(DeviceInboundConnections {
            connections: response.data.connections,
            poll_after_seconds: response.data.poll_after_seconds,
        })
    }

    /// Fetch the existing STUN/TURN configuration for one authorized device
    /// rendezvous. Credentials are never included in SDP or peer metadata.
    pub async fn ice_servers(&self, id: &str) -> Result<Vec<IceServer>> {
        let response = self
            .send(
                self.client
                    .get(self.endpoint(&format!("/{id}/ice")))
                    .bearer_auth(&self.credential),
            )
            .await?;
        if response.status() != StatusCode::OK {
            bail!("could not fetch device ICE servers: {}", response.status())
        }
        let response: Api<DeviceIceResponse> = timeout(SIGNALING_TIMEOUT, response.json())
            .await
            .context("device ICE response body timed out")?
            .context("read device ICE response body")?;
        if response.data.ice_servers.len() > 16 {
            bail!("device ICE response has too many servers")
        }
        Ok(response.data.ice_servers)
    }

    pub async fn end(&self, id: &str) -> Result<()> {
        let response = self
            .send(
                self.client
                    .delete(self.endpoint(&format!("/{id}")))
                    .bearer_auth(&self.credential),
            )
            .await?;
        if response.status() != StatusCode::NO_CONTENT {
            bail!("could not end device rendezvous: {}", response.status())
        }
        Ok(())
    }
}

/// ICE selection for a registered-device initiator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceIcePolicy {
    /// Obtain the connection's short-lived, authorized ICE configuration.
    Authorized,
    /// Use host candidates only. This is explicit so an empty override never
    /// silently disables STUN/TURN.
    DirectOnly,
}

/// Create a registered-device rendezvous and establish both existing Filebeam
/// channels. An empty `servers` override obtains the connection's authorized
/// ICE configuration; a non-empty override is used as supplied.
/// Authentication and private metadata exchange are deliberately a separate
/// subsequent step.
pub async fn connect_device_initiator(
    signaling: &DeviceSignaling,
    target_device_id: &str,
    servers: &[IceServer],
    relay_only: bool,
) -> Result<(NativePeer, DeviceConnection, DeviceChannels)> {
    connect_device_initiator_with_ice_policy(
        signaling,
        target_device_id,
        servers,
        relay_only,
        DeviceIcePolicy::Authorized,
    )
    .await
}

/// As [`connect_device_initiator`], with an explicit empty-override policy.
pub async fn connect_device_initiator_with_ice_policy(
    signaling: &DeviceSignaling,
    target_device_id: &str,
    servers: &[IceServer],
    relay_only: bool,
    ice_policy: DeviceIcePolicy,
) -> Result<(NativePeer, DeviceConnection, DeviceChannels)> {
    let connection = signaling.create(target_device_id).await?;
    let result = async {
        let servers = if servers.is_empty() && ice_policy == DeviceIcePolicy::Authorized {
            signaling.ice_servers(&connection.id).await?
        } else {
            servers.to_vec()
        };
        let peer = NativePeer::new(&servers, relay_only).await?;
        let setup = async {
            let (offer, channels) = peer.offer_device_channels().await?;
            signaling.publish_offer(&connection.id, &offer).await?;
            let answered =
                wait_for_device_connection(signaling, &connection.id, |entry| entry.answer.clone())
                    .await?;
            peer.accept_answer(
                answered
                    .answer
                    .as_ref()
                    .expect("wait predicate supplied an answer"),
            )
            .await?;
            wait_for_open(&channels.file).await?;
            wait_for_open(&channels.metadata).await?;
            Ok::<_, anyhow::Error>(channels)
        }
        .await;
        match setup {
            Ok(channels) => Ok((peer, channels)),
            Err(error) => {
                peer.close().await;
                Err(error)
            }
        }
    }
    .await;
    match result {
        Ok((peer, channels)) => Ok((peer, connection, channels)),
        Err(error) => {
            let _ = signaling.end(&connection.id).await;
            Err(error)
        }
    }
}

/// Accept a known rendezvous ID. The backend authorizes the caller as the
/// target device; a guessed ID cannot reveal the offer or create a channel.
pub async fn connect_device_responder(
    signaling: &DeviceSignaling,
    connection_id: &str,
    servers: &[IceServer],
    relay_only: bool,
) -> Result<(NativePeer, DeviceConnection, DeviceChannels)> {
    let connection =
        wait_for_device_connection(signaling, connection_id, |entry| entry.offer.clone()).await?;
    let offer = connection
        .offer
        .clone()
        .expect("wait predicate supplied an offer");
    let peer = NativePeer::new(servers, relay_only).await?;
    let result = async {
        let answer = peer.answer(&offer).await?;
        signaling.publish_answer(&connection.id, &answer).await?;
        let channels = peer.device_receiver_channels().await?;
        wait_for_open(&channels.file).await?;
        wait_for_open(&channels.metadata).await?;
        Ok(channels)
    }
    .await;
    match result {
        Ok(channels) => Ok((peer, connection, channels)),
        Err(error) => {
            peer.close().await;
            Err(error)
        }
    }
}

async fn wait_for_device_connection<T>(
    signaling: &DeviceSignaling,
    id: &str,
    select: impl Fn(&DeviceConnection) -> Option<T>,
) -> Result<DeviceConnection> {
    let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
    loop {
        let entry = signaling.poll(id).await?;
        if select(&entry).is_some() {
            return Ok(entry);
        }
        if matches!(entry.phase.as_str(), "ended" | "expired") {
            bail!("device rendezvous ended")
        }
        tokio::time::timeout_at(deadline, tokio::time::sleep(Duration::from_millis(500)))
            .await
            .context("timed out waiting for device rendezvous")?;
    }
}

#[derive(Clone, Debug)]
pub struct SenderSession {
    pub id: String,
    pub offer: Option<Description>,
    pub status: String,
    pub progress: u8,
}

#[derive(Clone, Debug)]
pub struct ReceiverSession {
    pub id: String,
    pub token: String,
    pub ice_servers: Vec<IceServer>,
}

#[derive(Deserialize)]
struct Api<T> {
    data: T,
}

#[derive(Deserialize)]
struct SenderSessions {
    sessions: Vec<SenderSessionWire>,
    ice_servers: Vec<IceServer>,
}
#[derive(Deserialize)]
struct SenderSessionWire {
    id: String,
    offer: Option<Description>,
    status: String,
    #[serde(default)]
    progress: u8,
}
#[derive(Deserialize)]
struct ReceiverSessionWire {
    id: String,
    token: String,
    ice_servers: Vec<IceServer>,
}
#[derive(Deserialize)]
struct ReceiverPoll {
    answer: Option<Description>,
    status: String,
}

impl Signaling {
    pub fn new(client: Client, instance: &str, transfer_id: &str) -> Result<Self> {
        if instance.is_empty() || transfer_id.is_empty() {
            bail!("invalid WebRTC signaling target");
        }
        Ok(Self {
            client,
            instance: instance.trim_end_matches('/').into(),
            transfer_id: transfer_id.into(),
        })
    }

    fn endpoint(&self, suffix: &str) -> String {
        format!(
            "{}/api/v1/transfers/{}/webrtc{}",
            self.instance, self.transfer_id, suffix
        )
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        timeout(SIGNALING_TIMEOUT, request.timeout(SIGNALING_TIMEOUT).send())
            .await
            .context("WebRTC signaling request timed out")?
            .context("send WebRTC signaling request")
    }

    async fn json<T: DeserializeOwned>(&self, response: reqwest::Response) -> Result<T> {
        timeout(SIGNALING_TIMEOUT, response.json())
            .await
            .context("WebRTC signaling response body timed out")?
            .context("read WebRTC signaling response body")
    }

    pub async fn publish(&self, upload_token: &str, encrypted_manifest: &str) -> Result<()> {
        let response = self
            .send(
                self.client
                    .put(self.endpoint("/publish"))
                    .header("X-Filebeam-Upload-Token", upload_token)
                    .json(&serde_json::json!({"encrypted_manifest": encrypted_manifest})),
            )
            .await?;
        if !response.status().is_success() {
            bail!("could not publish live transfer: {}", response.status());
        }
        Ok(())
    }

    pub async fn sender_sessions(
        &self,
        upload_token: &str,
    ) -> Result<(Vec<SenderSession>, Vec<IceServer>)> {
        let response = self
            .send(
                self.client
                    .get(self.endpoint("/sessions"))
                    .header("X-Filebeam-Upload-Token", upload_token),
            )
            .await?;
        if !response.status().is_success() {
            bail!(
                "could not poll live receiver sessions: {}",
                response.status()
            );
        }
        let page: Api<SenderSessions> = self.json(response).await?;
        let sessions = page
            .data
            .sessions
            .into_iter()
            .map(|session| SenderSession {
                id: session.id,
                offer: session.offer,
                status: session.status,
                progress: session.progress,
            })
            .collect();
        Ok((sessions, page.data.ice_servers))
    }

    pub async fn answer(
        &self,
        upload_token: &str,
        session_id: &str,
        answer: &Description,
    ) -> Result<()> {
        answer.validate("answer")?;
        let response = self
            .send(
                self.client
                    .put(self.endpoint(&format!("/sessions/{session_id}/answer")))
                    .header("X-Filebeam-Upload-Token", upload_token)
                    .json(&serde_json::json!({"description": answer})),
            )
            .await?;
        if response.status() != StatusCode::NO_CONTENT {
            bail!("could not publish WebRTC answer: {}", response.status());
        }
        Ok(())
    }

    pub async fn register(&self, join_token: &str) -> Result<ReceiverSession> {
        let response = self
            .send(
                self.client
                    .post(self.endpoint("/sessions"))
                    .header("X-Filebeam-Join-Token", join_token)
                    .json(&serde_json::json!({})),
            )
            .await?;
        if response.status() != StatusCode::CREATED {
            bail!("could not register WebRTC receiver: {}", response.status());
        }
        let registered: Api<ReceiverSessionWire> = self.json(response).await?;
        Ok(ReceiverSession {
            id: registered.data.id,
            token: registered.data.token,
            ice_servers: registered.data.ice_servers,
        })
    }

    pub async fn offer(&self, session: &ReceiverSession, offer: &Description) -> Result<()> {
        offer.validate("offer")?;
        let response = self
            .send(
                self.client
                    .put(self.endpoint(&format!("/sessions/{}/offer", session.id)))
                    .header("X-Filebeam-Session-Token", &session.token)
                    .json(&serde_json::json!({"description": offer})),
            )
            .await?;
        if response.status() != StatusCode::NO_CONTENT {
            bail!("could not publish WebRTC offer: {}", response.status());
        }
        Ok(())
    }

    pub async fn receiver_status(
        &self,
        session: &ReceiverSession,
    ) -> Result<(Option<Description>, String)> {
        let response = self
            .send(
                self.client
                    .get(self.endpoint(&format!("/sessions/{}", session.id)))
                    .header("X-Filebeam-Session-Token", &session.token),
            )
            .await?;
        if !response.status().is_success() {
            bail!("live sender is unavailable: {}", response.status());
        }
        let page: Api<ReceiverPoll> = self.json(response).await?;
        Ok((page.data.answer, page.data.status))
    }

    pub async fn report(
        &self,
        session: &ReceiverSession,
        status: &str,
        progress: u8,
    ) -> Result<()> {
        if !matches!(status, "active" | "completed" | "cancelled" | "failed") {
            bail!("invalid WebRTC receiver status");
        }
        let response = self
            .send(
                self.client
                    .patch(self.endpoint(&format!("/sessions/{}", session.id)))
                    .header("X-Filebeam-Session-Token", &session.token)
                    .json(&serde_json::json!({"status": status, "progress": progress})),
            )
            .await?;
        if response.status() != StatusCode::NO_CONTENT {
            bail!(
                "could not report WebRTC receiver status: {}",
                response.status()
            );
        }
        Ok(())
    }

    pub async fn end(&self, upload_token: &str) -> Result<()> {
        let response = self
            .send(
                self.client
                    .post(self.endpoint("/end"))
                    .header("X-Filebeam-Upload-Token", upload_token)
                    .json(&serde_json::json!({})),
            )
            .await?;
        if response.status() != StatusCode::NO_CONTENT {
            bail!("could not end live transfer: {}", response.status());
        }
        Ok(())
    }
}

/// Register a receiver, publish one fully-gathered offer, and wait for the
/// sender's immutable answer. The returned channel is the receiver-created,
/// reliable ordered `filebeam` channel.
pub async fn connect_receiver(
    signaling: &Signaling,
    join_token: &str,
    relay_only: bool,
) -> Result<(NativePeer, ReceiverSession, Arc<dyn DataChannel>)> {
    connect_receiver_with_ice_override(signaling, join_token, relay_only, None).await
}

/// As `connect_receiver`, with a policy held by the owning transfer control.
pub async fn connect_receiver_with_ice_override(
    signaling: &Signaling,
    join_token: &str,
    relay_only: bool,
    ice_override: Option<crate::ice::IceOverride>,
) -> Result<(NativePeer, ReceiverSession, Arc<dyn DataChannel>)> {
    connect_receiver_cancellable_with_ice_override(
        signaling,
        join_token,
        relay_only,
        Arc::new(AtomicBool::new(false)),
        ice_override,
    )
    .await
}

/// Register a receiver and establish its data channel, aborting each signaling
/// stage when `cancelled` is set. On cancellation or timeout the peer is closed
/// before this function returns.
pub async fn connect_receiver_cancellable(
    signaling: &Signaling,
    join_token: &str,
    relay_only: bool,
    cancelled: Arc<AtomicBool>,
) -> Result<(NativePeer, ReceiverSession, Arc<dyn DataChannel>)> {
    connect_receiver_cancellable_with_ice_override(
        signaling, join_token, relay_only, cancelled, None,
    )
    .await
}

/// Cancellable receiver setup with a policy owned by one transfer control.
pub async fn connect_receiver_cancellable_with_ice_override(
    signaling: &Signaling,
    join_token: &str,
    relay_only: bool,
    cancelled: Arc<AtomicBool>,
    ice_override: Option<crate::ice::IceOverride>,
) -> Result<(NativePeer, ReceiverSession, Arc<dyn DataChannel>)> {
    let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
    let session = tokio::select! {
        session = signaling.register(join_token) => session?,
        _ = wait_for_cancellation(&cancelled) => bail!("Transfer cancelled"),
        _ = tokio::time::sleep_until(deadline) => bail!("timed out connecting WebRTC receiver"),
    };
    let servers = ice_override
        .as_ref()
        .map(|policy| policy.resolve(&session.ice_servers))
        .transpose()?
        .unwrap_or_else(|| session.ice_servers.clone());
    let peer = tokio::select! {
        peer = NativePeer::new(&servers, relay_only) => peer?,
        _ = wait_for_cancellation(&cancelled) => bail!("Transfer cancelled"),
        _ = tokio::time::sleep_until(deadline) => bail!("timed out connecting WebRTC receiver"),
    };
    let result = tokio::time::timeout_at(deadline, async {
        let (offer, channel) = tokio::select! {
            offer = peer.offer_channel() => offer?,
            _ = wait_for_cancellation(&cancelled) => bail!("Transfer cancelled"),
        };
        tokio::select! {
            result = signaling.offer(&session, &offer) => result?,
            _ = wait_for_cancellation(&cancelled) => bail!("Transfer cancelled"),
        }
        loop {
            let (answer, status) = tokio::select! {
                status = signaling.receiver_status(&session) => status?,
                _ = wait_for_cancellation(&cancelled) => bail!("Transfer cancelled"),
            };
            if let Some(answer) = answer {
                tokio::select! {
                    result = peer.accept_answer(&answer) => result?,
                    _ = wait_for_cancellation(&cancelled) => bail!("Transfer cancelled"),
                }
                tokio::select! {
                    result = wait_for_open(&channel) => result?,
                    _ = wait_for_cancellation(&cancelled) => bail!("Transfer cancelled"),
                }
                return Ok(channel);
            }
            if matches!(status.as_str(), "cancelled" | "completed" | "failed") {
                bail!("live sender ended the WebRTC session ({status})");
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(1800)) => {},
                _ = wait_for_cancellation(&cancelled) => bail!("Transfer cancelled"),
            }
        }
    })
    .await
    .context("timed out connecting WebRTC receiver")?;
    match result {
        Ok(channel) => Ok((peer, session, channel)),
        Err(error) => {
            peer.close().await;
            Err(error)
        }
    }
}

/// Answer a browser/native receiver offer and return the accepted channel.
/// Callers must keep the returned peer alive while serving chunk requests.
pub async fn connect_sender(
    signaling: &Signaling,
    upload_token: &str,
    session: &SenderSession,
    servers: &[IceServer],
    relay_only: bool,
    cancellation: Arc<AtomicBool>,
) -> Result<(NativePeer, Arc<dyn DataChannel>)> {
    let offer = session
        .offer
        .as_ref()
        .context("receiver has not published an offer")?;
    let peer = NativePeer::new(servers, relay_only).await?;
    let result = async {
        let answer = tokio::select! {
            answer = peer.answer(offer) => answer?,
            _ = wait_for_cancellation(&cancellation) => bail!("Transfer cancelled"),
        };
        tokio::select! {
            result = signaling.answer(upload_token, &session.id, &answer) => result?,
            _ = wait_for_cancellation(&cancellation) => bail!("Transfer cancelled"),
        }
        tokio::select! {
            channel = peer.receiver_channel() => channel,
            _ = wait_for_cancellation(&cancellation) => bail!("Transfer cancelled"),
        }
    }
    .await;
    match result {
        Ok(channel) => Ok((peer, channel)),
        Err(error) => {
            peer.close().await;
            Err(error)
        }
    }
}

async fn wait_for_cancellation(cancellation: &AtomicBool) {
    while !cancellation.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub credential: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Description {
    #[serde(rename = "type")]
    pub kind: String,
    pub sdp: String,
}

impl Description {
    pub fn validate(&self, kind: &str) -> Result<()> {
        if self.kind != kind
            || self.sdp.is_empty()
            || self.sdp.len() > 65_536
            || !self
                .sdp
                .bytes()
                .all(|byte| matches!(byte, b'\t' | b'\n' | b'\r' | b' '..=b'~'))
        {
            bail!("invalid WebRTC {kind} SDP");
        }
        if !self.sdp.contains("a=candidate:") {
            bail!("WebRTC {kind} SDP has no gathered ICE candidates");
        }
        Ok(())
    }

    fn rtc(&self) -> Result<RTCSessionDescription> {
        serde_json::from_value(serde_json::json!({"type": self.kind, "sdp": self.sdp}))
            .context("invalid WebRTC session description")
    }
}

#[derive(Clone)]
struct Handler {
    gathered: Arc<Notify>,
    gathering_complete: Arc<AtomicBool>,
    channels: mpsc::Sender<Arc<dyn DataChannel>>,
}

#[async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.gathering_complete.store(true, Ordering::Release);
            self.gathered.notify_one();
        }
    }

    async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) {
        let _ = self.channels.send(channel).await;
    }
}

pub struct NativePeer {
    pub connection: Arc<dyn PeerConnection>,
    gathered: Arc<Notify>,
    gathering_complete: Arc<AtomicBool>,
    incoming: Mutex<mpsc::Receiver<Arc<dyn DataChannel>>>,
}

/// The existing file channel plus the bounded device-only metadata companion.
/// Public transfers continue using only `filebeam`.
pub struct DeviceChannels {
    pub file: Arc<dyn DataChannel>,
    pub metadata: Arc<dyn DataChannel>,
}

impl NativePeer {
    pub async fn new(servers: &[IceServer], relay_only: bool) -> Result<Self> {
        // webrtc expands the wildcard into routable non-loopback interfaces,
        // so retain an explicit loopback socket for local native peers as well.
        Self::with_udp_addrs_and_policy(
            servers,
            vec!["0.0.0.0:0".to_owned(), "127.0.0.1:0".to_owned()],
            relay_only,
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn with_udp_addrs(
        servers: &[IceServer],
        udp_addrs: Vec<String>,
    ) -> Result<Self> {
        Self::with_udp_addrs_and_policy(servers, udp_addrs, false).await
    }

    async fn with_udp_addrs_and_policy(
        servers: &[IceServer],
        udp_addrs: Vec<String>,
        relay_only: bool,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::channel(1);
        let gathered = Arc::new(Notify::new());
        let gathering_complete = Arc::new(AtomicBool::new(false));
        let ice_servers = rtc_ice_servers(servers, relay_only)?;
        let mut configuration = RTCConfigurationBuilder::new().with_ice_servers(ice_servers);
        if relay_only {
            configuration = configuration.with_ice_transport_policy(RTCIceTransportPolicy::Relay);
        }
        let configuration = configuration.build();
        let connection: Arc<dyn PeerConnection> = Arc::new(
            PeerConnectionBuilder::new()
                .with_configuration(configuration)
                .with_handler(Arc::new(Handler {
                    gathered: gathered.clone(),
                    gathering_complete: gathering_complete.clone(),
                    channels: sender,
                }))
                .with_udp_addrs(udp_addrs)
                .build()
                .await
                .context("create WebRTC peer connection")?,
        );
        Ok(Self {
            connection,
            gathered,
            gathering_complete,
            incoming: Mutex::new(receiver),
        })
    }

    pub async fn offer(&self) -> Result<Description> {
        self.offer_channel()
            .await
            .map(|(description, _)| description)
    }

    /// Creates the sole reliable, ordered Filebeam channel before generating a
    /// non-trickle offer. The caller retains this handle to serve requests once
    /// the answer has been applied.
    pub async fn offer_channel(&self) -> Result<(Description, Arc<dyn DataChannel>)> {
        let channel = self
            .connection
            .create_data_channel(CHANNEL_LABEL, Some(RTCDataChannelInit::default()))
            .await?;
        validate_channel(&channel).await?;
        let offer = self.connection.create_offer(None).await?;
        self.connection.set_local_description(offer).await?;
        Ok((self.local("offer").await?, channel))
    }

    /// Create both channels before the non-trickle offer. The existing file
    /// channel keeps its label and framing; metadata cannot enter its parser.
    pub async fn offer_device_channels(&self) -> Result<(Description, DeviceChannels)> {
        let file = self
            .connection
            .create_data_channel(CHANNEL_LABEL, Some(RTCDataChannelInit::default()))
            .await?;
        let metadata = self
            .connection
            .create_data_channel(METADATA_CHANNEL_LABEL, Some(RTCDataChannelInit::default()))
            .await?;
        validate_named_channel(&file, CHANNEL_LABEL).await?;
        validate_named_channel(&metadata, METADATA_CHANNEL_LABEL).await?;
        let offer = self.connection.create_offer(None).await?;
        self.connection.set_local_description(offer).await?;
        Ok((
            self.local("offer").await?,
            DeviceChannels { file, metadata },
        ))
    }

    pub async fn answer(&self, offer: &Description) -> Result<Description> {
        offer.validate("offer")?;
        self.connection.set_remote_description(offer.rtc()?).await?;
        let answer = self.connection.create_answer(None).await?;
        self.connection.set_local_description(answer).await?;
        self.local("answer").await
    }

    async fn local(&self, kind: &str) -> Result<Description> {
        if !self.gathering_complete.load(Ordering::Acquire) {
            timeout(SDP_TIMEOUT, self.gathered.notified())
                .await
                .context("timed out gathering ICE candidates")?;
        }
        let local = self
            .connection
            .local_description()
            .await
            .context("local WebRTC SDP is unavailable")?;
        let result = Description {
            kind: kind.into(),
            sdp: local.sdp,
        };
        result.validate(kind)?;
        Ok(result)
    }

    pub async fn accept_answer(&self, answer: &Description) -> Result<()> {
        answer.validate("answer")?;
        self.connection
            .set_remote_description(answer.rtc()?)
            .await?;
        Ok(())
    }

    pub async fn receiver_channel(&self) -> Result<Arc<dyn DataChannel>> {
        let channel = timeout(CONNECT_TIMEOUT, self.incoming.lock().await.recv())
            .await
            .context("timed out waiting for WebRTC data channel")?
            .context("WebRTC peer closed before opening a data channel")?;
        validate_channel(&channel).await?;
        Ok(channel)
    }

    /// Accept exactly the two channels required for a device session. An
    /// unexpected or duplicate channel fails closed before metadata is read.
    pub async fn device_receiver_channels(&self) -> Result<DeviceChannels> {
        let mut file = None;
        let mut metadata = None;
        while file.is_none() || metadata.is_none() {
            let channel = timeout(CONNECT_TIMEOUT, self.incoming.lock().await.recv())
                .await
                .context("timed out waiting for WebRTC device data channels")?
                .context("WebRTC peer closed before opening device data channels")?;
            match channel.label().await?.as_str() {
                CHANNEL_LABEL if file.is_none() => {
                    validate_named_channel(&channel, CHANNEL_LABEL).await?;
                    file = Some(channel);
                }
                METADATA_CHANNEL_LABEL if metadata.is_none() => {
                    validate_named_channel(&channel, METADATA_CHANNEL_LABEL).await?;
                    metadata = Some(channel);
                }
                _ => bail!("unexpected or duplicate WebRTC device data channel"),
            }
        }
        Ok(DeviceChannels {
            file: file.expect("device file channel was checked"),
            metadata: metadata.expect("device metadata channel was checked"),
        })
    }

    /// Returns the certificates the DTLS stack actually negotiated. These are
    /// intentionally read from transport statistics after connection, rather
    /// than accepting signaling SDP fingerprint strings.
    pub async fn certificate_fingerprints(&self) -> Result<([u8; 32], [u8; 32])> {
        let report = self
            .connection
            .get_stats(Instant::now(), StatsSelector::None)
            .await;
        let mut local_id = None;
        let mut remote_id = None;
        for entry in report.iter() {
            if let RTCStatsReportEntry::Transport(transport) = entry
                && !transport.local_certificate_id.is_empty()
                && !transport.remote_certificate_id.is_empty()
            {
                local_id = Some(transport.local_certificate_id.as_str());
                remote_id = Some(transport.remote_certificate_id.as_str());
                break;
            }
        }
        let local_id = local_id.context("DTLS local certificate is unavailable")?;
        let remote_id = remote_id.context("DTLS remote certificate is unavailable")?;
        let mut local = None;
        let mut remote = None;
        for entry in report.iter() {
            if let RTCStatsReportEntry::Certificate(certificate) = entry {
                if certificate.stats.id == local_id {
                    local = Some(parse_certificate_fingerprint(&certificate.fingerprint)?);
                }
                if certificate.stats.id == remote_id {
                    remote = Some(parse_certificate_fingerprint(&certificate.fingerprint)?);
                }
            }
        }
        Ok((
            local.context("DTLS local certificate fingerprint is unavailable")?,
            remote.context("DTLS remote certificate fingerprint is unavailable")?,
        ))
    }

    pub async fn close(&self) {
        let _ = self.connection.close().await;
    }
}

/// webrtc 0.20.5 allocates TURN relays only over UDP. Filter the backend's
/// short-lived ICE response so relay-only mode cannot silently select an
/// unsupported TCP or TLS URL.
fn rtc_ice_servers(servers: &[IceServer], relay_only: bool) -> Result<Vec<RTCIceServer>> {
    let mut usable_turn_urls = 0;
    let servers = servers
        .iter()
        .filter_map(|server| {
            let urls = if relay_only {
                server
                    .urls
                    .iter()
                    .filter(|url| is_turn_udp_url(url))
                    .cloned()
                    .collect::<Vec<_>>()
            } else {
                server.urls.clone()
            };
            usable_turn_urls += urls.len();
            (!urls.is_empty()).then(|| RTCIceServer {
                urls,
                username: server.username.clone().unwrap_or_default(),
                credential: server.credential.clone().unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    if relay_only && usable_turn_urls == 0 {
        bail!(
            "WebRTC relay-only requires a turn: UDP ICE URL; webrtc 0.20.5 does not support TURN over TCP or TLS"
        );
    }
    Ok(servers)
}

fn is_turn_udp_url(url: &str) -> bool {
    let Some((scheme, remainder)) = url.split_once(':') else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("turn") {
        return false;
    }
    remainder
        .split_once('?')
        .map(|(_, query)| {
            query.split('&').all(|parameter| {
                let Some((name, value)) = parameter.split_once('=') else {
                    return true;
                };
                !name.eq_ignore_ascii_case("transport") || value.eq_ignore_ascii_case("udp")
            })
        })
        .unwrap_or(true)
}

async fn validate_channel(channel: &Arc<dyn DataChannel>) -> Result<()> {
    validate_named_channel(channel, CHANNEL_LABEL).await
}

async fn validate_named_channel(channel: &Arc<dyn DataChannel>, label: &str) -> Result<()> {
    if channel.label().await? != label {
        bail!("unexpected WebRTC data channel label");
    }
    if !channel.ordered().await?
        || channel.max_retransmits().await?.is_some()
        || channel.max_packet_life_time().await?.is_some()
        || channel.negotiated().await?
    {
        bail!("WebRTC data channel must be reliable, ordered, and in-band");
    }
    Ok(())
}

fn parse_certificate_fingerprint(value: &str) -> Result<[u8; 32]> {
    let mut fingerprint = [0u8; 32];
    let mut values = value.split(':');
    for byte in &mut fingerprint {
        let value = values
            .next()
            .context("DTLS certificate fingerprint has the wrong length")?;
        if value.len() != 2 {
            bail!("DTLS certificate fingerprint is invalid")
        }
        *byte = u8::from_str_radix(value, 16).context("DTLS certificate fingerprint is invalid")?;
    }
    if values.next().is_some() {
        bail!("DTLS certificate fingerprint has the wrong length")
    }
    Ok(fingerprint)
}

const METADATA_MAGIC: u32 = 0x4642_4d44;
const METADATA_HEADER_BYTES: usize = 16;

/// Send a bounded metadata blob on the companion channel. This framing is
/// exclusive to `filebeam.metadata`; it cannot be decoded as a file chunk.
pub async fn send_metadata_blob(
    channel: Arc<dyn DataChannel>,
    stream_id: u32,
    bytes: &[u8],
) -> Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_METADATA_BYTES {
        bail!("invalid device metadata size")
    }
    let total = u32::try_from(bytes.len()).context("device metadata is too large")?;
    let payload = METADATA_MESSAGE_BYTES - METADATA_HEADER_BYTES;
    for (part, chunk) in bytes.chunks(payload).enumerate() {
        let offset = part
            .checked_mul(payload)
            .and_then(|offset| u32::try_from(offset).ok())
            .context("device metadata offset overflow")?;
        let mut message = Vec::with_capacity(METADATA_HEADER_BYTES + chunk.len());
        message.extend_from_slice(&METADATA_MAGIC.to_be_bytes());
        message.extend_from_slice(&stream_id.to_be_bytes());
        message.extend_from_slice(&total.to_be_bytes());
        message.extend_from_slice(&offset.to_be_bytes());
        message.extend_from_slice(chunk);
        channel.send(BytesMut::from(message.as_slice())).await?;
    }
    Ok(())
}

/// Receive one complete, strictly contiguous metadata blob. The caller chooses
/// stream IDs so a proof, offer, or receipt cannot be mistaken for another.
pub async fn receive_metadata_blob(
    channel: Arc<dyn DataChannel>,
    stream_id: u32,
) -> Result<Vec<u8>> {
    receive_metadata_blob_ignoring(channel, stream_id, None).await
}

/// Receive one metadata stream while discarding a previously authenticated,
/// stale stream. This is used only for post-commit receipt acknowledgement,
/// where a duplicated offer cannot change the committed outcome.
pub(crate) async fn receive_metadata_blob_ignoring(
    channel: Arc<dyn DataChannel>,
    stream_id: u32,
    ignored_stream: Option<u32>,
) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut total = None;
    let deadline = tokio::time::Instant::now() + METADATA_DEADLINE;
    loop {
        let event = tokio::time::timeout_at(deadline, channel.poll())
            .await
            .context("device metadata message deadline expired")?;
        let Some(event) = event else {
            bail!("device metadata channel closed")
        };
        match event {
            DataChannelEvent::OnMessage(message) if !message.is_string => {
                if message.data.len() <= METADATA_HEADER_BYTES
                    || message.data.len() > METADATA_MESSAGE_BYTES
                {
                    bail!("invalid device metadata frame length")
                }
                let magic = u32::from_be_bytes(message.data[0..4].try_into()?);
                let received_stream = u32::from_be_bytes(message.data[4..8].try_into()?);
                let received_total = u32::from_be_bytes(message.data[8..12].try_into()?);
                let offset = u32::from_be_bytes(message.data[12..16].try_into()?);
                if magic != METADATA_MAGIC
                    || received_total == 0
                    || received_total as usize > MAX_METADATA_BYTES
                {
                    bail!("invalid device metadata frame")
                }
                if ignored_stream == Some(received_stream) {
                    continue;
                }
                if usize::try_from(offset).ok() != Some(output.len()) {
                    bail!("invalid device metadata frame")
                }
                if received_stream != stream_id {
                    bail!("unexpected device metadata stream")
                }
                if total.get_or_insert(received_total) != &received_total
                    || output
                        .len()
                        .checked_add(message.data.len() - METADATA_HEADER_BYTES)
                        .is_none_or(|length| length > received_total as usize)
                {
                    bail!("invalid device metadata reassembly")
                }
                output.extend_from_slice(&message.data[METADATA_HEADER_BYTES..]);
                if output.len() == received_total as usize {
                    return Ok(output);
                }
            }
            DataChannelEvent::OnClose => bail!("device metadata channel closed"),
            DataChannelEvent::OnMessage(_) => bail!("device metadata must be binary"),
            // Opening and buffered-amount notifications precede the first
            // message on some platforms and carry no peer-controlled data.
            _ => {}
        }
    }
}

/// Send immutable ciphertext for serialized browser-compatible chunk requests.
pub async fn serve_channel(
    channel: Arc<dyn DataChannel>,
    chunks: HashMap<(String, u64), Vec<u8>>,
) -> Result<()> {
    let result = serve_channel_inner(channel.clone(), chunks).await;
    if result.is_err() {
        let _ = channel.close().await;
    }
    result
}

async fn serve_channel_inner(
    channel: Arc<dyn DataChannel>,
    chunks: HashMap<(String, u64), Vec<u8>>,
) -> Result<()> {
    let mut pending: Option<u32> = None;
    loop {
        let event = if pending.is_some() {
            timeout(REQUEST_TIMEOUT, channel.poll())
                .await
                .context("WebRTC chunk acknowledgement timed out")?
        } else {
            channel.poll().await
        };
        let Some(event) = event else { return Ok(()) };
        match event {
            DataChannelEvent::OnMessage(message) if message.is_string => {
                let text =
                    std::str::from_utf8(&message.data).context("invalid WebRTC control UTF-8")?;
                if text.len() > CONTROL_LIMIT {
                    bail!("WebRTC control frame is too large");
                }
                match parse_webrtc_control(text).map_err(anyhow::Error::msg)? {
                    WebRtcControl::Request {
                        seq,
                        item_id,
                        index,
                    } if pending.is_none() => {
                        let ciphertext = chunks
                            .get(&(item_id.clone(), index))
                            .context("unknown WebRTC chunk request")?;
                        let length = u64::try_from(ciphertext.len())?;
                        let chunk = WebRtcControl::chunk(seq, item_id, index, length)
                            .map_err(anyhow::Error::msg)?;
                        channel
                            .send_text(&encode_webrtc_control(&chunk).map_err(anyhow::Error::msg)?)
                            .await?;
                        for (offset, payload) in ciphertext.chunks(FRAME_PAYLOAD_BYTES).enumerate()
                        {
                            let frame = WebRtcFrame::new(
                                seq,
                                u32::try_from(offset * FRAME_PAYLOAD_BYTES)?,
                                payload.to_vec(),
                            )
                            .map_err(anyhow::Error::msg)?;
                            channel
                                .send(BytesMut::from(
                                    encode_webrtc_frame(&frame)
                                        .map_err(anyhow::Error::msg)?
                                        .as_slice(),
                                ))
                                .await?;
                        }
                        pending = Some(seq);
                    }
                    WebRtcControl::Ack { seq } if pending == Some(seq) => pending = None,
                    _ => bail!("unexpected WebRTC control frame"),
                }
            }
            DataChannelEvent::OnClose => return Ok(()),
            _ => {}
        }
    }
}

/// Request a chunk and enforce its declared ciphertext length before returning it.
pub async fn request_chunk(
    channel: Arc<dyn DataChannel>,
    seq: u32,
    item_id: String,
    index: u64,
    expected: u64,
) -> Result<Vec<u8>> {
    let request =
        WebRtcControl::request(seq, item_id.clone(), index).map_err(anyhow::Error::msg)?;
    let result = async {
        channel
            .send_text(&encode_webrtc_control(&request).map_err(anyhow::Error::msg)?)
            .await?;
        let mut bytes = Vec::with_capacity(usize::try_from(expected)?);
        let mut announced = false;
        loop {
            let event = timeout(REQUEST_TIMEOUT, channel.poll())
                .await
                .context("WebRTC chunk response timed out")?;
            let Some(event) = event else {
                bail!("WebRTC data channel closed")
            };
            match event {
                DataChannelEvent::OnMessage(message) if message.is_string => {
                    let text = std::str::from_utf8(&message.data)
                        .context("invalid WebRTC control UTF-8")?;
                    if text.len() > CONTROL_LIMIT {
                        bail!("WebRTC control frame is too large");
                    }
                    match parse_webrtc_control(text).map_err(anyhow::Error::msg)? {
                        WebRtcControl::Chunk {
                            seq: got,
                            item_id: got_id,
                            index: got_index,
                            length,
                        } if !announced
                            && got == seq
                            && got_id == item_id
                            && got_index == index
                            && length == expected =>
                        {
                            announced = true
                        }
                        _ => bail!("unexpected WebRTC chunk response"),
                    }
                }
                DataChannelEvent::OnMessage(message) => {
                    if !announced {
                        bail!("WebRTC binary frame arrived before chunk declaration");
                    }
                    let frame = parse_webrtc_frame(&message.data).map_err(anyhow::Error::msg)?;
                    frame
                        .validate_for_chunk(seq, bytes.len() as u64, expected)
                        .map_err(anyhow::Error::msg)?;
                    bytes.extend_from_slice(&frame.payload);
                    if bytes.len() as u64 == expected {
                        let ack = WebRtcControl::ack(seq);
                        channel
                            .send_text(&encode_webrtc_control(&ack).map_err(anyhow::Error::msg)?)
                            .await?;
                        return Ok(bytes);
                    }
                }
                DataChannelEvent::OnClose => bail!("WebRTC data channel closed"),
                _ => {}
            }
        }
    }
    .await;
    if result.is_err() {
        let _ = channel.close().await;
    }
    result
}

pub(crate) async fn wait_for_open(channel: &Arc<dyn DataChannel>) -> Result<()> {
    timeout(CONNECT_TIMEOUT, async {
        loop {
            match channel.poll().await {
                Some(DataChannelEvent::OnOpen) => return Ok(()),
                Some(DataChannelEvent::OnClose) | None => {
                    bail!("WebRTC data channel closed before opening")
                }
                _ => {}
            }
        }
    })
    .await
    .context("timed out opening WebRTC data channel")?
}

/// An on-device diagnostic for the complete native UDP/ICE/DTLS/data-channel
/// stack. Kept here so Android and future native hosts exercise the real framing.
pub fn loopback_self_test() -> Result<bool> {
    use filebeam_encryption::{
        decrypt_chunk, encrypt_chunk, generate_nonce_prefix, generate_transfer_key,
    };
    crate::runtime::shared_tokio_runtime().block_on(async {
        let receiver = NativePeer::new(&[], false).await?;
        let sender = NativePeer::new(&[], false).await?;
        let outcome = timeout(CONNECT_TIMEOUT, async {
            let (offer, channel) = receiver.offer_channel().await?;
            let answer = sender.answer(&offer).await?;
            receiver.accept_answer(&answer).await?;
            let sending = sender.receiver_channel().await?;
            wait_for_open(&channel).await?;
            wait_for_open(&sending).await?;
            let key = generate_transfer_key()?;
            let prefix = generate_nonce_prefix()?;
            let plain = vec![0x59; FRAME_PAYLOAD_BYTES + 17];
            let cipher = encrypt_chunk(&key, &prefix, 0, &plain, b"native-self-test")?;
            let length = cipher.len() as u64;
            let serving = tokio::spawn(serve_channel(
                sending,
                HashMap::from([(("probe".into(), 0), cipher)]),
            ));
            let received = request_chunk(channel.clone(), 1, "probe".into(), 0, length).await?;
            channel.close().await?;
            let _ = serving.await?;
            Ok::<_, anyhow::Error>(
                decrypt_chunk(&key, &prefix, 0, &received, b"native-self-test")? == plain,
            )
        })
        .await
        .context("local WebRTC diagnostic timed out")?;
        receiver.close().await;
        sender.close().await;
        outcome
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    struct Coturn {
        name: String,
    }

    impl Coturn {
        fn start() -> Result<Self> {
            let name = format!("filebeam-coturn-{}", std::process::id());
            let output = Command::new("docker")
                .args([
                    "run",
                    "--rm",
                    "-d",
                    "--name",
                    &name,
                    "--cpus=2",
                    "--memory=2g",
                    "-p",
                    "127.0.0.1:34780:3478/udp",
                    "-p",
                    "127.0.0.1:49160-49170:49160-49170/udp",
                    "coturn/coturn:4.6.2",
                    "-n",
                    "--log-file=stdout",
                    "--no-cli",
                    "--no-tls",
                    "--no-dtls",
                    "--realm=filebeam.test",
                    "--lt-cred-mech",
                    "--user=filebeam:local-only",
                    "--external-ip=127.0.0.1",
                    "--min-port=49160",
                    "--max-port=49170",
                ])
                .output()
                .context("start local coturn Docker fixture")?;
            if !output.status.success() {
                bail!(
                    "start local coturn Docker fixture: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
            Ok(Self { name })
        }
    }

    impl Drop for Coturn {
        fn drop(&mut self) {
            let _ = Command::new("docker")
                .args(["rm", "-f", &self.name])
                .output();
        }
    }

    #[test]
    fn descriptions_require_complete_nontrickle_sdp() {
        assert!(
            Description {
                kind: "offer".into(),
                sdp: "v=0\r\na=candidate:x\r\n".into()
            }
            .validate("offer")
            .is_ok()
        );
        assert!(
            Description {
                kind: "offer".into(),
                sdp: "v=0\r\n".into()
            }
            .validate("offer")
            .is_err()
        );
    }

    #[test]
    fn relay_only_accepts_only_turn_udp_urls() {
        assert!(is_turn_udp_url("turn:relay.example.test:3478"));
        assert!(is_turn_udp_url(
            "turn:relay.example.test:3478?transport=udp"
        ));
        assert!(!is_turn_udp_url(
            "turn:relay.example.test:3478?transport=tcp"
        ));
        assert!(!is_turn_udp_url(
            "turns:relay.example.test:5349?transport=tcp"
        ));
        assert!(!is_turn_udp_url("stun:relay.example.test:3478"));
        assert!(
            rtc_ice_servers(
                &[IceServer {
                    urls: vec!["turns:relay.example.test:5349".into()],
                    username: None,
                    credential: None,
                }],
                true,
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn receiver_connect_honors_cancellation_before_signaling() -> Result<()> {
        let cancelled = Arc::new(AtomicBool::new(true));
        let signaling = Signaling::new(Client::new(), "http://127.0.0.1:9", "transfer")?;
        let started = tokio::time::Instant::now();
        let error = match connect_receiver_cancellable(&signaling, "join", false, cancelled).await {
            Ok(_) => bail!("cancelled receiver started signaling"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("Transfer cancelled"));
        assert!(started.elapsed() < Duration::from_secs(1));
        Ok(())
    }

    #[tokio::test]
    async fn loopback_pair_requests_frames_and_acknowledges_each_chunk() -> Result<()> {
        let receiver = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".to_owned()]).await?;
        let sender = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".to_owned()]).await?;

        let (offer, receiver_channel) = receiver.offer_channel().await?;
        let answer = sender.answer(&offer).await?;
        receiver.accept_answer(&answer).await?;
        let sender_channel = sender.receiver_channel().await?;

        wait_for_open(&receiver_channel).await?;
        wait_for_open(&sender_channel).await?;

        let tag_only = vec![0xa5; 16];
        let tail_16_401 = (0..16_401)
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        let multiple_frames = (0..(FRAME_PAYLOAD_BYTES * 2 + 16))
            .map(|value| (value % 239) as u8)
            .collect::<Vec<_>>();
        let chunks = HashMap::from([
            (("item".to_owned(), 0), tag_only.clone()),
            (("item".to_owned(), 1), tail_16_401.clone()),
            (("item".to_owned(), 2), multiple_frames.clone()),
        ]);
        let serving = tokio::spawn(serve_channel(sender_channel, chunks));

        assert_eq!(
            request_chunk(receiver_channel.clone(), 7, "item".into(), 0, 16).await?,
            tag_only
        );
        // This request is accepted only after the first request's ACK clears the sender's slot.
        assert_eq!(
            request_chunk(receiver_channel.clone(), 8, "item".into(), 1, 16_401).await?,
            tail_16_401
        );
        assert_eq!(
            request_chunk(
                receiver_channel.clone(),
                9,
                "item".into(),
                2,
                multiple_frames.len() as u64,
            )
            .await?,
            multiple_frames
        );

        receiver_channel.close().await?;
        let _closed = timeout(CONNECT_TIMEOUT, serving)
            .await
            .context("timed out stopping loopback WebRTC sender")??;
        receiver.close().await;
        sender.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn production_native_peers_advertise_loopback_and_open_a_fresh_channel() -> Result<()> {
        let offerer = NativePeer::new(&[], false).await?;
        let answerer = NativePeer::new(&[], false).await?;

        let (offer, offered) = offerer.offer_channel().await?;
        assert!(offer.sdp.contains(" 127.0.0.1 "));
        let answer = answerer.answer(&offer).await?;
        assert!(answer.sdp.contains(" 127.0.0.1 "));
        offerer.accept_answer(&answer).await?;
        let accepted = answerer.receiver_channel().await?;
        wait_for_open(&offered).await?;
        wait_for_open(&accepted).await?;

        offered
            .send(BytesMut::from(&b"native-production-route"[..]))
            .await?;
        loop {
            match timeout(CONNECT_TIMEOUT, accepted.poll())
                .await
                .context("timed out receiving production native channel message")?
            {
                Some(DataChannelEvent::OnMessage(message)) => {
                    assert!(!message.is_string);
                    assert_eq!(&message.data[..], b"native-production-route");
                    break;
                }
                Some(DataChannelEvent::OnClose) | None => {
                    bail!("production native data channel closed before receiving a message")
                }
                _ => {}
            }
        }

        offered.close().await?;
        offerer.close().await;
        answerer.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn device_channels_bind_metadata_to_negotiated_dtls_certificates() -> Result<()> {
        use crate::device::{
            DeviceAuthenticationScope, LocalDevice, PinnedDevice, authenticate_device_connection,
        };
        use ed25519_dalek::SigningKey;

        let offerer = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".to_owned()]).await?;
        let answerer = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".to_owned()]).await?;
        let (offer, offered) = offerer.offer_device_channels().await?;
        let answer = answerer.answer(&offer).await?;
        offerer.accept_answer(&answer).await?;
        let accepted = answerer.device_receiver_channels().await?;
        wait_for_open(&offered.file).await?;
        wait_for_open(&offered.metadata).await?;
        wait_for_open(&accepted.file).await?;
        wait_for_open(&accepted.metadata).await?;

        let (offerer_local, offerer_remote) = offerer.certificate_fingerprints().await?;
        let (answerer_local, answerer_remote) = answerer.certificate_fingerprints().await?;
        assert_eq!(offerer_local, answerer_remote);
        assert_eq!(offerer_remote, answerer_local);
        assert_ne!(offerer_local, offerer_remote);

        let offerer_key = SigningKey::from_bytes(&[7; 32]);
        let answerer_key = SigningKey::from_bytes(&[8; 32]);
        let offerer_pin = PinnedDevice::new("offerer".into(), 1, offerer_key.verifying_key())?;
        let answerer_pin = PinnedDevice::new("answerer".into(), 1, answerer_key.verifying_key())?;
        let offerer_scope = DeviceAuthenticationScope::new(
            "https://filebeam.test".into(),
            "test-1".into(),
            "account-1".into(),
            1,
            LocalDevice::new("offerer".into(), 1)?,
            vec!["metadata-v1".into()],
        )?;
        let answerer_scope = DeviceAuthenticationScope::new(
            "https://filebeam.test".into(),
            "test-1".into(),
            "account-1".into(),
            1,
            LocalDevice::new("answerer".into(), 1)?,
            vec!["metadata-v1".into()],
        )?;
        let (offerer_authority, answerer_authority) = tokio::join!(
            authenticate_device_connection(
                &offerer,
                &offered,
                &offerer_scope,
                &offerer_key,
                &answerer_pin,
                "attempt-1",
            ),
            authenticate_device_connection(
                &answerer,
                &accepted,
                &answerer_scope,
                &answerer_key,
                &offerer_pin,
                "attempt-1",
            ),
        );
        assert_eq!(offerer_authority?.peer_device_id(), "answerer");
        assert_eq!(answerer_authority?.peer_device_id(), "offerer");

        let payload = (0..(METADATA_MESSAGE_BYTES * 2 + 9))
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let receiving = tokio::spawn(receive_metadata_blob(accepted.metadata, 7));
        send_metadata_blob(offered.metadata.clone(), 7, &payload).await?;
        assert_eq!(receiving.await??, payload);

        offered.file.close().await?;
        offered.metadata.close().await?;
        offerer.close().await;
        answerer.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn device_connection_rejects_a_wrong_pinned_key_after_live_hello() -> Result<()> {
        use crate::device::{
            DeviceAuthenticationScope, LocalDevice, PinnedDevice, authenticate_device_connection,
        };
        use ed25519_dalek::SigningKey;

        let offerer = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".to_owned()]).await?;
        let answerer = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".to_owned()]).await?;
        let (offer, offered) = offerer.offer_device_channels().await?;
        let answer = answerer.answer(&offer).await?;
        offerer.accept_answer(&answer).await?;
        let accepted = answerer.device_receiver_channels().await?;

        let offerer_key = SigningKey::from_bytes(&[17; 32]);
        let answerer_key = SigningKey::from_bytes(&[18; 32]);
        let wrong_key = SigningKey::from_bytes(&[19; 32]);
        let offerer_scope = DeviceAuthenticationScope::new(
            "https://filebeam.test".into(),
            "test-1".into(),
            "account-1".into(),
            1,
            LocalDevice::new("offerer".into(), 1)?,
            vec!["metadata-v1".into()],
        )?;
        let answerer_scope = DeviceAuthenticationScope::new(
            "https://filebeam.test".into(),
            "test-1".into(),
            "account-1".into(),
            1,
            LocalDevice::new("answerer".into(), 1)?,
            vec!["metadata-v1".into()],
        )?;
        let wrong_answerer_pin =
            PinnedDevice::new("answerer".into(), 1, wrong_key.verifying_key())?;
        let offerer_pin = PinnedDevice::new("offerer".into(), 1, offerer_key.verifying_key())?;
        let (offerer_result, answerer_result) = tokio::join!(
            authenticate_device_connection(
                &offerer,
                &offered,
                &offerer_scope,
                &offerer_key,
                &wrong_answerer_pin,
                "attempt-2",
            ),
            authenticate_device_connection(
                &answerer,
                &accepted,
                &answerer_scope,
                &answerer_key,
                &offerer_pin,
                "attempt-2",
            ),
        );
        assert!(offerer_result.is_err());
        let _ = answerer_result;
        offerer.close().await;
        answerer.close().await;
        Ok(())
    }

    #[test]
    fn certificate_fingerprint_requires_exact_sha256_hex() -> Result<()> {
        assert_eq!(
            parse_certificate_fingerprint(
                "00:01:02:03:04:05:06:07:08:09:0A:0B:0C:0D:0E:0F:10:11:12:13:14:15:16:17:18:19:1A:1B:1C:1D:1E:1F"
            )?,
            std::array::from_fn(|index| index as u8)
        );
        assert!(parse_certificate_fingerprint("00:01").is_err());
        assert!(parse_certificate_fingerprint(&"00:".repeat(32)).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn loopback_receiver_close_does_not_affect_another_peer() -> Result<()> {
        let closed_receiver =
            NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".to_owned()]).await?;
        let closed_sender = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".to_owned()]).await?;
        let (offer, receiver_channel) = closed_receiver.offer_channel().await?;
        let answer = closed_sender.answer(&offer).await?;
        closed_receiver.accept_answer(&answer).await?;
        let sender_channel = closed_sender.receiver_channel().await?;
        wait_for_open(&receiver_channel).await?;
        wait_for_open(&sender_channel).await?;
        let closed_serving = tokio::spawn(serve_channel(sender_channel, HashMap::new()));
        receiver_channel.close().await?;
        timeout(CONNECT_TIMEOUT, closed_serving)
            .await
            .context("timed out stopping closed loopback receiver")???;
        closed_receiver.close().await;
        closed_sender.close().await;

        let receiver = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".to_owned()]).await?;
        let sender = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".to_owned()]).await?;
        let (offer, receiver_channel) = receiver.offer_channel().await?;
        let answer = sender.answer(&offer).await?;
        receiver.accept_answer(&answer).await?;
        let sender_channel = sender.receiver_channel().await?;
        wait_for_open(&receiver_channel).await?;
        wait_for_open(&sender_channel).await?;
        let ciphertext = vec![7; 16];
        let serving = tokio::spawn(serve_channel(
            sender_channel,
            HashMap::from([(("item".to_owned(), 0), ciphertext.clone())]),
        ));
        assert_eq!(
            request_chunk(receiver_channel.clone(), 1, "item".into(), 0, 16).await?,
            ciphertext
        );
        receiver_channel.close().await?;
        timeout(CONNECT_TIMEOUT, serving)
            .await
            .context("timed out stopping independent loopback receiver")???;
        receiver.close().await;
        sender.close().await;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires Docker and downloads the coturn fixture image"]
    async fn relay_only_coturn_transfers_a_framed_chunk() -> Result<()> {
        let _coturn = Coturn::start()?;
        tokio::time::sleep(Duration::from_millis(750)).await;
        let servers = [IceServer {
            urls: vec!["turn:127.0.0.1:34780?transport=udp".into()],
            username: Some("filebeam".into()),
            credential: Some("local-only".into()),
        }];
        let receiver =
            NativePeer::with_udp_addrs_and_policy(&servers, vec!["127.0.0.1:0".to_owned()], true)
                .await?;
        let sender =
            NativePeer::with_udp_addrs_and_policy(&servers, vec!["127.0.0.1:0".to_owned()], true)
                .await?;

        let (offer, receiver_channel) = receiver.offer_channel().await?;
        assert!(offer.sdp.contains(" typ relay"));
        assert!(!offer.sdp.contains(" typ host"));
        assert!(!offer.sdp.contains(" typ srflx"));
        let answer = sender.answer(&offer).await?;
        assert!(answer.sdp.contains(" typ relay"));
        assert!(!answer.sdp.contains(" typ host"));
        assert!(!answer.sdp.contains(" typ srflx"));
        receiver.accept_answer(&answer).await?;
        let sender_channel = sender.receiver_channel().await?;
        wait_for_open(&receiver_channel).await?;
        wait_for_open(&sender_channel).await?;

        let ciphertext = (0..(FRAME_PAYLOAD_BYTES + 37))
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        let serving = tokio::spawn(serve_channel(
            sender_channel,
            HashMap::from([(("item".to_owned(), 0), ciphertext.clone())]),
        ));
        assert_eq!(
            request_chunk(
                receiver_channel.clone(),
                1,
                "item".into(),
                0,
                ciphertext.len() as u64,
            )
            .await?,
            ciphertext
        );
        let report = receiver
            .connection
            .get_stats(Instant::now(), StatsSelector::None)
            .await;
        assert!(
            report.iter().any(|entry| matches!(entry,
                RTCStatsReportEntry::IceCandidatePair(pair)
                    if pair.nominated
                        && pair.bytes_sent > 0
                        && pair.bytes_received > 0
            )),
            "no nominated ICE candidate pair carried the relay-only transfer"
        );
        receiver_channel.close().await?;
        let _closed = timeout(CONNECT_TIMEOUT, serving)
            .await
            .context("timed out stopping relay-only WebRTC sender")??;
        receiver.close().await;
        sender.close().await;
        Ok(())
    }
}
