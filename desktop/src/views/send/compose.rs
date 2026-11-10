//! Pure request building for the composer: what a draft turns into, and why it may
//! not be sent. Nothing here touches GPUI state.

use std::path::PathBuf;

use gpui::SharedString;
use gpui_component::searchable_list::SearchableListItem;

use super::{ComposerMode, Transport};
use crate::model::{
    ClientCommand, DirectoryMode, NoteDraft, NoteTransport, PolicyAvailability, PolicySnapshot,
    SendFiles, SendTransport,
};

pub(super) fn draft_has_content(mode: ComposerMode, paths: &[PathBuf], note: &str) -> bool {
    match mode {
        ComposerMode::Files => !paths.is_empty(),
        ComposerMode::Notes => !note.trim().is_empty(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LifetimeChoice {
    pub(super) hours: Option<u64>,
    pub(super) label: String,
}

impl LifetimeChoice {
    pub(super) fn default_choice(hours: Option<u64>) -> Self {
        Self {
            hours: None,
            label: match hours {
                Some(hours) => format!("Default ({})", duration_label(hours)),
                None => "Default lifetime".into(),
            },
        }
    }
}

impl SearchableListItem for LifetimeChoice {
    type Value = Option<u64>;

    fn title(&self) -> SharedString {
        self.label.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.hours
    }
}

pub(super) fn duration_label(hours: u64) -> String {
    crate::ui::format::retention(hours)
}

pub(super) fn lifetime_choices(
    policy: &PolicySnapshot,
    selected: Option<Option<u64>>,
) -> Vec<LifetimeChoice> {
    let mut hours = policy.retention_options_hours.clone();
    hours.sort_unstable();
    hours.dedup();

    let mut choices = vec![LifetimeChoice::default_choice(
        policy.default_retention_hours,
    )];
    choices.extend(hours.iter().copied().map(|hours| LifetimeChoice {
        hours: Some(hours),
        label: duration_label(hours),
    }));

    if let Some(Some(selected)) = selected
        && !hours.contains(&selected)
    {
        choices.push(LifetimeChoice {
            hours: Some(selected),
            label: format!("{} (unavailable)", duration_label(selected)),
        });
    }
    choices
}

pub(super) fn unavailable_lifetime_error(
    selected: Option<Option<u64>>,
    available_hours: &[u64],
) -> Option<String> {
    selected.flatten().and_then(|hours| {
        (!available_hours.contains(&hours))
            .then_some(String::from("Lifetime is not available on this instance."))
    })
}

#[derive(Clone)]
pub(super) struct PreferenceValues {
    pub(super) password: String,
    pub(super) recipient: String,
    pub(super) expected_recipient_id: Option<u64>,
    pub(super) lifetime: String,
    pub(super) transport: Transport,
    pub(super) include_key: bool,
    pub(super) turbo: bool,
    pub(super) zip_directories: bool,
    pub(super) burn_on_read: bool,
}

pub(super) fn retention_hours(value: &str) -> Result<Option<u64>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let hours = value
        .parse::<u64>()
        .map_err(|_| "Lifetime must be a positive whole number of hours.".to_string())?;
    if hours == 0 {
        return Err("Lifetime must be a positive whole number of hours.".into());
    }
    Ok(Some(hours))
}

/// "2.0 GiB per transfer · up to 20 files" for the selected transport.
pub(super) fn driver_policy_label(policy: &crate::model::DriverPolicy) -> String {
    let size = policy.maximum_ciphertext_bytes.map_or_else(
        || "No size limit advertised".to_owned(),
        |bytes| format!("{} per transfer", crate::ui::format::bytes(bytes)),
    );
    match policy.maximum_file_count {
        Some(count) => format!(
            "{size} · up to {}",
            crate::ui::format::plural(count as u64, "file")
        ),
        None => size,
    }
}

pub(super) fn retention_label(_: &PolicySnapshot) -> String {
    "The server may shorten this lifetime.".into()
}

#[cfg(test)]
pub(super) fn build_command(
    mode: ComposerMode,
    paths: Vec<PathBuf>,
    text: String,
    title: Option<String>,
    language: String,
    preferences: PreferenceValues,
) -> Result<ClientCommand, String> {
    build_command_for_policy(
        mode,
        paths,
        text,
        title,
        language,
        preferences,
        &PolicySnapshot {
            availability: PolicyAvailability::Available,
            anonymous_uploads: true,
            default_retention_hours: Some(12),
            retention_options_hours: vec![12],
            http: crate::model::DriverPolicy {
                enabled: true,
                ..Default::default()
            },
            webrtc: crate::model::DriverPolicy {
                enabled: true,
                ..Default::default()
            },
            ..PolicySnapshot::default()
        },
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_command_for_policy(
    mode: ComposerMode,
    paths: Vec<PathBuf>,
    text: String,
    title: Option<String>,
    language: String,
    preferences: PreferenceValues,
    policy: &PolicySnapshot,
    authenticated: bool,
) -> Result<ClientCommand, String> {
    let retention_hours = retention_hours(&preferences.lifetime)?;
    if policy.availability == PolicyAvailability::Unavailable {
        return Err(policy.reason.as_deref().map_or_else(
            || "The instance has not responded yet, so nothing can be shared.".into(),
            crate::ui::format::readable_error,
        ));
    }
    let driver = if preferences.transport == Transport::Http {
        &policy.http
    } else {
        &policy.webrtc
    };
    if !driver.enabled {
        return Err("This transport is not enabled by the current instance.".into());
    }
    if let Some(hours) = retention_hours
        && !policy.retention_options_hours.contains(&hours)
    {
        return Err("Lifetime is not one of this instance's available retention choices.".into());
    }
    // Empty alone means no password. Whitespace is a valid, intentional secret.
    let password = (!preferences.password.is_empty()).then_some(preferences.password);
    let recipient = (!preferences.recipient.trim().is_empty())
        .then_some(preferences.recipient.trim().to_owned());
    match mode {
        ComposerMode::Files => {
            if recipient.is_some() && !authenticated {
                return Err("Recipient delivery requires an authenticated account.".into());
            }
            if recipient.is_some() && preferences.transport != Transport::Http {
                return Err("Recipient delivery is only available via Server; choose Server or clear the recipient before sharing.".into());
            }
            if recipient.is_some() && password.is_some() {
                return Err("Recipient delivery cannot be combined with a transfer password; clear one before sharing.".into());
            }
            if recipient.is_none() && !policy.anonymous_uploads {
                return Err("This instance requires an authenticated account for sharing.".into());
            }
            Ok(ClientCommand::SendFiles(SendFiles {
                attached_note: None,
                paths,
                directory_mode: if preferences.zip_directories {
                    DirectoryMode::Zip
                } else {
                    DirectoryMode::Individual
                },
                transport: if preferences.transport == Transport::Http {
                    SendTransport::Http
                } else {
                    SendTransport::Live
                },
                retention_hours,
                turbo: preferences.turbo,
                include_key: preferences.include_key,
                password,
                recipient,
                expected_recipient_id: preferences.expected_recipient_id,
            }))
        }
        ComposerMode::Notes => {
            if recipient.is_some() {
                return Err("Recipient delivery is not available for notes; clear the recipient before sharing.".into());
            }
            Ok(ClientCommand::CreateNote(NoteDraft {
                text,
                title,
                language,
                password,
                burn_on_read: preferences.burn_on_read,
                include_key: preferences.include_key,
                retention_hours,
                transport: if preferences.transport == Transport::Http {
                    NoteTransport::Http
                } else {
                    NoteTransport::Live
                },
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::model::{ClientCommand, SendTransport};

    #[test]
    fn empty_drafts_cannot_be_dispatched() {
        assert!(!draft_has_content(ComposerMode::Files, &[], ""));
        assert!(!draft_has_content(ComposerMode::Notes, &[], "  \n"));
        assert!(draft_has_content(
            ComposerMode::Files,
            &[PathBuf::from("report.pdf")],
            ""
        ));
        assert!(draft_has_content(ComposerMode::Notes, &[], "private note"));
    }

    #[test]
    fn invalid_lifetime_is_rejected_instead_of_becoming_default() {
        assert_eq!(retention_hours("24"), Ok(Some(24)));
        assert_eq!(retention_hours(""), Ok(None));
        assert!(retention_hours("0").is_err());
        assert!(retention_hours("tomorrow").is_err());
    }

    #[test]
    fn unavailable_lifetime_error_tracks_current_policy_choices() {
        assert_eq!(unavailable_lifetime_error(Some(None), &[1, 24]), None);
        assert_eq!(unavailable_lifetime_error(Some(Some(24)), &[1, 24]), None);
        assert_eq!(
            unavailable_lifetime_error(Some(Some(24)), &[1, 168]),
            Some("Lifetime is not available on this instance.".into())
        );
    }

    #[test]
    fn removed_default_lifetime_is_rejected_like_the_transfer_worker() {
        let policy = crate::model::PolicySnapshot {
            availability: crate::model::PolicyAvailability::Available,
            anonymous_uploads: true,
            default_retention_hours: Some(24),
            retention_options_hours: vec![1, 168],
            http: crate::model::DriverPolicy {
                enabled: true,
                ..Default::default()
            },
            webrtc: crate::model::DriverPolicy {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut values = preferences("", "");
        values.lifetime = "24".into();
        assert!(
            build_command_for_policy(
                ComposerMode::Files,
                vec!["a.txt".into()],
                String::new(),
                None,
                "plain".into(),
                values,
                &policy,
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn lifetime_choices_are_human_readable_and_preserve_removed_selection() {
        let policy = crate::model::PolicySnapshot {
            default_retention_hours: Some(24),
            retention_options_hours: vec![168, 1, 24, 24],
            ..Default::default()
        };
        let choices = lifetime_choices(&policy, Some(Some(6)));
        assert_eq!(duration_label(1), "1 hour");
        assert_eq!(duration_label(24), "1 day");
        assert_eq!(duration_label(168), "1 week");
        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Default (1 day)",
                "1 hour",
                "1 day",
                "1 week",
                "6 hours (unavailable)",
            ]
        );
        assert_eq!(choices.last().unwrap().hours, Some(6));
    }

    #[test]
    fn lifetime_choices_do_not_invent_custom_durations_without_policy_values() {
        let policy = crate::model::PolicySnapshot {
            default_retention_hours: Some(48),
            ..Default::default()
        };
        let choices = lifetime_choices(&policy, None);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].label, "Default (2 days)");
        assert_eq!(choices[0].hours, None);
    }

    fn preferences(password: &str, recipient: &str) -> PreferenceValues {
        PreferenceValues {
            password: password.into(),
            recipient: recipient.into(),
            expected_recipient_id: None,
            lifetime: "12".into(),
            transport: Transport::Http,
            include_key: true,
            turbo: false,
            zip_directories: true,
            burn_on_read: false,
        }
    }

    #[test]
    fn password_whitespace_is_an_intentional_password() {
        let command = build_command(
            ComposerMode::Files,
            vec!["a.txt".into()],
            String::new(),
            None,
            "plain".into(),
            preferences("        ", ""),
        )
        .unwrap();
        let ClientCommand::SendFiles(request) = command else {
            panic!("expected file request")
        };
        assert_eq!(request.password.as_deref(), Some("        "));
    }

    #[test]
    fn recipient_drafts_are_not_silently_dropped() {
        assert!(
            build_command(
                ComposerMode::Notes,
                vec![],
                "note".into(),
                None,
                "plain".into(),
                preferences("", "alice")
            )
            .is_err()
        );
        let mut incompatible = preferences("", "alice");
        incompatible.transport = Transport::Live;
        assert!(
            build_command(
                ComposerMode::Files,
                vec!["a.txt".into()],
                String::new(),
                None,
                "plain".into(),
                incompatible
            )
            .is_err()
        );
        assert!(
            build_command(
                ComposerMode::Files,
                vec!["a.txt".into()],
                String::new(),
                None,
                "plain".into(),
                preferences("secret", "alice"),
            )
            .is_err()
        );
    }

    #[test]
    fn mode_values_remain_independent_when_building_requests() {
        let files = preferences("file secret", "");
        let mut notes = preferences("note secret", "");
        notes.transport = Transport::Live;
        let ClientCommand::SendFiles(file) = build_command(
            ComposerMode::Files,
            vec!["a.txt".into()],
            String::new(),
            None,
            "plain".into(),
            files,
        )
        .unwrap() else {
            panic!("expected file request")
        };
        let ClientCommand::CreateNote(note) = build_command(
            ComposerMode::Notes,
            vec![],
            "note".into(),
            None,
            "plain".into(),
            notes,
        )
        .unwrap() else {
            panic!("expected note request")
        };
        assert_eq!(file.password.as_deref(), Some("file secret"));
        assert!(matches!(file.transport, SendTransport::Http));
        assert_eq!(note.password.as_deref(), Some("note secret"));
    }
}
