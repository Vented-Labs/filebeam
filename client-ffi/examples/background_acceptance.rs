use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use filebeam_client_ffi::{
    BackgroundHeader, ClientConfig, JobState, NativeRuntime, PromptType, TransferClient,
    TransferJob, Transport, UploadAuthentication, UploadOptions,
};
use reqwest::{
    blocking::{Client, Response},
    header::{HeaderName, HeaderValue},
};
use sha2::{Digest, Sha256};

const RESPONSE_LIMIT: u64 = 2 * 1024 * 1024;
const PASSWORD: &str = "fixture-password-9";

fn main() {
    if let Err(error) = run() {
        eprintln!("background acceptance FAILED: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let instance = env::var("FILEBEAM_ACCEPTANCE_INSTANCE")?;
    if env::var("FILEBEAM_ACCEPTANCE_ALLOW_HTTP").as_deref() != Ok("1") {
        return Err("FILEBEAM_ACCEPTANCE_ALLOW_HTTP=1 is required for this loopback test".into());
    }
    let root = unique_temp_dir("background-acceptance")?;
    let state = root.join("state");
    let fixtures = root.join("fixtures");
    fs::create_dir_all(&fixtures)?;
    let runtime = Arc::new(NativeRuntime::new(config(&state))?);
    let client = new_client(&state, &runtime)?;
    let chunk = client.discover(instance.clone())?.chunk_bytes;
    if chunk == 0 {
        return Err("server advertised a zero chunk size".into());
    }

    let normal = write_fixture(&fixtures.join("normal-boundaries.bin"), chunk, 2)?;
    let password = write_fixture(&fixtures.join("password-boundaries.bin"), chunk, 3)?;
    let zip_a = write_fixture(&fixtures.join("archive-alpha.bin"), chunk, 1)?;
    let zip_b = write_fixture(&fixtures.join("archive-beta.bin"), chunk, 4)?;
    let turbo = write_fixture(&fixtures.join("turbo-boundaries.bin"), chunk, 5)?;

    exercise_files(
        &instance,
        &state,
        &runtime,
        "normal",
        vec![normal],
        false,
        false,
    )?;
    exercise_files(
        &instance,
        &state,
        &runtime,
        "password",
        vec![password],
        true,
        false,
    )?;
    exercise_files(
        &instance,
        &state,
        &runtime,
        "zip",
        vec![zip_a, zip_b],
        false,
        true,
    )?;
    exercise_turbo(&instance, &state, &runtime, turbo)?;
    fs::remove_dir_all(root)?;
    Ok(())
}

fn config(state: &Path) -> ClientConfig {
    ClientConfig {
        state_directory: state.display().to_string(),
        memory_budget_mib: 128,
        max_concurrency: 2,
        relay_only: true,
        allow_http: true,
    }
}

fn new_client(
    state: &Path,
    runtime: &Arc<NativeRuntime>,
) -> Result<TransferClient, Box<dyn Error>> {
    // The filesystem secret store is intentionally ephemeral because state is a test-only tempdir.
    Ok(TransferClient::new_with_runtime(
        config(state),
        runtime.clone(),
    )?)
}

fn write_fixture(path: &Path, chunk: u64, salt: u8) -> Result<Fixture, Box<dyn Error>> {
    let size = chunk
        .checked_mul(2)
        .and_then(|n| n.checked_add(257 + u64::from(salt)))
        .ok_or("fixture size overflow")?;
    let mut file = File::create(path)?;
    let mut remaining = size;
    let mut offset = 0u64;
    let mut block = [0u8; 8192];
    let mut digest = Sha256::new();
    while remaining > 0 {
        let count = remaining.min(block.len() as u64) as usize;
        for (index, byte) in block[..count].iter_mut().enumerate() {
            *byte = (offset.wrapping_add(index as u64).wrapping_mul(31) as u8) ^ salt;
        }
        file.write_all(&block[..count])?;
        digest.update(&block[..count]);
        offset += count as u64;
        remaining -= count as u64;
    }
    Ok(Fixture {
        path: path.to_path_buf(),
        size,
        hash: digest_hex(digest.finalize().as_slice()),
    })
}

fn exercise_files(
    instance: &str,
    state: &Path,
    runtime: &Arc<NativeRuntime>,
    label: &str,
    fixtures: Vec<Fixture>,
    password: bool,
    archive: bool,
) -> Result<(), Box<dyn Error>> {
    let client = new_client(state, runtime)?;
    let background = client.background_transfer();
    let job = background.prepare_upload(
        instance.into(),
        fixtures
            .iter()
            .map(|fixture| fixture.path.display().to_string())
            .collect(),
        options(password, archive, false),
    )?;
    let prepared = wait_job(&job, password, false)?;
    let checkpoint = prepared
        .checkpoint_id
        .ok_or("upload preparation did not return a checkpoint")?;
    drop(job);
    drop(background);
    drop(client);

    let link = upload_descriptors(instance, state, runtime, &checkpoint, password)?;
    let (bare_link, separate_key) = link
        .split_once("#k=")
        .ok_or("finalized share link did not include a portable key")?;
    // Keep the separately presented key in RAM; do not print or persist either value.
    let keyed_link = format!("{bare_link}#k={separate_key}");
    let output = state.join(format!("download-{label}"));
    let paths = download_descriptors(instance, state, runtime, &keyed_link, &output, password)?;
    if archive {
        verify_zip(&paths, &fixtures)?;
    } else {
        verify_files(&paths, &fixtures)?;
    }
    revoke(state, runtime, &checkpoint)?;
    print_pass(label, &fixtures);
    Ok(())
}

fn exercise_turbo(
    instance: &str,
    state: &Path,
    runtime: &Arc<NativeRuntime>,
    fixture: Fixture,
) -> Result<(), Box<dyn Error>> {
    let client = new_client(state, runtime)?;
    let background = client.background_transfer();
    let job = background.prepare_upload(
        instance.into(),
        vec![fixture.path.display().to_string()],
        options(false, false, true),
    )?;
    let prepared = wait_job(&job, false, true)?;
    let checkpoint = prepared
        .checkpoint_id
        .ok_or("Turbo preparation did not return a checkpoint")?;
    drop(job);
    drop(background);
    drop(client);
    let link = upload_descriptors(instance, state, runtime, &checkpoint, false)?;
    let output = state.join("download-turbo");
    let paths = download_descriptors(instance, state, runtime, &link, &output, false)?;
    verify_files(&paths, std::slice::from_ref(&fixture))?;
    revoke(state, runtime, &checkpoint)?;
    print_pass("turbo", &[fixture]);
    Ok(())
}

fn options(password: bool, archive: bool, turbo: bool) -> UploadOptions {
    UploadOptions {
        transport: Transport::Http,
        turbo,
        archive,
        password,
        retention_hours: None,
        authentication: UploadAuthentication {
            bearer_token: None,
            session_cookie: None,
        },
        recipient: None,
    }
}

fn upload_descriptors(
    instance: &str,
    state: &Path,
    runtime: &Arc<NativeRuntime>,
    checkpoint: &str,
    password: bool,
) -> Result<String, Box<dyn Error>> {
    let client = new_client(state, runtime)?;
    let background = client.background_transfer();
    let work = background.pending_upload_work(checkpoint.into())?;
    if work.is_empty() {
        return Err("upload yielded no HTTP work".into());
    }
    if let Some(descriptor) = work.into_iter().next() {
        let (status, response) = put_file(&descriptor)?;
        background.ingest_upload_completion(
            checkpoint.into(),
            descriptor.operation_id.clone(),
            status,
            Some(response.display().to_string()),
        )?;
        // A redelivered URLSession completion must be harmless after a client restart.
        drop(background);
        drop(client);
        let restarted = new_client(state, runtime)?;
        let restarted_background = restarted.background_transfer();
        restarted_background.ingest_upload_completion(
            checkpoint.into(),
            descriptor.operation_id,
            status,
            Some(response.display().to_string()),
        )?;
        fs::remove_file(response)?;
        drop(restarted_background);
        drop(restarted);
        return finish_remaining_uploads(instance, state, runtime, checkpoint, password);
    }
    unreachable!()
}

fn finish_remaining_uploads(
    _instance: &str,
    state: &Path,
    runtime: &Arc<NativeRuntime>,
    checkpoint: &str,
    password: bool,
) -> Result<String, Box<dyn Error>> {
    loop {
        let client = new_client(state, runtime)?;
        let background = client.background_transfer();
        let work = background.pending_upload_work(checkpoint.into())?;
        if work.is_empty() {
            let job = background.finalize_upload(checkpoint.into());
            let done = wait_job(&job, password, false)?;
            drop(job);
            return done
                .results
                .into_iter()
                .next()
                .ok_or("upload finalization returned no link".into());
        }
        for descriptor in work {
            let (status, response) = put_file(&descriptor)?;
            background.ingest_upload_completion(
                checkpoint.into(),
                descriptor.operation_id,
                status,
                Some(response.display().to_string()),
            )?;
            fs::remove_file(response)?;
        }
    }
}

fn download_descriptors(
    instance: &str,
    state: &Path,
    runtime: &Arc<NativeRuntime>,
    link: &str,
    output: &Path,
    password: bool,
) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    fs::create_dir_all(output)?;
    let client = new_client(state, runtime)?;
    let background = client.background_transfer();
    let job =
        background.prepare_download(instance.into(), link.into(), output.display().to_string())?;
    let prepared = wait_job(&job, password, false)?;
    let checkpoint = prepared
        .checkpoint_id
        .ok_or("download preparation did not return a checkpoint")?;
    drop(job);
    drop(background);
    drop(client);
    loop {
        let client = new_client(state, runtime)?;
        let background = client.background_transfer();
        let work = background.pending_download_work(checkpoint.clone())?;
        if work.is_empty() {
            let job = background.finalize_download(checkpoint.clone());
            let done = wait_job(&job, password, false)?;
            drop(job);
            return Ok(done.results.into_iter().map(PathBuf::from).collect());
        }
        for descriptor in work {
            let (status, headers, response) = get_file(&descriptor)?;
            background.ingest_download_completion(
                checkpoint.clone(),
                descriptor.operation_id,
                status,
                headers,
                response.display().to_string(),
            )?;
            fs::remove_file(response)?;
        }
    }
}

fn put_file(work: &filebeam_client_ffi::BackgroundWork) -> Result<(u16, PathBuf), Box<dyn Error>> {
    if work.method != "PUT" {
        return Err("unexpected upload HTTP method".into());
    }
    let request = request(work)?.body(File::open(&work.body_path)?).send()?;
    let status = request.status().as_u16();
    let path = unique_temp_file("upload-response")?;
    copy_bounded(
        request,
        &path,
        work.expected_response_bytes.min(RESPONSE_LIMIT),
    )?;
    Ok((status, path))
}

fn get_file(
    work: &filebeam_client_ffi::BackgroundWork,
) -> Result<(u16, Vec<BackgroundHeader>, PathBuf), Box<dyn Error>> {
    if work.method != "GET" {
        return Err("unexpected download HTTP method".into());
    }
    let response = request(work)?.send()?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| BackgroundHeader {
            name: name.as_str().into(),
            value: value.to_str().unwrap_or_default().into(),
        })
        .collect();
    let path = unique_temp_file("download-response")?;
    copy_bounded(response, &path, work.expected_response_bytes)?;
    Ok((status, headers, path))
}

