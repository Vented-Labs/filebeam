use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result, bail};
use bytes::BytesMut;
use filebeam_transfer::{
    WebRtcControl, WebRtcFrame, encode_webrtc_control, encode_webrtc_frame, parse_webrtc_control,
};
use sha2::{Digest, Sha256};
use webrtc::data_channel::{DataChannel, DataChannelEvent};

use crate::webrtc::{CONTROL_LIMIT, FRAME_PAYLOAD_BYTES};

#[derive(Clone, Debug)]
pub(super) struct CiphertextArtifact {
    pub path: PathBuf,
    pub bytes: u64,
    pub checksum: String,
}

/// Streams an immutable checkpointed record only after verifying its durable
/// identity. This avoids keeping all live-transfer ciphertext in memory.
pub(super) async fn serve_artifacts(
    channel: Arc<dyn DataChannel>,
    chunks: HashMap<(String, u64), CiphertextArtifact>,
) -> Result<()> {
    let mut pending: Option<u32> = None;
    while let Some(event) = channel.poll().await {
        match event {
            DataChannelEvent::OnMessage(message) if message.is_string => {
                let text = std::str::from_utf8(&message.data)
                    .context("invalid WebRTC control UTF-8")?;
                if text.len() > CONTROL_LIMIT {
                    bail!("WebRTC control frame is too large");
                }
                match parse_webrtc_control(text).map_err(anyhow::Error::msg)? {
                    WebRtcControl::Request { seq, item_id, index } if pending.is_none() => {
                        let artifact = chunks
                            .get(&(item_id.clone(), index))
                            .context("unknown WebRTC chunk request")?;
                        let ciphertext = tokio::fs::read(&artifact.path)
                            .await
                            .with_context(|| format!("read immutable ciphertext {}", artifact.path.display()))?;
                        if ciphertext.len() as u64 != artifact.bytes
                            || hex::encode(Sha256::digest(&ciphertext)) != artifact.checksum
                        {
                            bail!("immutable ciphertext was changed or is corrupt");
                        }
                        let response = WebRtcControl::chunk(seq, item_id, index, artifact.bytes)
                            .map_err(anyhow::Error::msg)?;
                        channel
                            .send_text(&encode_webrtc_control(&response).map_err(anyhow::Error::msg)?)
                            .await?;
                        for (offset, payload) in ciphertext.chunks(FRAME_PAYLOAD_BYTES).enumerate() {
                            let frame = WebRtcFrame::new(
                                seq,
                                u32::try_from(offset * FRAME_PAYLOAD_BYTES)?,
                                payload.to_vec(),
                            )
                            .map_err(anyhow::Error::msg)?;
                            channel
                                .send(BytesMut::from(
                                    encode_webrtc_frame(&frame).map_err(anyhow::Error::msg)?.as_slice(),
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
    Ok(())
}
