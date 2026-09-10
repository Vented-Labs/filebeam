use std::{
    env, fs,
    io::{Cursor, Read, Write},
    path::Path,
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(windows)]
use std::{path::PathBuf, process::Command, thread};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use flate2::read::GzDecoder;
use reqwest::{
    Url,
    blocking::{Client, Response},
};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tar::Archive;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::config::Config;

const CATALOG: &str = "https://releases.filebeam.io/cli/index.json";
const MAX_CATALOG_BYTES: u64 = 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Deserialize)]
struct Envelope {
    signed: String,
    signature: String,
}

#[derive(Deserialize, Debug)]
struct Catalog {
    schema: u64,
    generation: u64,
    published_at: String,
    expires_at: String,
    releases: Vec<Release>,
}

#[derive(Deserialize, Debug)]
struct Release {
    version: String,
    assets: Vec<Asset>,
}

#[derive(Clone, Deserialize, Debug)]
struct Asset {
    #[serde(default)]
    os: Option<String>,
    architecture: String,
    path: String,
    sha256: String,
    size: u64,
}

pub fn check(config: &Config) -> Result<String> {
    let (generation, update) = available(config, Duration::from_secs(8))?;
    let Some((version, asset)) = update else {
        store_generation(config, generation)?;
        bail!("beam is already on the latest release");
    };
    let staged = install(config, &version, &asset)?;
    store_generation(config, generation)?;
    let previous = config.home.join("bin").join(previous_executable_name());
    if staged {
        Ok(format!(
            "staged beam {version}; it will replace the current binary after this command exits, and the previous binary will be at {}",
            previous.display()
        ))
    } else {
        Ok(format!(
            "updated beam to {version}; the previous binary is at {}",
            previous.display()
        ))
    }
}

pub fn notify_if_available(config: &Config) {
    if !config.check_updates || trusted_key().is_err() {
        return;
    }
    let cache = config.cache_dir();
    let attempted = cache.join("update-checked");
    let fresh = fs::metadata(&attempted)
        .and_then(|metadata| metadata.modified())
        .and_then(|modified| modified.elapsed().map_err(std::io::Error::other))
        .is_ok_and(|elapsed| elapsed < Duration::from_secs(24 * 60 * 60));

    if fresh {
        notify_cached(&cache);
        return;
    }
    if config.ensure_cache_dir().is_err() {
        return;
    }
    let _ = fs::write(&attempted, b"");
    if let Ok((_, Some((version, _)))) = available(config, Duration::from_millis(1200)) {
        let _ = fs::write(cache.join("latest"), version.to_string());
        eprintln!("beam {version} is available; run `beam update`");
    }
}

fn notify_cached(cache: &Path) {
    let Ok(value) = fs::read_to_string(cache.join("latest")) else {
        return;
    };
    let Ok(latest) = Version::parse(value.trim()) else {
        return;
    };
    let Ok(current) = Version::parse(env!("BEAM_VERSION")) else {
        return;
    };
    if latest > current {
        eprintln!("beam {latest} is available; run `beam update`");
    }
}

fn available(config: &Config, timeout: Duration) -> Result<(u64, Option<(Version, Asset)>)> {
    let key = trusted_key()?;
    let response = Client::builder()
        .connect_timeout(timeout)
        .timeout(timeout)
        .build()?
        .get(CATALOG)
        .send()?
        .error_for_status()?;
    let payload = read_limited(response, MAX_CATALOG_BYTES)?;
    let catalog = verify_catalog(&payload, &key)?;
    verify_generation(config, catalog.generation)?;
    let generation = catalog.generation;
    let latest = catalog
        .releases
        .into_iter()
        .filter_map(|release| {
            Version::parse(&release.version)
                .ok()
                .map(|version| (version, release.assets))
        })
        .max_by(|left, right| left.0.cmp(&right.0))
        .context("catalog contains no valid releases")?;
    let current = Version::parse(env!("BEAM_VERSION"))?;
    if latest.0 <= current {
        return Ok((generation, None));
    }
    let asset = select_asset(latest.1)?
        .context("no update asset exists for this operating system and architecture")?;
    validate_asset(&asset)?;
    Ok((generation, Some((latest.0, asset))))
}

fn verify_generation(config: &Config, generation: u64) -> Result<()> {
    let path = config.cache_dir().join("catalog-generation");
    let previous = fs::read_to_string(&path)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0);
    if generation < previous {
        bail!("release catalog generation rolled back");
    }
    Ok(())
}

fn store_generation(config: &Config, generation: u64) -> Result<()> {
    fs::write(
        config.ensure_cache_dir()?.join("catalog-generation"),
        generation.to_string(),
    )?;
    Ok(())
}

