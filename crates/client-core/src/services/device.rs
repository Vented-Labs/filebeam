//! Registered-device account and credential control-plane operations.
//!
//! Enrollment and approval transcripts are deliberately constructed here so a
//! caller cannot turn a device key into an arbitrary signing oracle.

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use reqwest::{Url, blocking::Client};
use serde::{Deserialize, Serialize};
use std::io::Read;

use super::{account::Api, client::url};

const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_CAPABILITIES: usize = 16;

#[derive(Clone, Debug, Deserialize)]
pub struct AccountDevice {
    pub id: String,
    pub display_name: String,
    pub platform: String,
    pub public_key: String,
    pub key_version: u32,
    pub status: String,
    pub generation: u64,
    pub last_seen_at: Option<String>,
    pub presence_expires_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DeviceRegistrySnapshot {
    pub revision: u64,
    pub generation: u64,
    #[serde(default)]
    pub offline_freshness_seconds: Option<u64>,
    pub devices: Vec<AccountDevice>,
}

#[derive(Clone, Debug)]
pub struct DeviceEnrollmentChallenge {
    pub device: AccountDevice,
    pub challenge: String,
    pub expires_at: Option<String>,
    pub approval_context: DeviceApprovalContext,
}

#[derive(Clone, Debug)]
pub struct DeviceEnrollmentResult {
    pub device: AccountDevice,
    pub status: String,
    pub credential: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DeviceApprovalEvidence {
    pub version: u8,
    pub approver_device_id: String,
    pub approver_key_version: u32,
    pub approver_public_key: String,
    pub approval_signature: String,
}

#[derive(Clone, Debug)]
pub struct DeviceCredentialRedemption {
    pub credential: String,
    pub approval_evidence: Option<DeviceApprovalEvidence>,
}

/// Candidate-owned, one-use ceremony data scanned or pasted by an approved
/// device. Rust reconstructs the bounded backend transcript from this record.
#[derive(Clone, Debug)]
pub struct DeviceApprovalContext {
    pub instance_origin: String,
    pub deployment_id: String,
    pub account_id: u64,
    pub generation: u64,
    pub candidate_device_id: String,
    pub candidate_key_version: u32,
    pub candidate_public_key: String,
    pub challenge_sha256: String,
    pub expires_at_unix: u64,
    pub approver_device_id: String,
    pub approver_key_version: u32,
}

#[derive(Deserialize)]
struct ChallengeResponse {
    device: AccountDevice,
    challenge: String,
    expires_at: Option<String>,
    approval_context: ApprovalContextResponse,
}

#[derive(Deserialize)]
struct ApprovalContextResponse {
    version: u8,
    instance_origin: String,
    deployment_id: String,
    account_id: u64,
    generation: u64,
    candidate_device_id: String,
    candidate_key_version: u32,
    expires_at: String,
    challenge_sha256: String,
}

#[derive(Deserialize)]
struct EnrollmentResponse {
    device: AccountDevice,
    status: String,
    credential: Option<String>,
}

#[derive(Deserialize)]
struct DeviceResponse {
    device: AccountDevice,
}

#[derive(Deserialize)]
struct ResetResponse {
    revision: u64,
    generation: u64,
}

#[derive(Serialize)]
struct ChallengeRequest<'a> {
    device_id: &'a str,
    display_name: &'a str,
    platform: &'a str,
    public_key: &'a str,
    key_version: u32,
    capabilities: &'a [String],
}

#[derive(Serialize)]
struct ProofRequest<'a> {
    challenge: &'a str,
    signature: &'a str,
}

#[derive(Clone)]
pub struct DeviceService {
    instance: Url,
    http: Client,
}

impl DeviceService {
    pub(crate) fn new(instance: Url, http: Client) -> Self {
        Self { instance, http }
    }

    pub fn account_devices(&self) -> Result<DeviceRegistrySnapshot> {
        self.account_get("api/native/v1/devices", true)
    }

