use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use filebeam_client_core::{
    device_registry::{
        AccountId, DeploymentId, DeviceGeneration, DeviceId, DeviceRegistry, Origin, RegistryScope,
    },
    services::ServiceClient,
};
use filebeam_daemon::{
    ApprovalMode, Daemon, DeviceJobState, DeviceProvision, IPC_VERSION, Request,
};
use filebeam_transfer_native::{checkpoint::Store, device::load_device_receipt};
use reqwest::{
    Certificate, Url,
    blocking::Client,
    cookie::{CookieStore, Jar},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const SENDER_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const RECEIVER_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAW";

struct Identity {
    id: &'static str,
    key: SigningKey,
    credential: String,
}

fn main() -> Result<()> {
    let mut args = env::args_os();
    let _ = args.next();
    if args.next().as_deref() == Some(std::ffi::OsStr::new("daemon")) {
        let home = PathBuf::from(args.next().context("missing daemon home")?);
        let certificate = fs::read(args.next().context("missing CA certificate")?)?;
        return Daemon::open_with_trusted_certificate(&home, Certificate::from_pem(&certificate)?)?
            .run();
    }

    run()
}

fn run() -> Result<()> {
    let instance =
        env::var("FILEBEAM_DEVICE_INSTANCE").context("FILEBEAM_DEVICE_INSTANCE is required")?;
    let root = PathBuf::from(
        env::var("FILEBEAM_DEVICE_ROOT").context("FILEBEAM_DEVICE_ROOT is required")?,
    );
    let certificate_path =
        PathBuf::from(env::var("FILEBEAM_DEVICE_CA").context("FILEBEAM_DEVICE_CA is required")?);
    let certificate = Certificate::from_pem(&fs::read(&certificate_path)?)?;
    let jar = std::sync::Arc::new(Jar::default());
    let web = Client::builder()
        .add_root_certificate(certificate.clone())
        .cookie_provider(jar.clone())
        .build()?;
    login(&web, &jar, &instance)?;

    let mut sender = Identity {
        id: SENDER_ID,
        key: SigningKey::from_bytes(&[11; 32]),
        credential: String::new(),
    };
    let mut receiver = Identity {
        id: RECEIVER_ID,
        key: SigningKey::from_bytes(&[22; 32]),
        credential: String::new(),
    };
    sender.credential = enroll_first(&web, &jar, &instance, &sender)?;
    receiver.credential = enroll_and_approve(&web, &jar, &instance, &sender, &receiver)?;

    // Exercise the maintained Rust DTO decoder against Laravel before daemon provisioning.
    let native = ServiceClient::new_with_trusted_certificate(&instance, certificate.clone())?;
    let registry = native.devices().device_sync(&sender.credential)?;
    ensure!(
        registry.devices.len() == 2,
        "Laravel registry did not contain both approved devices"
    );

    let sender_home = root.join("sender");
    let receiver_home = root.join("receiver");
    let source = root.join("source");
    let destination = root.join("destination");
    for directory in [&sender_home, &receiver_home, &source, &destination] {
        fs::create_dir_all(directory)?;
    }
    let mut sender_daemon = start_daemon(&sender_home, &certificate_path)?;
    let mut receiver_daemon = start_daemon(&receiver_home, &certificate_path)?;

    let outcome = (|| -> Result<()> {
        provision(&sender_home, &instance, &sender)?;
        provision(&receiver_home, &instance, &receiver)?;
        pin(&sender_home, &instance, &sender, &receiver)?;
        pin(&receiver_home, &instance, &receiver, &sender)?;
        request(
            &receiver_home,
            Request::SetReceiveEnabled {
                version: IPC_VERSION,
                enabled: true,
            },
        )?;
        let grant = request(
            &receiver_home,
            Request::AddDestination {
                version: IPC_VERSION,
                alias: "production-backend".into(),
                path: destination.clone(),
                approval: ApprovalMode::Auto,
            },
        )?
        .destinations
        .into_iter()
        .next()
        .context("receiver did not return a destination grant")?;

        let tree = source.join("private-tree");
        fs::create_dir(&tree)?;
        fs::create_dir(tree.join("empty"))?;
        let bytes = (0..131_075)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        fs::write(tree.join("payload.bin"), &bytes)?;
        fs::write(tree.join(".hidden"), b"private hidden canary")?;
        let job = request(
            &sender_home,
            Request::EnqueueJob {
                version: IPC_VERSION,
                target: receiver.id.into(),
                grant_id: grant.id.to_string(),
                sources: vec![tree.clone()],
            },
        )?
        .jobs
        .into_iter()
        .next()
        .context("sender did not create a device job")?;
        wait_for("daemon delivery", Duration::from_secs(90), || {
            matches!(
                job_state(&sender_home, job.id),
                Some(DeviceJobState::Delivered | DeviceJobState::Failed)
            )
        })?;
        ensure!(
            job_state(&sender_home, job.id) == Some(DeviceJobState::Delivered),
            "daemon transfer did not complete"
        );
        let delivered = destination.join("private-tree").join("payload.bin");
        ensure!(
            Sha256::digest(fs::read(&delivered)?) == Sha256::digest(&bytes),
            "receiver hash differs from sender bytes"
        );
        let receipt = load_device_receipt(&Store::open(
            &sender_home.join("transfers"),
            &job.id.to_string(),
        )?)?
        .context("sender did not persist a receipt")?;
        receipt.verify(&receiver.key.verifying_key())?;

        let active_tree = source.join("active-revoke-tree");
        fs::create_dir(&active_tree)?;
        write_large_fixture(&active_tree.join("payload.bin"), 768)?;
        let active_job = request(
            &sender_home,
            Request::EnqueueJob {
                version: IPC_VERSION,
                target: receiver.id.into(),
                grant_id: grant.id.to_string(),
                sources: vec![active_tree],
            },
        )?
        .jobs
        .into_iter()
        .next()
        .context("sender did not create an active revoke job")?;
        wait_for("active direct transfer", Duration::from_secs(120), || {
            job_state(&sender_home, active_job.id) == Some(DeviceJobState::Transferring)
        })?;
        revoke(&web, &jar, &instance, receiver.id)?;
        ensure!(
            native.devices().device_sync(&receiver.credential).is_err(),
            "web revoke left the receiver credential usable"
        );
        let response = web
            .post(format!("{instance}/api/native/v1/devices/connections"))
            .bearer_auth(&sender.credential)
            .json(&json!({"target_device_id": receiver.id}))
            .send()?;
        ensure!(
            response.status().is_client_error(),
            "web revoke left rendezvous admission usable"
        );
        wait_for(
            "active transfer revocation",
            Duration::from_secs(60),
            || {
                matches!(
                    job_state(&sender_home, active_job.id),
                    Some(DeviceJobState::Failed | DeviceJobState::Paused)
                )
            },
        )?;
        ensure!(
            job_state(&sender_home, active_job.id) != Some(DeviceJobState::Delivered),
            "sender delivered after the web revoke"
        );
        ensure!(
            !destination.join("active-revoke-tree").exists(),
            "receiver committed after the web revoke"
        );
        ensure!(
            sender_peer_denied(&sender_home, &instance, &sender, receiver.id)?,
            "sender registry gate retained the revoked receiver"
        );
        Ok(())
    })();

    stop(&sender_home, &mut sender_daemon);
    stop(&receiver_home, &mut receiver_daemon);
    outcome
}

fn login(web: &Client, jar: &Jar, instance: &str) -> Result<()> {
    let url = Url::parse(&format!("{instance}/login"))?;
    ensure!(
        web.get(url.clone()).send()?.status().is_success(),
        "web login page was unavailable"
    );
    let response = web.post(format!("{instance}/api/native/v1/session"))
        .header("Sec-Fetch-Site", "same-origin").header("X-XSRF-TOKEN", xsrf(jar, &url)?)
        .json(&json!({"email":"device-harness@example.test","password":"correct horse battery staple"})).send()?;
    ensure!(
        response.status().is_success(),
        "web session login failed with {}",
        response.status()
    );
    Ok(())
}

fn enroll_first(web: &Client, jar: &Jar, instance: &str, identity: &Identity) -> Result<String> {
    let challenge_response = challenge(web, jar, instance, identity)?;
    let challenge = challenge_response["data"]["challenge"]
        .as_str()
        .context("challenge missing")?;
    let response = web.post(format!("{instance}/api/native/v1/devices/enrollments/{}", identity.id))
        .header("Sec-Fetch-Site", "same-origin").header("X-XSRF-TOKEN", xsrf(jar, &Url::parse(instance)?)?)
        .json(&json!({"challenge": challenge, "signature": enrollment_signature(identity, &challenge)?})).send()?;
    let body = response_json(response, 201)?;
    body["data"]["credential"]
        .as_str()
        .map(str::to_owned)
        .context("first enrollment did not issue a credential")
}

fn enroll_and_approve(
    web: &Client,
    jar: &Jar,
    instance: &str,
    approver: &Identity,
    candidate: &Identity,
) -> Result<String> {
    let challenge_response = challenge(web, jar, instance, candidate)?;
    let challenge = challenge_response["data"]["challenge"]
        .as_str()
        .context("challenge missing")?;
    let response = web.post(format!("{instance}/api/native/v1/devices/enrollments/{}", candidate.id))
        .header("Sec-Fetch-Site", "same-origin").header("X-XSRF-TOKEN", xsrf(jar, &Url::parse(instance)?)?)
        .json(&json!({"challenge": challenge, "signature": enrollment_signature(candidate, &challenge)?})).send()?;
    response_json(response, 201)?;
    let approval = approval_signature(instance, approver, candidate, &challenge_response)?;
    let response = web
        .post(format!(
            "{instance}/api/native/v1/devices/enrollments/{}/approve",
            candidate.id
        ))
        .bearer_auth(&approver.credential)
        .json(&json!({"approval_signature": approval}))
        .send()?;
    response_json(response, 201)?;
    let response = web.post(format!("{instance}/api/native/v1/devices/enrollments/{}/credential", candidate.id))
        .json(&json!({"challenge": challenge, "signature": enrollment_signature(candidate, &challenge)?})).send()?;
    let body = response_json(response, 201)?;
    body["data"]["credential"]
        .as_str()
        .map(str::to_owned)
        .context("approved enrollment did not issue a credential")
}

fn challenge(web: &Client, jar: &Jar, instance: &str, identity: &Identity) -> Result<Value> {
    let response = web.post(format!("{instance}/api/native/v1/devices/enrollment-challenge"))
        .header("Sec-Fetch-Site", "same-origin").header("X-XSRF-TOKEN", xsrf(jar, &Url::parse(instance)?)?)
        .json(&json!({"device_id":identity.id,"display_name":"Production harness","platform":"linux","public_key":STANDARD.encode(identity.key.verifying_key().to_bytes()),"key_version":1,"capabilities":[]})).send()?;
    response_json(response, 201)
}

fn approval_signature(
    instance: &str,
    approver: &Identity,
    candidate: &Identity,
    challenge_response: &Value,
) -> Result<String> {
    let data = &challenge_response["data"]["approval_context"];
    let digest = STANDARD.decode(
        data["challenge_sha256"]
            .as_str()
            .context("approval digest missing")?,
    )?;
    let expiry = time::OffsetDateTime::parse(
        data["expires_at"]
            .as_str()
            .context("approval expiry missing")?,
        &time::format_description::well_known::Rfc3339,
    )?
    .unix_timestamp();
    let message = [
        "filebeam-device-approval-v2",
        instance,
        data["deployment_id"]
            .as_str()
            .context("deployment missing")?,
        &data["account_id"]
            .as_u64()
            .context("account missing")?
            .to_string(),
        &data["generation"]
            .as_u64()
            .context("generation missing")?
            .to_string(),
        candidate.id,
        "1",
        approver.id,
        "1",
        &expiry.to_string(),
    ]
    .join("\0")
    .into_bytes();
    let mut message = message;
    message.push(0);
    message.extend(digest);
    message.extend(candidate.key.verifying_key().to_bytes());
    Ok(STANDARD.encode(approver.key.sign(&message).to_bytes()))
}

fn enrollment_signature(identity: &Identity, challenge: &str) -> Result<String> {
    let challenge = STANDARD.decode(challenge)?;
    let message = [
        b"filebeam-device-enrollment-v1\0".as_slice(),
        identity.id.as_bytes(),
        b"\0",
        b"1\0",
        challenge.as_slice(),
    ]
    .concat();
    Ok(STANDARD.encode(identity.key.sign(&message).to_bytes()))
}

fn provision(home: &Path, instance: &str, identity: &Identity) -> Result<()> {
    request(
        home,
        Request::ProvisionDevice {
            version: IPC_VERSION,
            provision: DeviceProvision {
                instance: instance.into(),
                deployment: instance.into(),
                account_id: 1,
                generation: 1,
                device_id: identity.id.into(),
                key_version: 1,
                public_key: STANDARD.encode(identity.key.verifying_key().to_bytes()),
                signing_key: STANDARD.encode(identity.key.to_bytes()),
                credential: identity.credential.clone(),
            },
        },
    )?;
    Ok(())
}

fn pin(home: &Path, instance: &str, local: &Identity, peer: &Identity) -> Result<()> {
    let store = Store::open(&home.join("registry"), "device-registry-v1")?;
    let scope = RegistryScope {
        origin: Origin::parse(instance)?,
        deployment: DeploymentId::parse(instance)?,
        account: AccountId(1),
        generation: DeviceGeneration(1),
        local_device: DeviceId::parse(local.id)?,
    };
    let mut registry = DeviceRegistry::open(&store, scope, Duration::from_secs(60))?;
    registry.approve_pairing(&DeviceId::parse(peer.id)?, now())?;
    Ok(())
}

fn sender_peer_denied(home: &Path, instance: &str, local: &Identity, peer: &str) -> Result<bool> {
    let store = Store::open(&home.join("registry"), "device-registry-v1")?;
    let scope = RegistryScope {
        origin: Origin::parse(instance)?,
        deployment: DeploymentId::parse(instance)?,
        account: AccountId(1),
        generation: DeviceGeneration(1),
        local_device: DeviceId::parse(local.id)?,
    };
    let registry = DeviceRegistry::open(&store, scope.clone(), Duration::from_secs(60))?;
    Ok(registry
        .pinned_target(
            &scope,
            &DeviceId::parse(peer)?,
            filebeam_client_core::device_registry::RegistryRoute::Instance,
            now(),
        )
        .is_err())
}

fn write_large_fixture(path: &Path, mebibytes: usize) -> Result<()> {
    use std::io::Write;

    let block = (0..1024 * 1024)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let mut file = fs::File::create(path)?;
    for _ in 0..mebibytes {
        file.write_all(&block)?;
    }
    file.sync_all()?;
    Ok(())
}

fn revoke(web: &Client, jar: &Jar, instance: &str, device_id: &str) -> Result<()> {
    let response = web
        .delete(format!("{instance}/api/native/v1/devices/{device_id}"))
        .header("Sec-Fetch-Site", "same-origin")
        .header("X-XSRF-TOKEN", xsrf(jar, &Url::parse(instance)?)?)
        .json(&json!({"current_password":"correct horse battery staple"}))
        .send()?;
    response_json(response, 200)?;
    Ok(())
}

fn start_daemon(home: &Path, certificate: &Path) -> Result<Child> {
    let child = Command::new(env::current_exe()?)
        .arg("daemon")
        .arg(home)
        .arg(certificate)
        .spawn()?;
    wait_for("daemon IPC socket", Duration::from_secs(15), || {
        filebeam_daemon::socket_path(home).exists()
    })?;
    Ok(child)
}

fn stop(home: &Path, child: &mut Child) {
    let _ = filebeam_daemon::request(
        home,
        Request::Shutdown {
            version: IPC_VERSION,
        },
    );
    if child.wait().is_err() {
        let _ = child.kill();
    }
}

fn request(home: &Path, request: Request) -> Result<filebeam_daemon::Reply> {
    let reply = filebeam_daemon::request(home, request)?;
    if let Some(error) = &reply.error {
        bail!("daemon IPC request failed: {error}");
    }
    Ok(reply)
}

fn job_state(home: &Path, id: uuid::Uuid) -> Option<DeviceJobState> {
    request(
        home,
        Request::ListJobs {
            version: IPC_VERSION,
        },
    )
    .ok()?
    .jobs
    .into_iter()
    .find(|job| job.id == id)
    .map(|job| job.state)
}

fn response_json(response: reqwest::blocking::Response, expected: u16) -> Result<Value> {
    let status = response.status();
    let body = response.text()?;
    ensure!(
        status.as_u16() == expected,
        "Laravel returned {status}: {body}"
    );
    Ok(serde_json::from_str(&body)?)
}

fn xsrf(jar: &Jar, url: &Url) -> Result<String> {
    let header = jar
        .cookies(url)
        .context("Laravel did not set a session cookie")?;
    let cookies = header.to_str()?;
    let value = cookies
        .split("; ")
        .find_map(|entry| entry.strip_prefix("XSRF-TOKEN="))
        .context("Laravel did not set an XSRF token")?;
    url::form_urlencoded::parse(format!("x={value}").as_bytes())
        .next()
        .map(|(_, value)| value.into_owned())
        .context("invalid XSRF token")
}

fn wait_for(label: &str, timeout: Duration, condition: impl Fn() -> bool) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("timed out waiting for {label}")
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs().try_into().unwrap_or(i64::MAX))
        .unwrap_or(i64::MAX)
}