fn install(config: &Config, version: &Version, asset: &Asset) -> Result<bool> {
    let url = Url::parse(CATALOG)?.join(&asset.path)?;
    if url.scheme() != "https"
        || url.host_str() != Some("releases.filebeam.io")
        || !url.path().starts_with("/cli/versions/")
    {
        bail!("signed catalog asset URL is outside the Filebeam release origin");
    }
    let response = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()?
        .get(url)
        .send()?
        .error_for_status()?;
    if response
        .content_length()
        .is_some_and(|size| size != asset.size)
    {
        bail!("release archive size does not match the signed catalog");
    }
    let archive = read_limited(response, asset.size)?;
    if archive.len() as u64 != asset.size
        || hex::encode(Sha256::digest(&archive)) != asset.sha256.to_ascii_lowercase()
    {
        bail!("release archive checksum does not match the signed catalog");
    }
    let binary = extract_binary(&archive)?;
    replace_binary(config, &binary).with_context(|| format!("install beam {version}"))
}

fn read_limited(response: Response, maximum: u64) -> Result<Vec<u8>> {
    if response.content_length().is_some_and(|size| size > maximum) {
        bail!("download exceeds its allowed size");
    }
    let mut bytes = Vec::with_capacity(maximum.min(1024 * 1024) as usize);
    response.take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        bail!("download exceeds its allowed size");
    }
    Ok(bytes)
}

fn extract_binary(bytes: &[u8]) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        extract_zip_binary(bytes)
    }
    #[cfg(not(windows))]
    {
        extract_tar_binary(bytes)
    }
}

#[cfg(not(windows))]
fn extract_tar_binary(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut archive = Archive::new(GzDecoder::new(Cursor::new(bytes)));
    for entry in archive.entries().context("read release archive")? {
        let mut entry = entry?;
        if entry.path()?.as_ref() != Path::new(executable_archive_path()) {
            continue;
        }
        if !entry.header().entry_type().is_file() || entry.size() > MAX_ARCHIVE_BYTES {
            bail!("release archive contains an invalid beam executable");
        }
        let mut binary = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut binary)?;
        if binary.is_empty() {
            bail!("release archive contains an empty beam executable");
        }
        return Ok(binary);
    }
    bail!(
        "release archive does not contain {}",
        executable_archive_path()
    )
}

#[cfg(windows)]
fn extract_zip_binary(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("read release archive")?;
    let expected = executable_archive_path();
    let mut entry = archive
        .by_name(expected)
        .with_context(|| format!("release archive does not contain {expected}"))?;
    if entry.is_dir() || entry.size() > MAX_ARCHIVE_BYTES {
        bail!("release archive contains an invalid beam executable");
    }
    let mut binary = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut binary)?;
    if binary.is_empty() {
        bail!("release archive contains an empty beam executable");
    }
    Ok(binary)
}

fn replace_binary(config: &Config, binary: &[u8]) -> Result<bool> {
    let bin = config.home.join("bin");
    fs::create_dir_all(&bin)?;
    let destination = bin.join(executable_name());
    ensure_running_installed_binary(&destination)?;
    let candidate = bin.join(format!(
        ".beam-update-{}{}",
        std::process::id(),
        executable_suffix()
    ));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&candidate)?;
    file.write_all(binary)?;
    file.sync_all()?;
    #[cfg(unix)]
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755))?;
    #[cfg(windows)]
    {
        stage_windows_replacement(&candidate, &destination)?;
        return Ok(true);
    }
    #[cfg(unix)]
    replace_unix_binary(&candidate, &destination)?;
    Ok(false)
}

#[cfg(unix)]
fn replace_unix_binary(candidate: &Path, destination: &Path) -> Result<()> {
    let backup = destination.with_file_name(previous_executable_name());
    let _ = fs::remove_file(&backup);
    fs::rename(destination, &backup)?;
    if let Err(error) = fs::rename(candidate, destination) {
        let _ = fs::rename(&backup, destination);
        let _ = fs::remove_file(candidate);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(windows)]
fn stage_windows_replacement(candidate: &Path, destination: &Path) -> Result<()> {
    Command::new(candidate)
        .arg("--beam-apply-update")
        .arg(destination)
        .spawn()
        .context("start Windows update helper")?;
    Ok(())
}

#[cfg(windows)]
pub fn apply_staged_update() -> Result<bool> {
    let mut arguments = env::args_os();
    let _ = arguments.next();
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new("--beam-apply-update")) {
        return Ok(false);
    }
    let destination = arguments
        .next()
        .map(PathBuf::from)
        .context("Windows update helper is missing its destination")?;
    if arguments.next().is_some() {
        bail!("Windows update helper received unexpected arguments");
    }
    let candidate = env::current_exe()?;
    let candidate_directory = candidate
        .parent()
        .context("Windows update helper has no executable directory")?
        .canonicalize()?;
    let destination_directory = destination
        .parent()
        .context("Windows update helper destination has no directory")?
        .canonicalize()?;
    if destination.file_name() != Some(std::ffi::OsStr::new(executable_name()))
        || destination_directory != candidate_directory
    {
        bail!("Windows update helper destination is invalid");
    }
    let backup = destination.with_file_name(previous_executable_name());
    let error_path = destination.with_file_name("beam.update-error");
    let result = (|| {
        for _ in 0..300 {
            let _ = fs::remove_file(&backup);
            match fs::rename(&destination, &backup) {
                Ok(()) => {
                    if let Err(error) = fs::copy(&candidate, &destination) {
                        let _ = fs::rename(&backup, &destination);
                        return Err(error.into());
                    }
                    let _ = fs::remove_file(&error_path);
                    return Ok(());
                }
                Err(_) => thread::sleep(Duration::from_millis(100)),
            }
        }
        bail!("timed out waiting for beam.exe to exit before applying update")
    })();
    if let Err(error) = result {
        let _ = fs::write(&error_path, error.to_string());
        return Err(error);
    }
    Ok(true)
}

