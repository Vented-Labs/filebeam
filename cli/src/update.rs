use std::{
    env, fs,
    io::{Cursor, Read, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
    time::Duration,
};

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

#[derive(Deserialize, Debug)]
struct Asset {
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
    install(config, &version, &asset)?;
    store_generation(config, generation)?;
    Ok(format!(
        "updated beam to {version}; the previous binary is at {}",
        config.home.join("bin/beam.previous").display()
    ))
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
    let asset = latest
        .1
        .into_iter()
        .find(|asset| asset.architecture == architecture())
        .context("no update asset exists for this architecture")?;
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

fn install(config: &Config, version: &Version, asset: &Asset) -> Result<()> {
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
    let mut archive = Archive::new(GzDecoder::new(Cursor::new(bytes)));
    for entry in archive.entries().context("read release archive")? {
        let mut entry = entry?;
        if entry.path()?.as_ref() != Path::new("beam/beam") {
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
    bail!("release archive does not contain beam/beam")
}

fn replace_binary(config: &Config, binary: &[u8]) -> Result<()> {
    let bin = config.home.join("bin");
    fs::create_dir_all(&bin)?;
    let destination = bin.join("beam");
    ensure_running_installed_binary(&destination)?;
    let candidate = bin.join(format!(".beam-update-{}", std::process::id()));
    let backup = bin.join("beam.previous");
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&candidate)?;
    file.write_all(binary)?;
    file.sync_all()?;
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755))?;
    let _ = fs::remove_file(&backup);
    fs::rename(&destination, &backup)?;
    if let Err(error) = fs::rename(&candidate, &destination) {
        let _ = fs::rename(&backup, &destination);
        let _ = fs::remove_file(&candidate);
        return Err(error.into());
    }
    Ok(())
}

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
        || asset.size == 0
        || asset.size > MAX_ARCHIVE_BYTES
    {
        bail!("signed catalog contains an unsafe asset");
    }
    Ok(())
}

fn architecture() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unsupported"
    }
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
}