    pub fn account_device_challenge(
        &self,
        device_id: &str,
        display_name: &str,
        platform: &str,
        public_key: &str,
        key_version: u32,
        capabilities: &[String],
    ) -> Result<DeviceEnrollmentChallenge> {
        validate_registration(
            device_id,
            display_name,
            platform,
            public_key,
            key_version,
            capabilities,
        )?;
        let response: ChallengeResponse = self.account_post(
            "api/native/v1/devices/enrollment-challenge",
            &ChallengeRequest {
                device_id,
                display_name,
                platform,
                public_key,
                key_version,
                capabilities,
            },
            201,
            true,
        )?;
        validate_challenge(&response.challenge)?;
        let context = response.approval_context;
        if context.version != 2
            || context.candidate_device_id != response.device.id
            || context.candidate_key_version != response.device.key_version
            || context.generation != response.device.generation
        {
            bail!("invalid enrollment approval context")
        }
        Ok(DeviceEnrollmentChallenge {
            device: response.device,
            challenge: response.challenge,
            expires_at: response.expires_at,
            approval_context: DeviceApprovalContext {
                instance_origin: context.instance_origin,
                deployment_id: context.deployment_id,
                account_id: context.account_id,
                generation: context.generation,
                candidate_device_id: context.candidate_device_id,
                candidate_key_version: context.candidate_key_version,
                candidate_public_key: String::new(),
                challenge_sha256: context.challenge_sha256,
                expires_at_unix: parse_rfc3339_unix(&context.expires_at)?,
                approver_device_id: String::new(),
                approver_key_version: 0,
            },
        })
    }

    pub fn account_device_enroll(
        &self,
        challenge: &DeviceEnrollmentChallenge,
        signature: &[u8; 64],
    ) -> Result<DeviceEnrollmentResult> {
        let signature = STANDARD.encode(signature);
        let response: EnrollmentResponse = self.account_post(
            &format!("api/native/v1/devices/enrollments/{}", challenge.device.id),
            &ProofRequest {
                challenge: &challenge.challenge,
                signature: &signature,
            },
            201,
            false,
        )?;
        Ok(DeviceEnrollmentResult {
            device: response.device,
            status: response.status,
            credential: response.credential,
        })
    }

    pub fn account_device_complete_enrollment(
        &self,
        challenge: &DeviceEnrollmentChallenge,
        signature: &[u8; 64],
    ) -> Result<DeviceCredentialRedemption> {
        let signature = STANDARD.encode(signature);
        #[derive(Deserialize)]
        struct CredentialResponse {
            credential: String,
            approval_evidence: Option<DeviceApprovalEvidence>,
        }
        let response: CredentialResponse = self.post(
            &format!(
                "api/native/v1/devices/enrollments/{}/credential",
                challenge.device.id
            ),
            &ProofRequest {
                challenge: &challenge.challenge,
                signature: &signature,
            },
            201,
            None,
            false,
        )?;
        validate_credential(&response.credential)?;
        Ok(DeviceCredentialRedemption {
            credential: response.credential,
            approval_evidence: response.approval_evidence,
        })
    }

    pub fn approve_device_enrollment(
        &self,
        device: &AccountDevice,
        credential: &str,
        signature: &[u8; 64],
    ) -> Result<AccountDevice> {
        validate_id(&device.id)?;
        let signature = STANDARD.encode(signature);
        let response: DeviceResponse = self.post(
            &format!("api/native/v1/devices/enrollments/{}/approve", device.id),
            &serde_json::json!({"approval_signature": signature}),
            201,
            Some(credential),
            false,
        )?;
        Ok(response.device)
    }

    pub fn account_device_rename(
        &self,
        device_id: &str,
        display_name: &str,
    ) -> Result<AccountDevice> {
        validate_id(device_id)?;
        validate_name(display_name)?;
        let response: DeviceResponse = self.account_request(
            reqwest::Method::PATCH,
            &format!("api/native/v1/devices/{device_id}"),
            serde_json::json!({"display_name": display_name}),
            200,
            false,
        )?;
        Ok(response.device)
    }

    pub fn account_device_revoke(
        &self,
        device_id: &str,
        current_password: &str,
    ) -> Result<(AccountDevice, u64)> {
        validate_id(device_id)?;
        if current_password.is_empty() {
            bail!("current password is required")
        }
        #[derive(Deserialize)]
        struct RevokeResponse {
            device: AccountDevice,
            revision: u64,
        }
        let response: RevokeResponse = self.account_request(
            reqwest::Method::DELETE,
            &format!("api/native/v1/devices/{device_id}"),
            serde_json::json!({"current_password": current_password}),
            200,
            false,
        )?;
        Ok((response.device, response.revision))
    }

    pub fn account_devices_reset(&self, current_password: &str) -> Result<(u64, u64)> {
        if current_password.is_empty() {
            bail!("current password is required")
        }
        let response: ResetResponse = self.account_request(
            reqwest::Method::DELETE,
            "api/native/v1/devices",
            serde_json::json!({"current_password": current_password, "confirmation": "RESET"}),
            200,
            false,
        )?;
        Ok((response.revision, response.generation))
    }

