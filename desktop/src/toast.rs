//! Shared in-app operational notifications.
//!
//! Keep validation beside its field. Use these helpers only after an operation
//! reached a remote service, local system resource, or background worker.

use std::collections::HashSet;

use gpui::{App, Window};
use gpui_component::{WindowExt, notification::Notification};

use crate::model::{ClientErrorCode, ClientMessage};

/// Shows a transient operational failure. GPUI Kit dismisses it after five
/// seconds and pauses that countdown while the notification stack is hovered.
pub fn error(
    title: impl Into<String>,
    detail: impl Into<String>,
    window: &mut Window,
    cx: &mut App,
) {
    window.push_notification(Notification::error(detail.into()).title(title.into()), cx);
}

/// Shows a persistent notice for a state that requires an explicit response.
/// Normal backend failures should use [`error`] instead.
pub fn critical(
    title: impl Into<String>,
    detail: impl Into<String>,
    window: &mut Window,
    cx: &mut App,
) {
    window.push_notification(
        Notification::error(detail.into())
            .title(title.into())
            .autohide(false),
        cx,
    );
}

/// Returns each newly observed operational backend event exactly once.
///
/// Validation and cancellation are intentionally not global notifications.
/// The retained ids follow the client's bounded event history, so a later,
/// identical error with a new id is still presented.
pub(crate) fn take_operational_messages<'a>(
    seen_ids: &mut HashSet<u64>,
    messages: &'a [ClientMessage],
) -> Vec<&'a ClientMessage> {
    seen_ids.retain(|id| messages.iter().any(|message| message.id == *id));
    messages
        .iter()
        .filter(|message| {
            seen_ids.insert(message.id)
                && !matches!(
                    message.error.code,
                    ClientErrorCode::InvalidInput | ClientErrorCode::Cancelled
                )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClientError, ClientErrorCode};

    fn message(id: u64, code: ClientErrorCode) -> ClientMessage {
        ClientMessage {
            id,
            operation: "request".into(),
            error: ClientError {
                code,
                detail: "same detail".into(),
            },
        }
    }

    #[test]
    fn emits_initial_and_distinct_identical_operational_events_once() {
        let mut seen = HashSet::new();
        let first = message(4, ClientErrorCode::Remote);
        let repeated = message(5, ClientErrorCode::Remote);

        assert_eq!(
            take_operational_messages(&mut seen, std::slice::from_ref(&first)).len(),
            1
        );
        assert!(take_operational_messages(&mut seen, std::slice::from_ref(&first)).is_empty());
        assert_eq!(
            take_operational_messages(&mut seen, &[first, repeated]).len(),
            1
        );
    }

    #[test]
    fn retains_ids_only_while_client_history_retains_events() {
        let mut seen = HashSet::new();
        let first = message(1, ClientErrorCode::Remote);
        let retained = message(2, ClientErrorCode::Network);
        let new = message(3, ClientErrorCode::Remote);

        assert_eq!(
            take_operational_messages(&mut seen, &[first, retained.clone()]).len(),
            2
        );
        assert_eq!(
            take_operational_messages(&mut seen, &[retained, new]).len(),
            1
        );
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn keeps_validation_and_cancellation_inline() {
        let mut seen = HashSet::new();
        assert!(
            take_operational_messages(
                &mut seen,
                &[
                    message(1, ClientErrorCode::InvalidInput),
                    message(2, ClientErrorCode::Cancelled),
                ],
            )
            .is_empty()
        );
    }
}