#[cfg(windows)]
pub fn report_staged_update_error() {
    let Ok(executable) = env::current_exe() else {
        return;
    };
    let Some(directory) = executable.parent() else {
        return;
    };
    let error_path = directory.join("beam.update-error");
    let Ok(error) = fs::read_to_string(&error_path) else {
        return;
    };
    let _ = fs::remove_file(error_path);
    eprintln!(
        "beam update failed: {}; run `beam update` to retry",
        error.trim()
    );
}

#[cfg(not(windows))]
pub fn apply_staged_update() -> Result<bool> {
    Ok(false)
}

#[cfg(not(windows))]
pub fn report_staged_update_error() {}

fn ensure_running_installed_binary(destination: &Path) -> Result<()> {
    let current = env::current_exe()?.canonicalize()?;
    let expected = destination
        .canonicalize()
        .context("beam update requires an installer-managed binary")?;
    if current != expected {
        bail!("beam update requires running the installer-managed binary");
    }
    Ok(())
}

fn trusted_key() -> Result<VerifyingKey> {
    let encoded = option_env!("BEAM_RELEASE_PUBLIC_KEY")
        .filter(|value| !value.is_empty())
        .context(
            "updates are unavailable: this build has no embedded trusted release signing key",
        )?;
    let bytes = STANDARD
        .decode(encoded)
        .context("embedded release public key is not base64")?;
    VerifyingKey::from_bytes(
        bytes
            .as_slice()
            .try_into()
            .context("embedded release public key must be 32 bytes")?,
    )
    .context("embedded release public key is invalid")
}

fn verify_catalog(bytes: &[u8], key: &VerifyingKey) -> Result<Catalog> {
    let envelope: Envelope =
        serde_json::from_slice(bytes).context("release catalog envelope is invalid")?;
    let signed = STANDARD
        .decode(envelope.signed)
        .context("release catalog payload is not base64")?;
    let signature = Signature::from_slice(
        &STANDARD
            .decode(envelope.signature)
            .context("release catalog signature is not base64")?,
    )
    .context("release catalog signature is invalid")?;
    key.verify(&signed, &signature)
        .context("release catalog signature verification failed")?;
    let catalog: Catalog =
        serde_json::from_slice(&signed).context("signed release catalog payload is invalid")?;
    let expires = OffsetDateTime::parse(&catalog.expires_at, &Rfc3339)
        .context("signed release catalog expiry is invalid")?;
    if catalog.schema != 1
        || catalog.generation == 0
        || catalog.published_at.is_empty()
        || expires <= OffsetDateTime::now_utc()
    {
        bail!("signed release catalog is expired or unsupported");
    }
    Ok(catalog)
}

fn validate_asset(asset: &Asset) -> Result<()> {
    if !valid_sha256(&asset.sha256)
        || asset.path.starts_with('/')
        || asset
            .path
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
        || !asset.path.starts_with("versions/")
        || !asset.path.ends_with(archive_extension())
        || asset.size == 0
        || asset.size > MAX_ARCHIVE_BYTES
        || asset
            .os
            .as_deref()
            .is_some_and(|os| !matches!(os, "linux" | "macos" | "windows"))
    {
        bail!("signed catalog contains an unsafe asset");
    }
    Ok(())
}

fn select_asset(assets: Vec<Asset>) -> Result<Option<Asset>> {
    let os = os()?;
    let architecture = architecture()?;
    Ok(assets
        .iter()
        .find(|asset| asset.architecture == architecture && asset.os.as_deref() == Some(os))
        .or_else(|| {
            (os == "linux").then(|| {
                assets
                    .iter()
                    .find(|asset| asset.architecture == architecture && asset.os.is_none())
            })?
        })
        .cloned())
}

