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
}
