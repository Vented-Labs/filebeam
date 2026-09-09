# WebRTC Transfers

## Transport Policy

HTTP is enabled and selected by default. It stores encrypted payload chunks in the configured private filestore. An administrator can enable HTTP only, WebRTC only, or both, and selects the default driver in **Administration > Instance settings**. Environment values take precedence over the database setting and lock the corresponding admin control:

```dotenv
# FILEBEAM_ENABLED_TRANSFER_DRIVERS=["http","webrtc"]
# FILEBEAM_DEFAULT_TRANSFER_DRIVER=http
```

Changing the policy affects new reservations and links. It does not convert existing HTTP transfers: their encrypted payloads remain available through HTTP until expiry. Senders must create a new link to use a newly selected default driver; restart an interrupted HTTP transfer as a new link rather than expecting it to become peer-to-peer.

WebRTC supports public file links and notes. Inbox/username receiving remains HTTP-only. Signaling, session coordination, expiry, and encrypted transfer metadata still use Filebeam over HTTPS, including in a WebRTC-only deployment. WebRTC avoids payload storage, not database records: encrypted metadata persists until its normal expiry.

## Connectivity

When both `FILEBEAM_WEBRTC_ICE_SERVERS` and `FILEBEAM_WEBRTC_TURN_URLS` are unset, Filebeam uses the public Vented STUN server at `stun:stun.vented.com:3478`. Set `FILEBEAM_WEBRTC_ICE_SERVERS=[]` to explicitly disable that fallback, including for isolated local or browser test environments. A custom ICE list always takes precedence; configuring TURN URLs with no ICE list disables the STUN fallback so the TURN deployment is the sole connectivity configuration.

Configure infrastructure you trust when the default STUN service is not appropriate:

```dotenv
FILEBEAM_WEBRTC_ICE_SERVERS=[{"urls":["stun:stun.example.net:3478"]}]
FILEBEAM_WEBRTC_TURN_URLS=turn:turn.example.net:3478?transport=udp,turns:turn.example.net:5349?transport=tcp
FILEBEAM_WEBRTC_TURN_SECRET=replace-with-a-private-shared-secret
FILEBEAM_WEBRTC_TURN_TTL_SECONDS=3600
FILEBEAM_WEBRTC_SESSION_IDLE_SECONDS=120
FILEBEAM_WEBRTC_SESSION_LIMIT=8
FILEBEAM_WEBRTC_MAX_SDP_BYTES=65536
FILEBEAM_WEBRTC_LIVE_MAX_HOURS=24
```

`FILEBEAM_WEBRTC_ICE_SERVERS` must be a JSON array. Use only `stun:` URLs for public STUN entries. TURN URLs and the TURN shared secret are server configuration; never expose the secret to users, logs, or client configuration. TURN relays can carry encrypted payloads and have bandwidth, privacy, and cost implications. A WebRTC transfer may use TURN when a direct connection is not possible; selecting WebRTC does not guarantee a relay-free path.

For multi-instance deployments, WebRTC signaling polling needs a shared, lock-capable cache such as Redis. Keep all application nodes on the same cache configuration and prefix. The cache coordinates polling/session state; it is not payload storage.

## Limits And Operation

Plans have independent HTTP and WebRTC transfer-byte, file-count, and note-byte quotas. Each WebRTC quota can be set to **Unlimited** without changing the corresponding finite HTTP quota. Unlimited means Filebeam does not apply that plan quota; browser memory, WebRTC implementation limits, configured SDP/session bounds, peer bandwidth, relay capacity, and link expiry still apply. WebRTC traffic is not server-metered as stored HTTP payload bytes.

WebRTC creates a prehashed encrypted manifest before peer delivery. This reads each source file once before publishing the link, using bounded browser memory; preparation time depends on file size and device speed. Payload bytes are not staged in Filebeam storage. Both peers must stay connected through the session. The current protocol permits up to 100 items and 65,535 encrypted chunks per item, with encrypted chunks up to 25,000,000 bytes. These safety bounds also apply to Unlimited plans. Browsers without streamed file saving cannot download files above the 512 MiB in-memory compatibility limit; note display has the same memory limit.

## Consent And Failure

Selecting a method updates its limits immediately without opening a connection or a warning. Starting a WebRTC upload, download, or note decryption first asks for consent in a modal. Acceptance sets the host-only `webRTCRiskAccepted=1` cookie for one year (`SameSite=Lax`, `Secure` on HTTPS). It is a browser preference for guests and signed-in users, not an authorization token; clearing it makes the warning appear again.

There is no automatic HTTP fallback. The sender may explicitly restart as a new stored HTTP transfer, with a new link and key, only when HTTP is enabled and the content fits its limits. The original live share is then revoked.

Burn-on-read notes admit only one recipient session. Successful decryption revokes the share, but does not erase a recipient's copy. A failed or abandoned connection can require the sender to create a fresh note link: the server conservatively retains the claim because it cannot prove that a peer has not already received the content. Existing admitted sessions may finish when an administrator disables WebRTC, but new publications and joins are rejected. Explicit deletion or takedown revokes coordination for existing sessions as well.
