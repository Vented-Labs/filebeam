//! One place for every human-facing number, time and error message.

use std::time::{SystemTime, UNIX_EPOCH};

/// Sizes in binary units with one decimal where it helps, e.g. "2.0 GiB".
pub fn bytes(value: u64) -> String {
    if value < 1024 {
        return format!("{value} B");
    }
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    let mut amount = value as f64 / 1024.;
    let mut unit = 0;
    while amount >= 1024. && unit < UNITS.len() - 1 {
        amount /= 1024.;
        unit += 1;
    }
    if amount >= 100. {
        format!("{amount:.0} {}", UNITS[unit])
    } else {
        format!("{amount:.1} {}", UNITS[unit])
    }
}

pub fn plural(count: u64, singular: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {singular}s")
    }
}

/// A count with an irregular plural, e.g. "2 deliveries".
pub fn count(count: u64, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

/// "1 hour", "3 days", "1 week": retention reads as a duration, never as raw hours.
pub fn retention(hours: u64) -> String {
    if hours > 0 && hours.is_multiple_of(8760) {
        plural(hours / 8760, "year")
    } else if hours > 0 && hours.is_multiple_of(24 * 7) {
        plural(hours / (24 * 7), "week")
    } else if hours > 0 && hours.is_multiple_of(24) {
        plural(hours / 24, "day")
    } else {
        plural(hours, "hour")
    }
}

/// Days since the Unix epoch for a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Parses the server's RFC 3339 timestamps into Unix seconds; `None` when absent or malformed.
pub fn parse_instant(value: &str) -> Option<i64> {
    let value = value.trim();
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't' | b' ')
    {
        return None;
    }
    let number = |range: std::ops::Range<usize>| value.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut rest = &value[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        rest = &fraction[digits..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let rest = &rest[1..];
            if rest.len() != 5 || rest.as_bytes()[2] != b':' {
                return None;
            }
            sign * (rest[0..2].parse::<i64>().ok()? * 3600 + rest[3..5].parse::<i64>().ok()? * 60)
        }
    };
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// "in 3 hours", "2 days ago", "moments ago".
pub fn relative_to(instant: i64, now: i64) -> String {
    let seconds = instant - now;
    let future = seconds > 0;
    let magnitude = seconds.unsigned_abs();
    let amount = match magnitude {
        0..60 => {
            return if future {
                "in under a minute".into()
            } else {
                "moments ago".into()
            };
        }
        60..3600 => plural(magnitude / 60, "minute"),
        3600..86_400 => plural(magnitude / 3600, "hour"),
        _ => plural(magnitude / 86_400, "day"),
    };
    if future {
        format!("in {amount}")
    } else {
        format!("{amount} ago")
    }
}

/// A server timestamp as relative time, or the raw value when it cannot be parsed.
pub fn relative(value: &str) -> String {
    parse_instant(value).map_or_else(|| value.to_owned(), |instant| relative_to(instant, now()))
}

/// An expiry as a label and a time: ("Expires", "in 3 days") until it passes, then
/// ("Expired", "2 days ago").
pub fn expiry(value: &str) -> (&'static str, String) {
    let now = now();
    match parse_instant(value) {
        Some(instant) if instant <= now => ("Expired", relative_to(instant, now)),
        Some(instant) => ("Expires", relative_to(instant, now)),
        None => ("Expires", value.to_owned()),
    }
}

/// "Sep 18, 2026, 10:24 UTC". The pinned toolchain has no time-zone database, so
/// absolute times are labelled as UTC rather than guessed as local.
pub fn absolute(value: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let Some(instant) = parse_instant(value) else {
        return value.to_owned();
    };
    let (year, month, day) = civil_from_days(instant.div_euclid(86_400));
    let seconds = instant.rem_euclid(86_400);
    format!(
        "{} {day}, {year}, {:02}:{:02} UTC",
        MONTHS[(month - 1) as usize],
        seconds / 3600,
        seconds % 3600 / 60
    )
}

/// The HTTP status a failed request ended with, when the failure names one.
pub fn status_code(raw: &str) -> Option<u16> {
    let lowered = raw.to_lowercase();
    lowered.find("returned ").and_then(|index| {
        lowered[index + 9..]
            .get(..3)
            .and_then(|digits| digits.parse::<u16>().ok())
    })
}

/// Turns a failure from the transfer core into a sentence a person can act on. Core
/// errors can end in an HTTP status or a transport phrase; those never reach the
/// screen raw.
pub fn readable_error(raw: &str) -> String {
    let mut text = raw.trim();
    if let Some(detail) = text.strip_prefix("detail=") {
        text = detail.trim();
    }
    if text.is_empty() {
        return "Something went wrong. Try again.".into();
    }
    let lowered = text.to_lowercase();
    match status_code(text) {
        Some(401 | 419) => return "Your session ended. Sign in again.".into(),
        Some(403) => return "This account is not allowed to do that.".into(),
        Some(404) => {
            return "That could not be found. It may have expired or been removed.".into();
        }
        Some(422) => {
            return "The server did not accept that. Check what you entered and try again.".into();
        }
        Some(429) => return "Too many attempts. Wait a moment and try again.".into(),
        Some(500..) => return "The server had a problem. Try again shortly.".into(),
        _ => {}
    }
    // The service withholds any failure that could quote a secret.
    if lowered.contains("sensitive detail was withheld") {
        return "That did not work. Check what you entered and try again.".into();
    }
    if lowered.contains("error sending request")
        || lowered.contains("dns")
        || lowered.contains("connection refused")
        || lowered.contains("could not connect")
    {
        return "The instance could not be reached. Check your connection.".into();
    }
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + characters.as_str()
    })
}