fn request(
    work: &filebeam_client_ffi::BackgroundWork,
) -> Result<reqwest::blocking::RequestBuilder, Box<dyn Error>> {
    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
    let mut request = client.request(work.method.parse()?, &work.url);
    for header in &work.headers {
        request = request.header(
            HeaderName::from_bytes(header.name.as_bytes())?,
            HeaderValue::from_str(&header.value)?,
        );
    }
    Ok(request)
}

fn copy_bounded(mut response: Response, path: &Path, limit: u64) -> Result<(), Box<dyn Error>> {
    let mut output = File::create(path)?;
    let mut total = 0u64;
    let mut buffer = [0u8; 8192];
    loop {
        let count = response.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or("response size overflow")?;
        if total > limit {
            return Err("HTTP response exceeded descriptor bound".into());
        }
        output.write_all(&buffer[..count])?;
    }
    Ok(())
}

fn wait_job(
    job: &Arc<TransferJob>,
    password: bool,
    require_early_link: bool,
) -> Result<filebeam_client_ffi::TransferSnapshot, Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut early_link = false;
    loop {
        let snapshot = job.snapshot();
        if require_early_link
            && snapshot.share_url.is_some()
            && !matches!(snapshot.state, JobState::Complete)
        {
            early_link = true;
        }
        if let Some(ref prompt) = snapshot.prompt {
            if password && matches!(prompt.kind, PromptType::Password) {
                job.respond(prompt.id, PASSWORD.into())?;
            } else if matches!(prompt.kind, PromptType::ShareReady) {
                job.respond(prompt.id, "ack".into())?;
            } else {
                return Err(match prompt.kind {
                    PromptType::ShareKey => "unexpected share-key prompt",
                    PromptType::Password => "unexpected password prompt",
                    PromptType::PeerConsent => "unexpected peer-consent prompt",
                    PromptType::Directory => "unexpected directory prompt",
                    PromptType::ShareReady => unreachable!(),
                }
                .into());
            }
        }
        match snapshot.state {
            JobState::Complete => {
                if require_early_link && !early_link {
                    return Err("Turbo link was not observable before upload completion".into());
                }
                return Ok(snapshot);
            }
            JobState::Failed => {
                return Err(snapshot
                    .error
                    .unwrap_or_else(|| "transfer failed".into())
                    .into());
            }
            _ if Instant::now() >= deadline => return Err("transfer timed out".into()),
            _ => thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn verify_files(paths: &[PathBuf], fixtures: &[Fixture]) -> Result<(), Box<dyn Error>> {
    if paths.len() != fixtures.len() {
        return Err("download returned an unexpected file count".into());
    }
    for fixture in fixtures {
        let path = paths
            .iter()
            .find(|path| path.file_name() == fixture.path.file_name())
            .ok_or("downloaded fixture is missing")?;
        if file_hash(path)? != fixture.hash || fs::metadata(path)?.len() != fixture.size {
            return Err("downloaded file differs from fixture".into());
        }
    }
    Ok(())
}

fn verify_zip(paths: &[PathBuf], fixtures: &[Fixture]) -> Result<(), Box<dyn Error>> {
    let archive = paths.first().ok_or("ZIP download returned no archive")?;
    let mut zip = zip::ZipArchive::new(File::open(archive)?)?;
    if zip.len() != fixtures.len() {
        return Err("ZIP has an unexpected file count".into());
    }
    for fixture in fixtures {
        let name = fixture
            .path
            .file_name()
            .ok_or("fixture has no name")?
            .to_string_lossy();
        let mut entry = zip.by_name(&name)?;
        let mut hasher = Sha256::new();
        let bytes = io::copy(&mut entry, &mut HashWriter(&mut hasher))?;
        if bytes != fixture.size || digest_hex(hasher.finalize().as_slice()) != fixture.hash {
            return Err("ZIP entry differs from fixture".into());
        }
    }
    Ok(())
}

struct HashWriter<'a>(&'a mut Sha256);
impl Write for HashWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.update(buffer);
        Ok(buffer.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn file_hash(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest_hex(digest.finalize().as_slice()))
}

fn digest_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn revoke(
    state: &Path,
    runtime: &Arc<NativeRuntime>,
    checkpoint: &str,
) -> Result<(), Box<dyn Error>> {
    let client = new_client(state, runtime)?;
    let job = client.revoke_upload(checkpoint.into());
    let _ = wait_job(&job, false, false)?;
    Ok(())
}

fn print_pass(label: &str, fixtures: &[Fixture]) {
    let sizes = fixtures
        .iter()
        .map(|fixture| fixture.size.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let hashes = fixtures
        .iter()
        .map(|fixture| fixture.hash.as_str())
        .collect::<Vec<_>>()
        .join(",");
    println!("{label} PASS sizes={sizes} sha256={hashes}");
}

#[derive(Clone)]
struct Fixture {
    path: PathBuf,
    size: u64,
    hash: String,
}

fn unique_temp_dir(prefix: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&path)?;
    Ok(path)
}

fn unique_temp_file(prefix: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    File::create(&path)?;
    Ok(path)
}
