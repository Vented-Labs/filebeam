//! Launches production Studio entities against a no-worker fixture client.
use std::{fs, sync::Arc};

use anyhow::{Context, Result};
use filebeam_desktop::{app, client::DesktopClient, visual};

fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let id = arguments.next().unwrap_or_else(|| "send-empty".into());
    let mut theme = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--theme" => {
                let value = arguments.next().context("--theme requires light or dark")?;
                match value.as_str() {
                    "light" | "dark" => theme = Some(value),
                    _ => anyhow::bail!("--theme must be light or dark"),
                }
            }
            _ => anyhow::bail!("unknown visual_studio argument: {argument}"),
        }
    }
    let scenario =
        visual::scenario(&id).with_context(|| format!("unknown visual scenario: {id}"))?;
    let theme = theme.as_deref().unwrap_or(scenario.theme);
    let root = tempfile::tempdir().context("create visual fixture home")?;
    fs::write(
        root.path().join("config.toml"),
        format!(
            "schema_version = 1\n[server]\nurl = \"https://studio.fixture.invalid\"\n[appearance]\ntheme = \"{}\"\n",
            theme
        ),
    )?;
    let (client, _commands) =
        DesktopClient::test_client(visual::snapshot(&id).expect("scenario was checked"));
    eprintln!("visual fixture id={id} theme={theme}");
    app::run_visual_with_client(Arc::new(client), root.keep(), scenario.destination)
        .map_err(anyhow::Error::msg)
}