/// A short, readable title for a failed operation, e.g. "Could not sign in".
pub fn failure_title(operation: &str) -> String {
    match operation {
        "login" => "Could not sign in".into(),
        "logout" => "Could not sign out".into(),
        "register" => "Could not create the account".into(),
        "share" => "Could not share".into(),
        "note" => "Could not create the note".into(),
        "receive" | "inspect receive" => "Could not open the transfer".into(),
        "revoke link" => "Could not revoke the link".into(),
        "refresh" => "Could not refresh".into(),
        "end live session" => "Could not end the live session".into(),
        "command" => "That did not work".into(),
        other => {
            let mut characters = other.chars();
            let name = characters.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + characters.as_str()
            });
            format!("{name} failed")
        }
    }
}

/// A long identifier reduced to its ends, for places that only need to tell two apart.
pub fn abbreviated(value: &str, head: usize, tail: usize) -> String {
    let characters: Vec<char> = value.chars().collect();
    if characters.len() <= head + tail + 1 {
        return value.to_owned();
    }
    let start: String = characters[..head].iter().collect();
    let end: String = characters[characters.len() - tail..].iter().collect();
    format!("{start}\u{2026}{end}")
}

/// The host of an instance URL, or the URL itself when it has none.
pub fn host(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed.host_str().map(|host| match parsed.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_owned(),
            })
        })
        .unwrap_or_else(|| url.to_owned())
}

/// The last path component as a display name.
pub fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map_or_else(|| path.display().to_string(), str::to_owned)
}

