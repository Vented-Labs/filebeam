/// A validated, non-secret share-link fragment. Decoding and using the key is
/// deliberately left to the encryption-owning caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareLink {
    pub id: String,
    pub key: Option<String>,
}

pub fn parse_share_link(input: &str) -> Result<ShareLink, String> {
    let mut value = input
        .trim()
        .trim_matches(|c| matches!(c, '\'' | '"' | '`' | '<' | '>' | '[' | ']'));
    if let Some((_, markdown)) = value.rsplit_once("](") {
        value = markdown.trim().trim_end_matches(')').trim();
    }
    value = value.trim_matches(|c| matches!(c, '\'' | '"' | '`' | '<' | '>'));
    let value = value
        .rsplit_once('/')
        .map_or(value, |(_, path)| path)
        .trim_start_matches('/');
    let (id, fragment) = value
        .split_once('#')
        .map_or((value, None), |(id, fragment)| (id, Some(fragment)));
    if id.is_empty() || id.contains('/') || id.contains('?') {
        return Err("transfer link has an invalid path".into());
    }
    if is_uuid(id) {
        return Err("UUIDs are not Filebeam transfer IDs; provide a 26-character ULID".into());
    }
    let id = id.to_ascii_uppercase();
    if !is_ulid(&id) {
        return Err("transfer ID must be a canonical 26-character ULID".into());
    }
    let key = match fragment {
        None => None,
        Some(fragment) => {
            let key = fragment
                .strip_prefix("k=")
                .unwrap_or(fragment)
                .strip_prefix("v1.")
                .ok_or("share key must use v1.<base64url>")?;
            if !valid_base64url(key, 43, 0b11) {
                return Err("share key must decode to 32 bytes".into());
            }
            Some(key.to_owned())
        }
    };
    Ok(ShareLink { id, key })
}

fn valid_base64url(value: &str, length: usize, unused_bits: u8) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        && base64url_value(*value.as_bytes().last().unwrap())
            .is_some_and(|last| last & unused_bits == 0)
}

fn base64url_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn is_ulid(value: &str) -> bool {
    value.len() == 26
        && matches!(value.as_bytes()[0], b'0'..=b'7')
        && value.bytes().skip(1).all(|c| matches!(c, b'0'..=b'9' | b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z'))
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value
            .bytes()
            .enumerate()
            .all(|(i, c)| matches!(i, 8 | 13 | 18 | 23) && c == b'-' || c.is_ascii_hexdigit())
}
