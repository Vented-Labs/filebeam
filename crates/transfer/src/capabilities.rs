use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Limits advertised for one transport. Missing limits mean the server did not
/// advertise a bound, not that another transport's bound applies.
#[derive(Clone, Default, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct DriverLimits {
    pub maximum_transfer_bytes: Option<u64>,
    pub maximum_file_count: Option<usize>,
    #[serde(default)]
    pub maximum_note_bytes: Option<u64>,
}

pub fn select_driver_limits(
    driver: &str,
    advertised: &HashMap<String, DriverLimits>,
    legacy_http_bytes: Option<u64>,
    legacy_http_count: Option<usize>,
) -> DriverLimits {
    advertised.get(driver).cloned().unwrap_or_else(|| {
        if driver == "http" {
            DriverLimits {
                maximum_transfer_bytes: legacy_http_bytes,
                maximum_file_count: legacy_http_count,
                maximum_note_bytes: None,
            }
        } else {
            DriverLimits::default()
        }
    })
}
