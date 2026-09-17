use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::Url;

/// Presentation-only share material. This preserves the web link convention;
/// callers decide whether to place the key in the link or share it separately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareLinkPresentation {
    pub link: String,
    pub separate_key: String,
}

/// Formats the exact POSIX-shell command accepted by the current `beam down`
/// parser. It deliberately has no output or transport flags: the CLI defaults
/// are the only portable download invocation.
pub fn format_download_with_cli(target: &str) -> Result<String> {
    if target.is_empty() || target.chars().any(char::is_control) {
        bail!("CLI target must be nonempty and contain no control characters");
    }
    if is_transfer_id(target) {
        return Ok(format!("beam down {}", quote_shell_argument(target)));
    }
    let url = Url::parse(target).context("CLI target must be a full HTTP transfer URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
    {
        bail!("CLI target must be a full HTTP transfer URL without credentials or a query");
    }
    let parsed = filebeam_transfer_native::protocol::parse_link_for_instance(target, target)
        .context("CLI target is not a supported transfer link")?;
    if url.path() != format!("/{}", parsed.id) || !is_transfer_id(&parsed.id) {
        bail!("CLI target is not a supported transfer link");
    }
    Ok(format!("beam down {}", quote_shell_argument(target)))
}

fn quote_shell_argument(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn is_transfer_id(value: &str) -> bool {
    value.len() == 26
        && value.bytes().enumerate().all(|(index, byte)| {
            matches!(byte, b'0'..=b'7') || (index > 0 && matches!(byte, b'0'..=b'9' | b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z' | b'a'..=b'h' | b'j'..=b'k' | b'm'..=b'n' | b'p'..=b't' | b'v'..=b'z'))
        })
}

/// Splits a canonical existing native share link without requiring the caller
/// to separately retain raw key material. The encrypted manifest continues to
/// carry any WebRTC join capability; only the portable `k` fragment is split.
pub fn split_share_link(instance: &str, link: &str) -> Result<ShareLinkPresentation> {
    let parsed = filebeam_transfer_native::protocol::parse_link_for_instance(link, instance)?;
    let separate_key = parsed
        .key
        .as_deref()
        .map(|key| URL_SAFE_NO_PAD.encode(key))
        .map(|key| format!("v1.{key}"))
        .context("share link has no included key")?;
    Ok(ShareLinkPresentation {
        link: format!("{}/{}", parsed.instance, parsed.id),
        separate_key,
    })
}

pub fn present_share_link(
    instance: &str,
    share_url: &str,
    share_key: &str,
    include_key: bool,
) -> Result<ShareLinkPresentation> {
    let raw_key = share_key
        .trim()
        .strip_prefix("v1.")
        .unwrap_or(share_key.trim());
    let key = URL_SAFE_NO_PAD
        .decode(raw_key)
        .context("share key is not base64url")?;
    if key.len() != 32 || URL_SAFE_NO_PAD.encode(&key) != raw_key {
        bail!("share key must be canonical and 32 bytes");
    }
    // This is the same operation as web buildShareLink: resolve against the
    // instance, discard any server fragment, then append the chosen fragment.
    let base = Url::parse(instance).context("configured Filebeam instance is invalid")?;
    let mut link = base
        .join(share_url)
        .context("server returned an invalid share URL")?;
    link.set_fragment(None);
    let separate_key = format!("v1.{raw_key}");
    if include_key {
        link.set_fragment(Some(&format!("k={separate_key}")));
    }
    Ok(ShareLinkPresentation {
        link: link.into(),
        separate_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE";

    #[test]
    fn matches_web_relative_link_format_and_keeps_the_key_separate_on_request() {
        let shown = present_share_link(
            "https://filebeam.test",
            "/01ARZ3NDEKTSV4RRFFQ69G5FAV#ignored",
            KEY,
            false,
        )
        .unwrap();
        assert_eq!(
            shown.link,
            "https://filebeam.test/01ARZ3NDEKTSV4RRFFQ69G5FAV"
        );
        assert_eq!(shown.separate_key, format!("v1.{KEY}"));
        assert_eq!(
            present_share_link(
                "https://filebeam.test",
                "/01ARZ3NDEKTSV4RRFFQ69G5FAV",
                KEY,
                true
            )
            .unwrap()
            .link,
            format!("https://filebeam.test/01ARZ3NDEKTSV4RRFFQ69G5FAV#k=v1.{KEY}")
        );
    }

    #[test]
    fn splits_http_webrtc_and_note_links_with_the_same_portable_fragment() {
        for path in ["/HTTP", "/WEBRTC", "/NOTE"] {
            let source = format!("https://filebeam.test{path}#k=v1.{KEY}");
            // Use valid transfer IDs while retaining labels in the test name.
            let source = source.replace(path, "/01ARZ3NDEKTSV4RRFFQ69G5FAV");
            let split = split_share_link("https://filebeam.test", &source).unwrap();
            assert_eq!(
                split.link,
                "https://filebeam.test/01ARZ3NDEKTSV4RRFFQ69G5FAV"
            );
            assert_eq!(split.separate_key, format!("v1.{KEY}"));
        }
    }

    #[test]
    fn formats_cli_downloads_with_literal_shell_quoting() {
        let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let keyed = format!("https://files.example.test:8443/{id}#k=v1.{KEY}");
        assert_eq!(
            format_download_with_cli(&keyed).unwrap(),
            format!("beam down '{keyed}'")
        );
        assert_eq!(
            format_download_with_cli(&format!("https://files.example.test:8443/{id}")).unwrap(),
            format!("beam down 'https://files.example.test:8443/{id}'")
        );
        assert_eq!(
            format_download_with_cli(id).unwrap(),
            format!("beam down '{id}'")
        );
        assert_eq!(
            quote_shell_argument("$(not-run) it's literal"),
            "'$(not-run) it'\"'\"'s literal'"
        );
    }

    #[test]
    fn cli_formatter_keeps_the_users_key_fragment_choice_and_rejects_unsupported_targets() {
        let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let keyless =
            present_share_link("https://files.example.test", &format!("/{id}"), KEY, false)
                .unwrap();
        let keyed =
            present_share_link("https://files.example.test", &format!("/{id}"), KEY, true).unwrap();
        assert_eq!(
            format_download_with_cli(&keyless.link).unwrap(),
            format!("beam down '{}'", keyless.link)
        );
        assert_eq!(
            format_download_with_cli(&keyed.link).unwrap(),
            format!("beam down '{}'", keyed.link)
        );
        for target in [
            "--help",
            &format!("https://u:p@files.example.test/{id}"),
            &format!("https://files.example.test/{id}?output=/tmp"),
            &format!("https://files.example.test/f/{id}"),
            &format!("https://files.example.test/{id}#k=invalid"),
            &format!("https://files.example.test/{id}\n"),
        ] {
            assert!(format_download_with_cli(target).is_err(), "{target}");
        }
    }
}