    pub fn device_sync(&self, credential: &str) -> Result<DeviceRegistrySnapshot> {
        validate_credential(credential)?;
        self.get("api/native/v1/devices/sync", Some(credential), false)
    }

    /// Presence is an authenticated, short-lived assertion made only by a
    /// running receiver loop. It is not a substitute for registry pinning.
    pub fn device_presence(
        &self,
        credential: &str,
        ready: bool,
        capabilities: &[String],
    ) -> Result<AccountDevice> {
        validate_credential(credential)?;
        if capabilities.len() > MAX_CAPABILITIES
            || capabilities
                .iter()
                .any(|item| item.is_empty() || item.len() > 64)
        {
            bail!("invalid device capabilities")
        }
        let response: DeviceResponse = self.post(
            "api/native/v1/devices/presence",
            &serde_json::json!({"ready": ready, "capabilities": capabilities}),
            200,
            Some(credential),
            false,
        )?;
        Ok(response.device)
    }

    fn account_get<T: for<'a> Deserialize<'a>>(
        &self,
        path: &str,
        unsupported_on_404: bool,
    ) -> Result<T> {
        self.get(path, None, unsupported_on_404)
    }
    fn account_post<T: for<'a> Deserialize<'a>, B: Serialize>(
        &self,
        path: &str,
        body: &B,
        expected: u16,
        unsupported_on_404: bool,
    ) -> Result<T> {
        self.post(path, body, expected, None, unsupported_on_404)
    }
    fn account_request<T: for<'a> Deserialize<'a>>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: serde_json::Value,
        expected: u16,
        unsupported_on_404: bool,
    ) -> Result<T> {
        let response = self
            .http
            .request(method, url(&self.instance, path)?)
            .header("Sec-Fetch-Site", "same-origin")
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&body)
            .send()
            .context("native device account request")?;
        self.decode(response, expected, unsupported_on_404)
    }
    fn get<T: for<'a> Deserialize<'a>>(
        &self,
        path: &str,
        credential: Option<&str>,
        unsupported_on_404: bool,
    ) -> Result<T> {
        let mut request = self
            .http
            .get(url(&self.instance, path)?)
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(credential) = credential {
            request = request.bearer_auth(credential);
        }
        self.decode(
            request.send().context("native device request")?,
            200,
            unsupported_on_404,
        )
    }
    fn post<T: for<'a> Deserialize<'a>, B: Serialize>(
        &self,
        path: &str,
        body: &B,
        expected: u16,
        credential: Option<&str>,
        unsupported_on_404: bool,
    ) -> Result<T> {
        let mut request = self
            .http
            .post(url(&self.instance, path)?)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(body);
        if let Some(credential) = credential {
            request = request.bearer_auth(credential);
        } else {
            request = request.header("Sec-Fetch-Site", "same-origin");
        }
        self.decode(
            request.send().context("native device request")?,
            expected,
            unsupported_on_404,
        )
    }
    fn decode<T: for<'a> Deserialize<'a>>(
        &self,
        response: reqwest::blocking::Response,
        expected: u16,
        unsupported_on_404: bool,
    ) -> Result<T> {
        if response.status().as_u16() == 404 && unsupported_on_404 {
            bail!("INSTANCE_UNSUPPORTED")
        }
        if response.status().as_u16() != expected {
            bail!("native device request returned {}", response.status())
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("read native device response")?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            bail!("native device response exceeds the control-plane limit")
        }
        serde_json::from_slice::<Api<T>>(&bytes)
            .context("decode native device response")
            .map(|value| value.data)
    }
}