/// An approved icon name for a file, chosen from its extension.
pub fn file_icon(name: &str) -> &'static str {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" => "archive",
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "m4v" => "video",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus" => "music",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "svg" | "heic" | "bmp" | "tiff" => {
            "image"
        }
        "txt" | "md" | "note" | "rtf" | "log" => "note",
        "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "php" | "go" | "c" | "h" | "cpp" | "java"
        | "kt" | "swift" | "rb" | "sh" | "json" | "toml" | "yaml" | "yml" | "html" | "css"
        | "xml" | "sql" => "code",
        _ => "file",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_sizes_use_binary_units() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(2_147_483_648), "2.0 GiB");
        assert_eq!(bytes(482_344_960), "460 MiB");
        assert_eq!(bytes(3_758_080), "3.6 MiB");
    }

    #[test]
    fn retention_reads_as_a_duration() {
        assert_eq!(retention(1), "1 hour");
        assert_eq!(retention(6), "6 hours");
        assert_eq!(retention(24), "1 day");
        assert_eq!(retention(72), "3 days");
        assert_eq!(retention(168), "1 week");
        assert_eq!(retention(8760), "1 year");
    }

    #[test]
    fn rfc3339_instants_parse_with_fractions_and_offsets() {
        assert_eq!(parse_instant("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_instant("2026-09-18T10:24:00Z"), Some(1_789_727_040));
        assert_eq!(
            parse_instant("2026-09-18T10:24:00.123456Z"),
            Some(1_789_727_040)
        );
        assert_eq!(
            parse_instant("2026-09-18T12:24:00+02:00"),
            Some(1_789_727_040)
        );
        assert_eq!(parse_instant("2000-02-29T00:00:00Z"), Some(951_782_400));
        assert_eq!(parse_instant("tomorrow"), None);
        assert_eq!(parse_instant("2026-13-01T00:00:00Z"), None);
        assert_eq!(parse_instant("2026-09-18T10:24:00"), None);
    }

    #[test]
    fn relative_times_read_naturally_in_both_directions() {
        let now = 1_000_000;
        assert_eq!(relative_to(now - 5, now), "moments ago");
        assert_eq!(relative_to(now + 5, now), "in under a minute");
        assert_eq!(relative_to(now - 120, now), "2 minutes ago");
        assert_eq!(relative_to(now + 3 * 3600, now), "in 3 hours");
        assert_eq!(relative_to(now - 86_400, now), "1 day ago");
        assert_eq!(relative_to(now + 7 * 86_400, now), "in 7 days");
    }

    #[test]
    fn absolute_times_are_labelled_utc_and_unparsed_values_pass_through() {
        assert_eq!(absolute("2026-09-18T10:24:00Z"), "Sep 18, 2026, 10:24 UTC");
        assert_eq!(absolute("2024-02-29T23:59:59Z"), "Feb 29, 2024, 23:59 UTC");
        assert_eq!(absolute("unavailable"), "unavailable");
        assert_eq!(relative("not a date"), "not a date");
    }

    #[test]
    fn core_failures_become_actionable_sentences() {
        assert_eq!(
            readable_error("The server returned 404."),
            "That could not be found. It may have expired or been removed."
        );
        assert_eq!(
            readable_error("request returned 503"),
            "The server had a problem. Try again shortly."
        );
        assert_eq!(
            readable_error("detail=error sending request for url"),
            "The instance could not be reached. Check your connection."
        );
        assert_eq!(
            readable_error("this transfer has expired."),
            "This transfer has expired."
        );
        assert_eq!(readable_error("  "), "Something went wrong. Try again.");
        assert_eq!(failure_title("login"), "Could not sign in");
        assert_eq!(
            status_code("native login returned 422 Unprocessable"),
            Some(422)
        );
        assert_eq!(status_code("connection reset"), None);
        assert_eq!(count(1, "delivery", "deliveries"), "1 delivery");
        assert_eq!(count(4, "delivery", "deliveries"), "4 deliveries");
        assert_eq!(expiry("1999-01-01T00:00:00Z").0, "Expired");
        assert_eq!(expiry("2999-01-01T00:00:00Z").0, "Expires");
        assert_eq!(expiry("soon"), ("Expires", "soon".to_owned()));
        assert_eq!(failure_title("revoke link"), "Could not revoke the link");
        assert_eq!(
            abbreviated("01JD8Q4XK7M2N9P3R5T6V8W0YZ", 8, 4),
            "01JD8Q4X\u{2026}W0YZ"
        );
        assert_eq!(abbreviated("short-id", 8, 4), "short-id");
        assert_eq!(failure_title("refresh devices"), "Refresh devices failed");
    }

    #[test]
    fn hosts_and_file_kinds_are_derived_without_io() {
        assert_eq!(host("https://files.example"), "files.example");
        assert_eq!(host("http://localhost:8000"), "localhost:8000");
        assert_eq!(host("not a url"), "not a url");
        assert_eq!(file_icon("Launch assets.ZIP"), "archive");
        assert_eq!(file_icon("walkthrough.mp4"), "video");
        assert_eq!(file_icon("README"), "file");
        assert_eq!(
            file_name(std::path::Path::new("/tmp/report.pdf")),
            "report.pdf"
        );
    }
}
