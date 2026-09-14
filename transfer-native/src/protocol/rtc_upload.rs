use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result, bail};
use bytes::BytesMut;
use filebeam_transfer::{
    WebRtcControl, WebRtcFrame, encode_webrtc_control, encode_webrtc_frame, parse_webrtc_control,
};
use sha2::{Digest, Sha256};
use webrtc::data_channel::{DataChannel, DataChannelEvent};

use crate::webrtc::{CONTROL_LIMIT, FRAME_PAYLOAD_BYTES, REQUEST_TIMEOUT};

#[derive(Clone, Debug)]
pub(super) struct CiphertextArtifact {
    pub path: PathBuf,
    pub bytes: u64,
    pub checksum: String,
}

/// Streams an immutable checkpointed record only after verifying its durable
/// identity. Verification and framing use fixed-size buffers, so eight peers
/// do not multiply a full ciphertext chunk into memory.
pub(super) async fn serve_artifacts(
    channel: Arc<dyn DataChannel>,
    chunks: HashMap<(String, u64), CiphertextArtifact>,
) -> Result<()> {
    let mut pending: Option<u32> = None;
    loop {
        let event = if pending.is_some() {
            tokio::time::timeout(REQUEST_TIMEOUT, channel.poll())
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
                        let artifact = chunks
                            .get(&(item_id.clone(), index))
                            .context("unknown WebRTC chunk request")?;
                        let mut ciphertext = verify_artifact(artifact).await?;
                        let response = WebRtcControl::chunk(seq, item_id, index, artifact.bytes)
                            .map_err(anyhow::Error::msg)?;
                        channel
                            .send_text(
                                &encode_webrtc_control(&response).map_err(anyhow::Error::msg)?,
                            )
                            .await?;
                        let mut offset = 0u64;
                        let mut payload = vec![0; FRAME_PAYLOAD_BYTES];
                        while offset < artifact.bytes {
                            let length = usize::try_from(
                                (artifact.bytes - offset).min(FRAME_PAYLOAD_BYTES as u64),
                            )?;
                            tokio::io::AsyncReadExt::read_exact(
                                &mut ciphertext,
                                &mut payload[..length],
                            )
                            .await
                            .context("immutable ciphertext was truncated")?;
                            let frame = WebRtcFrame::new(
                                seq,
                                u32::try_from(offset)?,
                                payload[..length].to_vec(),
                            )
                            .map_err(anyhow::Error::msg)?;
                            channel
                                .send(BytesMut::from(
                                    encode_webrtc_frame(&frame)
                                        .map_err(anyhow::Error::msg)?
                                        .as_slice(),
                                ))
                                .await?;
                            offset = offset
                                .checked_add(length as u64)
                                .context("WebRTC frame offset overflow")?;
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

async fn verify_artifact(artifact: &CiphertextArtifact) -> Result<tokio::fs::File> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let mut file = tokio::fs::File::open(&artifact.path)
        .await
        .with_context(|| format!("read immutable ciphertext {}", artifact.path.display()))?;
    if file.metadata().await?.len() != artifact.bytes {
        bail!("immutable ciphertext was changed or is corrupt");
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    if hex::encode(hash.finalize()) != artifact.checksum {
        bail!("immutable ciphertext was changed or is corrupt");
    }
    file.seek(std::io::SeekFrom::Start(0)).await?;
    Ok(file)
}