pub fn enrollment_transcript(device_id: &str, generation: u64, challenge: &str) -> Result<Vec<u8>> {
    validate_id(device_id)?;
    let challenge = decode_challenge(challenge)?;
    Ok([
        b"filebeam-device-enrollment-v1\0".as_slice(),
        device_id.as_bytes(),
        b"\0",
        generation.to_string().as_bytes(),
        b"\0",
        challenge.as_slice(),
    ]
    .concat())
}
pub fn enrollment_signature(
    key: &SigningKey,
    device_id: &str,
    generation: u64,
    challenge: &str,
) -> Result<[u8; 64]> {
    Ok(key
        .sign(&enrollment_transcript(device_id, generation, challenge)?)
        .to_bytes())
}
pub fn approval_transcript(context: &DeviceApprovalContext) -> Result<Vec<u8>> {
    validate_id(&context.candidate_device_id)?;
    validate_id(&context.approver_device_id)?;
    if context.instance_origin.is_empty()
        || context.deployment_id.is_empty()
        || context.account_id == 0
        || context.candidate_key_version == 0
        || context.approver_key_version == 0
    {
        bail!("invalid device approval context")
    }
    let key = STANDARD
        .decode(&context.candidate_public_key)
        .context("device public key is not base64")?;
    if key.len() != 32 {
        bail!("device public key must be 32 bytes")
    }
    let digest = decode_challenge(&context.challenge_sha256)?;
    let mut bytes = Vec::with_capacity(512);
    let account_id = context.account_id.to_string();
    let generation = context.generation.to_string();
    let candidate_key_version = context.candidate_key_version.to_string();
    let approver_key_version = context.approver_key_version.to_string();
    let expiry = context.expires_at_unix.to_string();
    for value in [
        "filebeam-device-approval-v2",
        &context.instance_origin,
        &context.deployment_id,
        &account_id,
        &generation,
        &context.candidate_device_id,
        &candidate_key_version,
        &context.approver_device_id,
        &approver_key_version,
        &expiry,
    ] {
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(0);
    }
    bytes.extend_from_slice(&digest);
    bytes.extend_from_slice(&key);
    Ok(bytes)
}
pub fn approval_signature(key: &SigningKey, context: &DeviceApprovalContext) -> Result<[u8; 64]> {
    Ok(key.sign(&approval_transcript(context)?).to_bytes())
}

fn parse_rfc3339_unix(value: &str) -> Result<u64> {
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};

    OffsetDateTime::parse(value, &Rfc3339)
        .context("approval expiry is not RFC3339")
        .and_then(|value| {
            u64::try_from(value.unix_timestamp())
                .context("approval expiry is before the Unix epoch")
        })
}

fn validate_registration(
    id: &str,
    name: &str,
    platform: &str,
    public_key: &str,
    version: u32,
    capabilities: &[String],
) -> Result<()> {
    validate_id(id)?;
    validate_name(name)?;
    if platform.is_empty()
        || platform.len() > 32
        || version == 0
        || capabilities.len() > MAX_CAPABILITIES
        || capabilities
            .iter()
            .any(|item| item.is_empty() || item.len() > 64)
    {
        bail!("invalid device registration")
    }
    let key = STANDARD
        .decode(public_key)
        .context("device public key is not base64")?;
    if key.len() != 32 {
        bail!("device public key must be 32 bytes")
    }
    Ok(())
}
fn validate_id(value: &str) -> Result<()> {
    if value.len() != 26
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        bail!("device id must be a ULID")
    }
    Ok(())
}
fn validate_name(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 80 {
        bail!("device display name must be 1-80 bytes")
    }
    Ok(())
}
fn decode_challenge(value: &str) -> Result<Vec<u8>> {
    let value = STANDARD
        .decode(value)
        .context("device challenge is not base64")?;
    if value.len() != 32 {
        bail!("device challenge must be 32 bytes")
    }
    Ok(value)
}
fn validate_challenge(value: &str) -> Result<()> {
    decode_challenge(value).map(|_| ())
}
fn validate_credential(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        bail!("device credential is invalid")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signature, Verifier};
    use sha2::{Digest, Sha256};

    const DEVICE: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    #[test]
    fn enrollment_and_approval_sign_only_the_backend_transcripts() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let challenge = STANDARD.encode([3; 32]);
        let enrollment = enrollment_signature(&key, DEVICE, 4, &challenge).unwrap();
        key.verifying_key()
            .verify(
                &enrollment_transcript(DEVICE, 4, &challenge).unwrap(),
                &Signature::from_bytes(&enrollment),
            )
            .unwrap();
        let public = STANDARD.encode([9; 32]);
        let approval_context = DeviceApprovalContext {
            instance_origin: "https://filebeam.test".into(),
            deployment_id: "https://filebeam.test".into(),
            account_id: 42,
            generation: 4,
            candidate_device_id: DEVICE.into(),
            candidate_key_version: 1,
            candidate_public_key: public,
            challenge_sha256: STANDARD.encode(Sha256::digest(STANDARD.decode(challenge).unwrap())),
            expires_at_unix: 1_700_000_000,
            approver_device_id: "01ARZ3NDEKTSV4RRFFQ69G5FAW".into(),
            approver_key_version: 1,
        };
        let approval = approval_signature(&key, &approval_context).unwrap();
        key.verifying_key()
            .verify(
                &approval_transcript(&approval_context).unwrap(),
                &Signature::from_bytes(&approval),
            )
            .unwrap();
    }
}
