use std::{future::Future, time::Duration};

use anyhow::{Context, Result, bail};
use filebeam_transfer::MAX_CIPHERTEXT_BYTES;
use reqwest::{
    Client,
    header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, ETAG, IF_RANGE, RANGE},
};
use tokio::time;
use tokio_util::sync::CancellationToken;

/// Build the shared pool client. Request deadlines belong to individual
/// operations: a global timeout cannot express a long transfer with a short
/// body-idle deadline.
pub fn client(connect_timeout: Duration) -> Result<Client> {
    Client::builder()
        .connect_timeout(connect_timeout)
        .user_agent(concat!("beam/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("create async transfer HTTP client")
}

/// Response limits for small control-plane bodies. The header deadline and
/// body-idle deadline are intentionally separate from transfer duration.
#[derive(Clone, Copy, Debug)]
pub struct ResponseLimits {
    pub headers_timeout: Duration,
    pub body_idle_timeout: Duration,
    pub maximum_bytes: usize,
}

/// Wait for response headers while allowing the caller's cancellation future
/// to interrupt the socket operation.
pub async fn request_headers<F>(
    request: reqwest::RequestBuilder,
    headers_timeout: Duration,
    cancelled: F,
) -> Result<reqwest::Response>
where
    F: Future<Output = ()>,
{
    tokio::pin!(cancelled);
    tokio::select! {
        _ = &mut cancelled => bail!("transfer cancelled"),
        response = time::timeout(headers_timeout, request.send()) => {
            Ok(response.context("HTTP response headers timed out")??)
        }
    }
}

/// Read a bounded control-plane body. Missing Content-Length is allowed, but
/// the observed bytes are still bounded and every body wait is cancellable.
pub async fn read_limited<F>(
    mut response: reqwest::Response,
    limits: ResponseLimits,
    cancelled: F,
) -> Result<Vec<u8>>
where
    F: Future<Output = ()>,
{
    if response
        .content_length()
        .is_some_and(|length| length > limits.maximum_bytes as u64)
    {
        bail!("HTTP response body exceeds its limit");
    }
    tokio::pin!(cancelled);
    let mut body = Vec::new();
    while let Some(chunk) = tokio::select! {
        _ = &mut cancelled => bail!("transfer cancelled"),
        chunk = time::timeout(limits.body_idle_timeout, response.chunk()) => {
            chunk.context("HTTP response body became idle")??
        }
    } {
        if body.len().saturating_add(chunk.len()) > limits.maximum_bytes {
            bail!("HTTP response body exceeds its limit");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Limits and identity checks for one continuation range request.
pub struct RangeRequest<'a> {
    pub url: &'a str,
    pub start: u64,
    pub end: u64,
    pub ciphertext_total: u64,
    pub expected_etag: &'a str,
    pub headers_timeout: Duration,
    pub idle_timeout: Duration,
}

/// Fetch exactly one continuation range. `ciphertext_total` is required and is
/// bounded to one Filebeam ciphertext chunk; callers must not derive an
/// allocation size from an untrusted range response. `headers_timeout` applies
/// before the body begins and `idle_timeout` applies between every body chunk.
pub async fn get_range(
    client: &Client,
    request: &RangeRequest<'_>,
    cancelled: &CancellationToken,
) -> Result<Vec<u8>> {
    if request.ciphertext_total == 0
        || request.ciphertext_total > MAX_CIPHERTEXT_BYTES
        || request.end < request.start
        || request.end >= request.ciphertext_total
    {
        bail!("invalid byte range");
    }
    let expected = request
        .end
        .checked_sub(request.start)
        .and_then(|n| n.checked_add(1))
        .context("range is too large")?;
    let response = tokio::select! {
        _ = cancelled.cancelled() => bail!("transfer cancelled"),
        response = time::timeout(request.headers_timeout, client
            .get(request.url)
            .header(RANGE, format!("bytes={}-{}", request.start, request.end))
            // If the ciphertext changed, the server deliberately returns 200;
            // rejecting that response prevents mixing two authenticated bodies.
            .header(IF_RANGE, request.expected_etag)
            .send()) => response.context("range response headers timed out")??,
    };
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        bail!("range request returned {}", response.status());
    }
    let headers = response.headers();
    let etag = header(headers, ETAG)?;
    if etag != request.expected_etag {
        bail!("range ETag changed");
    }
    if !header(headers, ACCEPT_RANGES)?.eq_ignore_ascii_case("bytes") {
        bail!("server did not confirm byte ranges");
    }
    let range = header(headers, CONTENT_RANGE)?;
    validate_content_range(range, request.start, request.end, request.ciphertext_total)?;
    let length: u64 = header(headers, CONTENT_LENGTH)?
        .parse()
        .context("invalid range Content-Length")?;
    if length != expected {
        bail!("range Content-Length mismatch");
    }

    let mut response = response;
    let mut body = Vec::with_capacity(expected.try_into().unwrap_or(0));
    while let Some(chunk) = tokio::select! {
        _ = cancelled.cancelled() => bail!("transfer cancelled"),
        chunk = time::timeout(request.idle_timeout, response.chunk()) => chunk.context("range body became idle")??,
    } {
        body.extend_from_slice(&chunk);
        if body.len() as u64 > expected {
            bail!("range response exceeds Content-Length");
        }
    }
    if body.len() as u64 != expected {
        bail!("range response is truncated");
    }
    Ok(body)
}

fn header(headers: &reqwest::header::HeaderMap, name: reqwest::header::HeaderName) -> Result<&str> {
    headers
        .get(name)
        .context("range response is missing a required header")?
        .to_str()
        .context("range response contains an invalid header")
}

fn validate_content_range(value: &str, start: u64, end: u64, total_expected: u64) -> Result<()> {
    let value = value
        .strip_prefix("bytes ")
        .context("invalid Content-Range unit")?;
    let (range, total) = value.split_once('/').context("invalid Content-Range")?;
    let (actual_start, actual_end) = range.split_once('-').context("invalid Content-Range")?;
    let actual_start: u64 = actual_start
        .parse()
        .context("invalid Content-Range start")?;
    let actual_end: u64 = actual_end.parse().context("invalid Content-Range end")?;
    let total: u64 = total.parse().context("invalid Content-Range total")?;
    if actual_start != start || actual_end != end || total != total_expected {
        bail!("Content-Range does not match request");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn server(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = socket.read(&mut request);
            socket.write_all(response.as_bytes()).unwrap();
        });
        format!("http://{address}/chunk")
    }

    #[test]
    fn content_range_must_exactly_match_the_requested_slice() {
        assert!(validate_content_range("bytes 10-19/20", 10, 19, 20).is_ok());
        assert!(validate_content_range("bytes 10-20/21", 10, 19, 20).is_err());
        assert!(validate_content_range("bytes 10-19/*", 10, 19, 20).is_err());
        assert!(validate_content_range("items 10-19/20", 10, 19, 20).is_err());
    }

    #[tokio::test]
    async fn rejects_wrong_total_and_truncated_range() {
        let client = client(Duration::from_secs(1)).unwrap();
        let cancel = CancellationToken::new();
        let wrong_total = server(
            "HTTP/1.1 206 Partial Content\r\nETag: \"sum\"\r\nAccept-Ranges: bytes\r\nContent-Range: bytes 0-2/4\r\nContent-Length: 3\r\n\r\nabc",
        );
        assert!(
            get_range(
                &client,
                &RangeRequest {
                    url: &wrong_total,
                    start: 0,
                    end: 2,
                    ciphertext_total: 3,
                    expected_etag: "\"sum\"",
                    headers_timeout: Duration::from_secs(1),
                    idle_timeout: Duration::from_secs(1),
                },
                &cancel
            )
            .await
            .is_err()
        );
        let truncated = server(
            "HTTP/1.1 206 Partial Content\r\nETag: \"sum\"\r\nAccept-Ranges: bytes\r\nContent-Range: bytes 0-2/3\r\nContent-Length: 3\r\n\r\nab",
        );
        assert!(
            get_range(
                &client,
                &RangeRequest {
                    url: &truncated,
                    start: 0,
                    end: 2,
                    ciphertext_total: 3,
                    expected_etag: "\"sum\"",
                    headers_timeout: Duration::from_secs(1),
                    idle_timeout: Duration::from_secs(1),
                },
                &cancel
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn cancellation_aborts_a_stalled_header_request() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let _ = listener.accept();
            thread::sleep(Duration::from_secs(2));
        });
        let client = client(Duration::from_secs(1)).unwrap();
        let cancel = CancellationToken::new();
        let url = format!("http://{address}/chunk");
        let task = tokio::spawn({
            let client = client.clone();
            let cancel = cancel.clone();
            async move {
                get_range(
                    &client,
                    &RangeRequest {
                        url: &url,
                        start: 0,
                        end: 0,
                        ciphertext_total: 1,
                        expected_etag: "\"sum\"",
                        headers_timeout: Duration::from_secs(30),
                        idle_timeout: Duration::from_secs(1),
                    },
                    &cancel,
                )
                .await
            }
        });
        time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
        assert!(task.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn response_limits_bound_unknown_length_and_idle_bodies() {
        let client = client(Duration::from_secs(1)).unwrap();
        let cancel = CancellationToken::new();
        let excess = server("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nabc");
        let response = request_headers(
            client.get(&excess),
            Duration::from_secs(1),
            cancel.cancelled(),
        )
        .await
        .unwrap();
        assert!(
            read_limited(
                response,
                ResponseLimits {
                    headers_timeout: Duration::from_secs(1),
                    body_idle_timeout: Duration::from_secs(1),
                    maximum_bytes: 2,
                },
                cancel.cancelled(),
            )
            .await
            .is_err()
        );

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = socket.read(&mut request);
            socket.write_all(b"HTTP/1.1 200 OK\r\n\r\na").unwrap();
            thread::sleep(Duration::from_millis(100));
        });
        let response = request_headers(
            client.get(format!("http://{address}/metadata")),
            Duration::from_millis(50),
            cancel.cancelled(),
        )
        .await
        .unwrap();
        assert!(
            read_limited(
                response,
                ResponseLimits {
                    headers_timeout: Duration::from_millis(50),
                    body_idle_timeout: Duration::from_millis(20),
                    maximum_bytes: 2,
                },
                cancel.cancelled(),
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn cancellation_aborts_a_stalled_control_body_within_one_second() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = socket.read(&mut request);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{")
                .unwrap();
            thread::sleep(Duration::from_secs(2));
        });
        let client = client(Duration::from_secs(1)).unwrap();
        let cancel = CancellationToken::new();
        let response = request_headers(
            client.get(format!("http://{address}/metadata")),
            Duration::from_secs(1),
            cancel.cancelled(),
        )
        .await
        .unwrap();
        let started = std::time::Instant::now();
        let task = tokio::spawn({
            let cancel = cancel.clone();
            async move {
                read_limited(
                    response,
                    ResponseLimits {
                        headers_timeout: Duration::from_secs(1),
                        body_idle_timeout: Duration::from_secs(30),
                        maximum_bytes: 1024,
                    },
                    cancel.cancelled(),
                )
                .await
            }
        });
        time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
        assert!(task.await.unwrap().is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
