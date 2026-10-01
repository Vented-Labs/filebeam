use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use toml_edit::{DocumentMut, Item, value};
use url::Url;

pub const DEFAULT_MEMORY_LIMIT_MIB: u64 = 512;
pub const MIN_MEMORY_LIMIT_MIB: u64 = 64;
pub const MAX_MEMORY_LIMIT_MIB: u64 = 4096;
pub const MAX_CONCURRENCY: u32 = 64;
const CONFIG_FILE: &str = "config.toml";
const SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub home: PathBuf,
    pub server: Server,
    pub updates: Updates,
    pub transfers: Transfers,
    pub webrtc: WebRtc,
    pub appearance: Appearance,
    pub cli: Cli,
    pub desktop: Desktop,
    baseline: Option<Settings>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Settings {
    server: Server,
    updates: Updates,
    transfers: Transfers,
    webrtc: WebRtc,
    appearance: Appearance,
    cli: Cli,
    desktop: Desktop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Updates {
    pub auto_update: bool,
    pub channel: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transfers {
    pub memory_limit_mib: u64,
    pub max_concurrency: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebRtc {
    pub relay_only: bool,
    pub ice_source: IceSource,
    pub ice_servers: Vec<IceServer>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IceSource {
    #[default]
    Server,
    Merge,
    Replace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IceServer {
    pub url: String,
    pub credential_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Appearance {
    pub theme: Theme,
    pub reduced_motion: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cli {
    pub no_color: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Desktop {
    pub notifications: bool,
    pub background_transfers: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            home: PathBuf::new(),
            server: Server {
                url: "https://filebeam.io".into(),
            },
            updates: Updates {
                auto_update: true,
                channel: "stable".into(),
            },
            transfers: Transfers {
                memory_limit_mib: DEFAULT_MEMORY_LIMIT_MIB,
                max_concurrency: None,
            },
            webrtc: WebRtc {
                relay_only: false,
                ice_source: IceSource::Server,
                ice_servers: Vec::new(),
            },
            appearance: Appearance {
                theme: Theme::System,
                reduced_motion: false,
            },
            cli: Cli::default(),
            desktop: Desktop {
                notifications: false,
                background_transfers: true,
            },
            baseline: None,
        }
    }
}

impl Config {
    pub fn load(home: Option<PathBuf>) -> Result<Self> {
        let legacy_home = home
            .is_none()
            .then(|| std::env::var_os("FILEBEAM_HOME"))
            .flatten()
            .map(PathBuf::from);
        Self::load_with_legacy_settings(home, std::env::var_os("FILEBEAM_INSTANCE"), legacy_home)
    }

    fn load_with_legacy_settings(
        home: Option<PathBuf>,
        legacy_instance: Option<std::ffi::OsString>,
        legacy_home: Option<PathBuf>,
    ) -> Result<Self> {
        let home = home
            .or_else(|| legacy_home.map(|path| path.join(".filebeam")))
            .unwrap_or_else(default_home);
        ensure_home(&home)?;
        let path = home.join(CONFIG_FILE);
        let lock = lock_file(&home)?;
        lock.lock_exclusive().context("lock config.toml")?;
        let result = (|| {
            if !path.exists() {
                let mut config = Self {
                    home,
                    ..Self::default()
                };
                // Legacy environment values are imported once into a newly created file, never read at runtime.
                if let Some(url) = legacy_instance_env(legacy_instance) {
                    config.server.url = normalize_server_url(&url)?;
                }
                config.validate()?;
                let mut document = DocumentMut::new();
                apply_config(&mut document, &config);
                atomic_write(&path, document.to_string().as_bytes())?;
                return Ok(config);
            }
            let source =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let mut document = source.parse::<DocumentMut>().context("parse config.toml")?;
            let (mut config, migrated) = from_document(&document, home)?;
            import_legacy_instance(&mut config, &document, legacy_instance)?;
            config.validate()?;
            if migrated {
                apply_config(&mut document, &config);
                atomic_write(&path, document.to_string().as_bytes())?;
            }
            Ok(config)
        })();
        let _ = FileExt::unlock(&lock);
        result
    }

    pub fn save(&self) -> Result<()> {
        self.validate()?;
        ensure_home(&self.home)?;
        let path = self.home.join(CONFIG_FILE);
        let lock = lock_file(&self.home)?;
        lock.lock_exclusive().context("lock config.toml")?;
        let result = (|| {
            let document = if path.exists() {
                fs::read_to_string(&path)
                    .with_context(|| format!("read {}", path.display()))?
                    .parse::<DocumentMut>()
                    .context("parse config.toml")?
            } else {
                DocumentMut::new()
            };
            let mut document = document;
            // Reject malformed existing documents before indexing tables during serialization.
            let (mut current, _) = from_document(&document, self.home.clone())?;
            if let Some(baseline) = &self.baseline {
                current.apply_changes_from(self, baseline);
            } else {
                current = self.clone();
            }
            apply_config(&mut document, &current);
            atomic_write(&path, document.to_string().as_bytes())
        })();
        let _ = FileExt::unlock(&lock);
        result
    }

    /// Applies a small change while holding the configuration lock, so independent writers do not
    /// replace each other's fields with stale in-memory documents.
    pub fn update(home: Option<PathBuf>, change: impl FnOnce(&mut Self)) -> Result<Self> {
        let home = home.unwrap_or_else(default_home);
        ensure_home(&home)?;
        let path = home.join(CONFIG_FILE);
        let lock = lock_file(&home)?;
        lock.lock_exclusive().context("lock config.toml")?;
        let result = (|| {
            let document = if path.exists() {
                fs::read_to_string(&path)?
                    .parse::<DocumentMut>()
                    .context("parse config.toml")?
            } else {
                DocumentMut::new()
            };
            let (mut config, _) = from_document(&document, home)?;
            change(&mut config);
            config.validate()?;
            let mut document = document;
            apply_config(&mut document, &config);
            atomic_write(&path, document.to_string().as_bytes())?;
            Ok(config)
        })();
        let _ = FileExt::unlock(&lock);
        result
    }

    pub fn validate(&self) -> Result<()> {
        validate_server_url(&self.server.url)?;
        if self.updates.channel != "stable" {
            bail!("updates.channel must be stable");
        }
        if self
            .transfers
            .max_concurrency
            .is_some_and(|value| value == 0 || value > MAX_CONCURRENCY)
        {
            bail!("transfers.max_concurrency must be between 1 and {MAX_CONCURRENCY}");
        }
        if !(MIN_MEMORY_LIMIT_MIB..=MAX_MEMORY_LIMIT_MIB).contains(&self.transfers.memory_limit_mib)
        {
            bail!(
                "transfers.memory_limit_mib must be between {MIN_MEMORY_LIMIT_MIB} and {MAX_MEMORY_LIMIT_MIB}"
            );
        }
        for server in &self.webrtc.ice_servers {
            server.validate()?;
        }
        Ok(())
    }

    pub fn ice_policy(&self) -> IcePolicy<'_> {
        IcePolicy {
            source: self.webrtc.ice_source,
            servers: &self.webrtc.ice_servers,
        }
    }

    /// Converts persisted references into a runtime policy. A native host must
    /// attach its secure credential resolver before configured TURN is used.
    pub fn native_ice_override(&self) -> filebeam_transfer_native::ice::IceOverride {
        use filebeam_transfer_native::ice::{
            ConfiguredIceServer, IceOverride, IceSource as NativeIceSource,
        };
        let source = match self.webrtc.ice_source {
            IceSource::Server => NativeIceSource::Server,
            IceSource::Merge => NativeIceSource::Merge,
            IceSource::Replace => NativeIceSource::Replace,
        };
        IceOverride::new(
            source,
            self.webrtc
                .ice_servers
                .iter()
                .map(|server| ConfiguredIceServer {
                    url: server.url.clone(),
                    credential_ref: server.credential_ref.clone(),
                })
                .collect(),
        )
    }

    fn settings(&self) -> Settings {
        Settings {
            server: self.server.clone(),
            updates: self.updates.clone(),
            transfers: self.transfers.clone(),
            webrtc: self.webrtc.clone(),
            appearance: self.appearance.clone(),
            cli: self.cli.clone(),
            desktop: self.desktop.clone(),
        }
    }

    fn apply_changes_from(&mut self, changed: &Self, baseline: &Settings) {
        if changed.server != baseline.server {
            self.server = changed.server.clone();
        }
        if changed.updates != baseline.updates {
            self.updates = changed.updates.clone();
        }
        if changed.transfers.memory_limit_mib != baseline.transfers.memory_limit_mib {
            self.transfers.memory_limit_mib = changed.transfers.memory_limit_mib;
        }
        if changed.transfers.max_concurrency != baseline.transfers.max_concurrency {
            self.transfers.max_concurrency = changed.transfers.max_concurrency;
        }
        if changed.webrtc != baseline.webrtc {
            self.webrtc = changed.webrtc.clone();
        }
        if changed.appearance != baseline.appearance {
            self.appearance = changed.appearance.clone();
        }
        if changed.cli != baseline.cli {
            self.cli = changed.cli.clone();
        }
        if changed.desktop != baseline.desktop {
            self.desktop = changed.desktop.clone();
        }
    }
}

impl IceServer {
    pub fn validate(&self) -> Result<()> {
        let (scheme, authority) = self
            .url
            .split_once(':')
            .context("webrtc ICE server URL is invalid")?;
        if !matches!(scheme, "stun" | "stuns" | "turn" | "turns")
            || authority
                .trim_start_matches('/')
                .split(['/', '?', ':'])
                .next()
                .is_none_or(str::is_empty)
        {
            bail!("webrtc ICE server URL must use stun, stuns, turn, or turns with a host");
        }
        if self.credential_ref.as_deref().is_some_and(|reference| {
            reference.trim().is_empty() || reference.contains(['\r', '\n'])
        }) {
            bail!("webrtc ICE credential_ref must not be empty");
        }
        if matches!(scheme, "turn" | "turns") && self.credential_ref.is_none() {
            bail!("TURN ICE servers require credential_ref");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IcePolicy<'a> {
    pub source: IceSource,
    pub servers: &'a [IceServer],
}

pub fn default_home() -> PathBuf {
    // FILEBEAM_HOME historically names the parent of .filebeam. Keep that
    // location so upgrades retain sessions, keys, and transfer checkpoints.
    std::env::var_os("FILEBEAM_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .map(|path| path.join(".filebeam"))
        .unwrap_or_else(|| PathBuf::from(".filebeam"))
}

fn from_document(document: &DocumentMut, home: PathBuf) -> Result<(Config, bool)> {
    let mut config = Config {
        home,
        ..Config::default()
    };
    let schema = match document.get("schema_version") {
        Some(item) => Some(
            item.as_integer()
                .context("schema_version must be an integer")?,
        ),
        None => None,
    };
    if schema.is_some_and(|version| version != SCHEMA_VERSION) {
        bail!("unsupported config schema_version");
    }
    let legacy = schema.is_none();
    let mut migrated = legacy;
    macro_rules! legacy {
        ($name:literal, $target:expr) => {
            if let Some(item) = document.get($name) {
                $target = item
                    .as_value()
                    .and_then(|v| v.as_bool())
                    .context(concat!("invalid ", $name))?;
                migrated = true;
            }
        };
    }
    // Read legacy values first. Nested values win after migration while legacy keys stay in place
    // to retain their attached comments exactly.
    if legacy {
        legacy!("no_color", config.cli.no_color);
        legacy!("reduced_motion", config.appearance.reduced_motion);
        legacy!("webrtc_relay_only", config.webrtc.relay_only);
        if let Some(value) = document.get("check_updates") {
            config.updates.auto_update = value
                .as_value()
                .and_then(|v| v.as_bool())
                .context("invalid check_updates")?;
        }
        if let Some(value) = document.get("max_concurrency") {
            config.transfers.max_concurrency = Some(
                u32::try_from(
                    value
                        .as_value()
                        .and_then(|v| v.as_integer())
                        .context("invalid max_concurrency")?,
                )
                .context("max_concurrency is out of range")?,
            );
        }
        if let Some(value) = document.get("memory_limit_mib") {
            config.transfers.memory_limit_mib = u64::try_from(
                value
                    .as_value()
                    .and_then(|v| v.as_integer())
                    .context("invalid memory_limit_mib")?,
            )
            .context("memory_limit_mib is out of range")?;
        }
    }
    if let Some(value) = string_at(document, "server", "url")? {
        config.server.url = normalize_server_url(&value)?;
    }
    if let Some(value) = bool_at(document, "updates", "auto_update")? {
        config.updates.auto_update = value;
    }
    if let Some(value) = string_at(document, "updates", "channel")? {
        config.updates.channel = value;
    }
    if let Some(value) = integer_at(document, "transfers", "memory_limit_mib")? {
        config.transfers.memory_limit_mib =
            u64::try_from(value).context("transfers.memory_limit_mib is out of range")?;
    }
    if let Some(value) = integer_at(document, "transfers", "max_concurrency")? {
        config.transfers.max_concurrency =
            Some(u32::try_from(value).context("transfers.max_concurrency is out of range")?);
    }
    if let Some(value) = bool_at(document, "webrtc", "relay_only")? {
        config.webrtc.relay_only = value;
    }
    if let Some(value) = string_at(document, "webrtc", "ice_source")? {
        config.webrtc.ice_source = parse_ice_source(&value)?;
    }
    if let Some(value) = bool_at(document, "appearance", "reduced_motion")? {
        config.appearance.reduced_motion = value;
    }
    if let Some(value) = string_at(document, "appearance", "theme")? {
        config.appearance.theme = parse_theme(&value)?;
    }
    if let Some(value) = bool_at(document, "cli", "no_color")? {
        config.cli.no_color = value;
    }
    if let Some(value) = bool_at(document, "desktop", "notifications")? {
        config.desktop.notifications = value;
    }
    if let Some(value) = bool_at(document, "desktop", "background_transfers")? {
        config.desktop.background_transfers = value;
    }
    if let Some(item) = table_at(document, "webrtc")?.and_then(|table| table.get("ice_servers")) {
        let array = item
            .as_array_of_tables()
            .context("webrtc.ice_servers must be an array of tables")?;
        config.webrtc.ice_servers = array
            .iter()
            .map(|table| {
                if table.contains_key("username")
                    || table.contains_key("credential")
                    || table.contains_key("urls")
                {
                    bail!("webrtc ICE credentials must use credential_ref only");
                }
                Ok(IceServer {
                    url: table
                        .get("url")
                        .and_then(Item::as_str)
                        .context("webrtc.ice_servers.url is required")?
                        .to_owned(),
                    credential_ref: match table.get("credential_ref") {
                        None => None,
                        Some(item) => Some(
                            item.as_str()
                                .context("webrtc.ice_servers.credential_ref must be a string")?
                                .to_owned(),
                        ),
                    },
                })
            })
            .collect::<Result<_>>()?;
    }
    config.baseline = Some(config.settings());
    Ok((config, migrated))
}

fn apply_config(document: &mut DocumentMut, config: &Config) {
    document["schema_version"] = value(SCHEMA_VERSION);
    document["server"].or_insert(toml_edit::table());
    document["server"]["url"] = value(&config.server.url);
    document["updates"].or_insert(toml_edit::table());
    document["updates"]["auto_update"] = value(config.updates.auto_update);
    document["updates"]["channel"] = value(&config.updates.channel);
    document["transfers"].or_insert(toml_edit::table());
    document["transfers"]["memory_limit_mib"] = value(config.transfers.memory_limit_mib as i64);
    if let Some(max) = config.transfers.max_concurrency {
        document["transfers"]["max_concurrency"] = value(max as i64);
    } else {
        document["transfers"]
            .as_table_mut()
            .expect("transfers is a table")
            .remove("max_concurrency");
    }
    document["webrtc"].or_insert(toml_edit::table());
    document["webrtc"]["relay_only"] = value(config.webrtc.relay_only);
    document["webrtc"]["ice_source"] = value(ice_source_name(config.webrtc.ice_source));
    let mut servers = toml_edit::ArrayOfTables::new();
    for ice in &config.webrtc.ice_servers {
        let mut table = toml_edit::Table::new();
        table["url"] = value(&ice.url);
        if let Some(reference) = &ice.credential_ref {
            table["credential_ref"] = value(reference);
        }
        servers.push(table);
    }
    document["webrtc"]["ice_servers"] = Item::ArrayOfTables(servers);
    document["appearance"].or_insert(toml_edit::table());
    document["appearance"]["theme"] = value(theme_name(config.appearance.theme));
    document["appearance"]["reduced_motion"] = value(config.appearance.reduced_motion);
    document["cli"].or_insert(toml_edit::table());
    document["cli"]["no_color"] = value(config.cli.no_color);
    document["desktop"].or_insert(toml_edit::table());
    document["desktop"]["notifications"] = value(config.desktop.notifications);
    document["desktop"]["background_transfers"] = value(config.desktop.background_transfers);
}

fn table_at<'a>(document: &'a DocumentMut, name: &str) -> Result<Option<&'a toml_edit::Table>> {
    match document.get(name) {
        None => Ok(None),
        Some(item) => item
            .as_table()
            .map(Some)
            .context(format!("{name} must be a table")),
    }
}
fn string_at(document: &DocumentMut, table: &str, key: &str) -> Result<Option<String>> {
    table_at(document, table)?
        .and_then(|t| t.get(key))
        .map(|i| {
            i.as_str()
                .map(str::to_owned)
                .context(format!("invalid {table}.{key}"))
        })
        .transpose()
}
fn bool_at(document: &DocumentMut, table: &str, key: &str) -> Result<Option<bool>> {
    table_at(document, table)?
        .and_then(|t| t.get(key))
        .map(|i| {
            i.as_value()
                .and_then(|v| v.as_bool())
                .context(format!("invalid {table}.{key}"))
        })
        .transpose()
}
fn integer_at(document: &DocumentMut, table: &str, key: &str) -> Result<Option<i64>> {
    table_at(document, table)?
        .and_then(|t| t.get(key))
        .map(|i| {
            i.as_value()
                .and_then(|v| v.as_integer())
                .context(format!("invalid {table}.{key}"))
        })
        .transpose()
}
fn parse_ice_source(value: &str) -> Result<IceSource> {
    match value {
        "server" => Ok(IceSource::Server),
        "merge" => Ok(IceSource::Merge),
        "replace" => Ok(IceSource::Replace),
        _ => bail!("webrtc.ice_source must be server, merge, or replace"),
    }
}
fn ice_source_name(value: IceSource) -> &'static str {
    match value {
        IceSource::Server => "server",
        IceSource::Merge => "merge",
        IceSource::Replace => "replace",
    }
}
fn parse_theme(value: &str) -> Result<Theme> {
    match value {
        "system" => Ok(Theme::System),
        "light" => Ok(Theme::Light),
        "dark" => Ok(Theme::Dark),
        _ => bail!("appearance.theme must be system, light, or dark"),
    }
}
fn theme_name(value: Theme) -> &'static str {
    match value {
        Theme::System => "system",
        Theme::Light => "light",
        Theme::Dark => "dark",
    }
}
pub fn normalize_server_url(value: &str) -> Result<String> {
    let mut url = Url::parse(value).context("server.url is invalid")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("server.url must be an HTTP(S) origin without credentials, query, or fragment");
    }
    url.set_path("");
    Ok(url.to_string().trim_end_matches('/').to_owned())
}
fn validate_server_url(value: &str) -> Result<()> {
    normalize_server_url(value).map(|_| ())
}
fn legacy_instance_env(value: Option<std::ffi::OsString>) -> Option<String> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string_lossy().into_owned())
}

fn import_legacy_instance(
    config: &mut Config,
    document: &DocumentMut,
    instance: Option<std::ffi::OsString>,
) -> Result<()> {
    if document.get("schema_version").is_none()
        && string_at(document, "server", "url")?.is_none()
        && let Some(instance) = legacy_instance_env(instance)
    {
        config.server.url = normalize_server_url(&instance)?;
        config.baseline = Some(config.settings());
    }
    Ok(())
}

fn ensure_home(home: &Path) -> Result<()> {
    fs::create_dir_all(home).with_context(|| format!("create {}", home.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(home, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("secure {}", home.display()))?;
    }
    Ok(())
}
fn lock_file(home: &Path) -> Result<File> {
    let path = home.join("config.toml.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("config path has no parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, thread};

    #[test]
    fn migrates_flat_config_and_preserves_unknown_content() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join(CONFIG_FILE),
            "# keep\ncheck_updates = false\nmemory_limit_mib = 768\ncustom = 4\n",
        )
        .unwrap();
        let config = Config::load(Some(temp.path().to_owned())).unwrap();
        assert!(!config.updates.auto_update);
        assert_eq!(config.transfers.memory_limit_mib, 768);
        let written = fs::read_to_string(temp.path().join(CONFIG_FILE)).unwrap();
        assert!(
            written.contains("# keep")
                && written.contains("custom = 4")
                && written.contains("auto_update = false"),
            "{written}"
        );
    }

    #[test]
    fn rejects_malformed_toml_and_invalid_ice_policy() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join(CONFIG_FILE), "[server\nurl = 'x'").unwrap();
        assert!(Config::load(Some(temp.path().to_owned())).is_err());
        let mut config = Config {
            home: temp.path().to_owned(),
            ..Config::default()
        };
        config.webrtc.ice_servers.push(IceServer {
            url: "https://not-ice.example".into(),
            credential_ref: None,
        });
        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_standard_opaque_ice_urls_and_requires_turn_credentials() {
        for url in [
            "stun:stun.example:3478",
            "stuns:stun.example",
            "turn:turn.example:3478?transport=udp",
            "turns:turn.example",
        ] {
            let credential_ref = url.starts_with("turn").then(|| "keyring:turn".into());
            IceServer {
                url: url.into(),
                credential_ref,
            }
            .validate()
            .unwrap();
        }
        assert!(
            IceServer {
                url: "turn:turn.example".into(),
                credential_ref: None
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn rejects_wrong_tables_overflow_and_unsupported_schema() {
        let temp = tempfile::tempdir().unwrap();
        for source in [
            "schema_version = 1\nserver = 'wrong'\n",
            "schema_version = 1\n[transfers]\nmax_concurrency = -1\n",
            "schema_version = 1\n[transfers]\nmemory_limit_mib = 9223372036854775807\n",
            "schema_version = 2\n",
        ] {
            fs::write(temp.path().join(CONFIG_FILE), source).unwrap();
            assert!(
                Config::load(Some(temp.path().to_owned())).is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn migration_is_one_time_and_nested_opt_out_wins() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(CONFIG_FILE);
        fs::write(
            &path,
            "# legacy opt out\ncheck_updates = true\n[updates]\nauto_update = false\n",
        )
        .unwrap();
        assert!(
            !Config::load(Some(temp.path().to_owned()))
                .unwrap()
                .updates
                .auto_update
        );
        let first = fs::read_to_string(&path).unwrap();
        assert!(first.contains("schema_version = 1") && first.contains("# legacy opt out"));
        Config::load(Some(temp.path().to_owned())).unwrap();
        assert_eq!(first, fs::read_to_string(path).unwrap());
    }

    #[test]
    fn concurrent_updates_do_not_overwrite_unrelated_fields() {
        let temp = Arc::new(tempfile::tempdir().unwrap());
        let home = temp.path().to_owned();
        Config::load(Some(home.clone())).unwrap();
        let first_home = home.clone();
        let second_home = home.clone();
        let a = thread::spawn(move || {
            Config::update(Some(first_home), |config| config.cli.no_color = true)
        });
        let b = thread::spawn(move || {
            Config::update(Some(second_home), |config| {
                config.desktop.notifications = true
            })
        });
        a.join().unwrap().unwrap();
        b.join().unwrap().unwrap();
        let written = fs::read_to_string(home.join(CONFIG_FILE)).unwrap();
        assert!(written.contains("no_color = true") && written.contains("notifications = true"));
    }

    #[test]
    fn save_merges_only_fields_changed_since_load() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().to_owned();
        let mut first = Config::load(Some(home.clone())).unwrap();
        let mut second = Config::load(Some(home.clone())).unwrap();
        first.cli.no_color = true;
        second.desktop.notifications = true;
        first.save().unwrap();
        second.save().unwrap();
        let config = Config::load(Some(home)).unwrap();
        assert!(config.cli.no_color && config.desktop.notifications);
    }

    #[test]
    fn concurrent_first_loads_create_one_valid_document() {
        let temp = Arc::new(tempfile::tempdir().unwrap());
        let first = temp.path().to_owned();
        let second = temp.path().to_owned();
        let a = thread::spawn(move || {
            Config::load_with_legacy_settings(
                Some(first),
                Some("https://first.example".into()),
                None,
            )
        });
        let b = thread::spawn(move || {
            Config::load_with_legacy_settings(
                Some(second),
                Some("https://second.example".into()),
                None,
            )
        });
        a.join().unwrap().unwrap();
        b.join().unwrap().unwrap();
        let config = Config::load(Some(temp.path().to_owned())).unwrap();
        assert!(matches!(
            config.server.url.as_str(),
            "https://first.example" | "https://second.example"
        ));
    }

    #[test]
    fn imports_legacy_instance_only_when_creating_config() {
        let temp = tempfile::tempdir().unwrap();
        let config = Config::load_with_legacy_settings(
            Some(temp.path().to_owned()),
            Some("https://legacy.example".into()),
            None,
        )
        .unwrap();
        assert_eq!(config.server.url, "https://legacy.example");
        let config = Config::load_with_legacy_settings(
            Some(temp.path().to_owned()),
            Some("https://ignored.example".into()),
            None,
        )
        .unwrap();
        assert_eq!(config.server.url, "https://legacy.example");
    }

    #[test]
    fn migrating_existing_flat_settings_keeps_the_legacy_instance_and_opt_out() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join(CONFIG_FILE),
            "check_updates = false\ncustom = 'keep'\n",
        )
        .unwrap();
        let config = Config::load_with_legacy_settings(
            Some(temp.path().to_owned()),
            Some("https://existing.example".into()),
            None,
        )
        .unwrap();
        assert_eq!(config.server.url, "https://existing.example");
        assert!(!config.updates.auto_update);
        let config = Config::load_with_legacy_settings(
            Some(temp.path().to_owned()),
            Some("https://ignored.example".into()),
            None,
        )
        .unwrap();
        assert_eq!(config.server.url, "https://existing.example");
        assert!(
            fs::read_to_string(temp.path().join(CONFIG_FILE))
                .unwrap()
                .contains("custom = 'keep'")
        );
    }

    #[test]
    fn explicit_home_ignores_legacy_home() {
        let legacy = tempfile::tempdir().unwrap();
        fs::write(
            legacy.path().join(CONFIG_FILE),
            "server = { url = 'https://legacy.example' }",
        )
        .unwrap();
        let explicit = tempfile::tempdir().unwrap();
        let config = Config::load_with_legacy_settings(
            Some(explicit.path().to_owned()),
            None,
            Some(legacy.path().to_owned()),
        )
        .unwrap();
        assert_eq!(config.server.url, "https://filebeam.io");
    }

    #[test]
    fn legacy_home_is_migrated_in_place_with_private_state_retained() {
        let parent = tempfile::tempdir().unwrap();
        let home = parent.path().join(".filebeam");
        fs::create_dir(&home).unwrap();
        fs::write(home.join(CONFIG_FILE), "check_updates = false\n").unwrap();
        fs::write(home.join("private-state"), b"retained").unwrap();
        let config = Config::load_with_legacy_settings(
            None,
            Some("https://legacy.example".into()),
            Some(parent.path().to_owned()),
        )
        .unwrap();
        assert_eq!(config.home, home);
        assert_eq!(config.server.url, "https://legacy.example");
        assert!(!config.updates.auto_update);
        assert_eq!(fs::read(home.join("private-state")).unwrap(), b"retained");
    }
}