fn os() -> Result<&'static str> {
    if cfg!(target_os = "windows") {
        Ok("windows")
    } else if cfg!(target_os = "macos") {
        Ok("macos")
    } else if cfg!(target_os = "linux") {
        Ok("linux")
    } else {
        bail!("updates are unsupported on this operating system")
    }
}

fn architecture() -> Result<&'static str> {
    if cfg!(target_arch = "x86_64") {
        Ok("x86_64")
    } else if cfg!(target_arch = "aarch64") {
        Ok("aarch64")
    } else {
        bail!("updates are unsupported on this architecture")
    }
}

fn executable_name() -> &'static str {
    if cfg!(windows) { "beam.exe" } else { "beam" }
}

fn previous_executable_name() -> &'static str {
    if cfg!(windows) {
        "beam.previous.exe"
    } else {
        "beam.previous"
    }
}

fn executable_suffix() -> &'static str {
    if cfg!(windows) { ".exe" } else { "" }
}

fn executable_archive_path() -> &'static str {
    if cfg!(windows) {
        "beam/beam.exe"
    } else {
        "beam/beam"
    }
}

fn archive_extension() -> &'static str {
    if cfg!(windows) { ".zip" } else { ".tar.gz" }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn signed_catalog_schema_verifies() {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let payload = format!(
            r#"{{"schema":1,"generation":1,"published_at":"2026-01-01T00:00:00Z","expires_at":"2999-01-08T00:00:00Z","releases":[{{"version":"0.1.1","assets":[{{"architecture":"x86_64","path":"versions/v0.1.1/beam.tar.gz","sha256":"{}","size":1}}]}}]}}"#,
            "a".repeat(64)
        );
        let envelope = serde_json::json!({
            "signed": STANDARD.encode(payload.as_bytes()),
            "signature": STANDARD.encode(signing.sign(payload.as_bytes()).to_bytes())
        });
        let catalog = verify_catalog(
            &serde_json::to_vec(&envelope).unwrap(),
            &signing.verifying_key(),
        )
        .unwrap();
        assert_eq!(catalog.releases[0].version, "0.1.1");
        assert!(valid_sha256(&catalog.releases[0].assets[0].sha256));
    }

    #[cfg(not(windows))]
    #[test]
    fn binary_is_extracted_only_from_expected_archive_path() {
        let mut bytes = Vec::new();
        {
            let encoder = flate2::write::GzEncoder::new(&mut bytes, flate2::Compression::fast());
            let mut archive = tar::Builder::new(encoder);
            let content = b"binary";
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            archive
                .append_data(&mut header, "beam/beam", &content[..])
                .unwrap();
            archive.finish().unwrap();
        }
        assert_eq!(extract_binary(&bytes).unwrap(), b"binary");
    }

    #[cfg(windows)]
    #[test]
    fn binary_is_extracted_only_from_expected_archive_path() {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut bytes);
            archive
                .start_file("beam/beam.exe", zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(b"binary").unwrap();
            archive.finish().unwrap();
        }
        assert_eq!(extract_binary(bytes.get_ref()).unwrap(), b"binary");
    }

    #[test]
    fn selects_assets_by_os_and_architecture() {
        let os = os().unwrap();
        let architecture = architecture().unwrap();
        let asset = |os: Option<&str>, architecture: &str| Asset {
            os: os.map(str::to_owned),
            architecture: architecture.to_owned(),
            path: "versions/v0.1.1/beam.tar.gz".to_owned(),
            sha256: "a".repeat(64),
            size: 1,
        };
        let selected = select_asset(vec![
            asset(Some("windows"), architecture),
            asset(Some(os), "other"),
            asset(Some(os), architecture),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(selected.os.as_deref(), Some(os));
        assert_eq!(selected.architecture, architecture);
    }

    #[test]
    fn legacy_osless_assets_are_linux_only() {
        let os = os().unwrap();
        let legacy = Asset {
            os: None,
            architecture: architecture().unwrap().to_owned(),
            path: "versions/v0.1.1/beam.tar.gz".to_owned(),
            sha256: "a".repeat(64),
            size: 1,
        };
        let selected = select_asset(vec![legacy]).unwrap();
        assert_eq!(selected.is_some(), os == "linux");
    }

    #[test]
    fn executable_paths_match_the_platform() {
        if cfg!(windows) {
            assert_eq!(executable_archive_path(), "beam/beam.exe");
            assert_eq!(executable_name(), "beam.exe");
            assert_eq!(archive_extension(), ".zip");
        } else {
            assert_eq!(executable_archive_path(), "beam/beam");
            assert_eq!(executable_name(), "beam");
            assert_eq!(archive_extension(), ".tar.gz");
        }
    }
}
