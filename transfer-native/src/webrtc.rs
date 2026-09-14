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
    time::Duration,
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
    },
};

pub const CHANNEL_LABEL: &str = "filebeam";
pub const CONTROL_LIMIT: usize = 1024;
pub const FRAME_PAYLOAD_BYTES: usize = 16 * 1024;
pub const SDP_TIMEOUT: Duration = Duration::from_secs(10);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
pub const SIGNALING_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct Signaling {
    client: Client,
    instance: String,
    transfer_id: String,
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
    let session = signaling.register(join_token).await?;
    let peer = NativePeer::new(&session.ice_servers, relay_only).await?;
    let (offer, channel) = peer.offer_channel().await?;
    signaling.offer(&session, &offer).await?;
    let started = tokio::time::Instant::now();
    loop {
        let (answer, status) = signaling.receiver_status(&session).await?;
        if let Some(answer) = answer {
            peer.accept_answer(&answer).await?;
            if let Err(error) = wait_for_open(&channel).await {
                peer.close().await;
                return Err(error);
            }
            return Ok((peer, session, channel));
        }
        if matches!(status.as_str(), "cancelled" | "completed" | "failed") {
            bail!("live sender ended the WebRTC session ({status})");
        }
        if started.elapsed() >= CONNECT_TIMEOUT {
            bail!("timed out waiting for WebRTC sender");
        }
        tokio::time::sleep(Duration::from_millis(1800)).await;
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
) -> Result<(NativePeer, Arc<dyn DataChannel>)> {
    let offer = session
        .offer
        .as_ref()
        .context("receiver has not published an offer")?;
    let peer = NativePeer::new(servers, relay_only).await?;
    let answer = peer.answer(offer).await?;
    signaling.answer(upload_token, &session.id, &answer).await?;
    let channel = peer.receiver_channel().await?;
    Ok((peer, channel))
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

impl NativePeer {
    pub async fn new(servers: &[IceServer], relay_only: bool) -> Result<Self> {
        Self::with_udp_addrs_and_policy(servers, vec!["0.0.0.0:0".to_owned()], relay_only).await
    }

    async fn with_udp_addrs(servers: &[IceServer], udp_addrs: Vec<String>) -> Result<Self> {
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
    if channel.label().await? != CHANNEL_LABEL {
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

async fn wait_for_open(channel: &Arc<dyn DataChannel>) -> Result<()> {
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
                .status();
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
        receiver_channel.close().await?;
        timeout(CONNECT_TIMEOUT, serving)
            .await
            .context("timed out stopping relay-only WebRTC sender")??;
        receiver.close().await;
        sender.close().await;
        Ok(())
    }
}
