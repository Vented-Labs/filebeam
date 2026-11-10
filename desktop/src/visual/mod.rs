//! Development-only synthetic snapshots for native visual capture. Timestamps are
//! placed relative to now so that relative labels read as they would in use.
//! This module is compiled only with `--features visual-test`.

use crate::{model::*, ui::format};

/// An RFC 3339 timestamp `hours` from now; negative values are in the past.
fn at(hours: i64) -> String {
    let instant = format::now() + hours * 3600;
    let (year, month, day) = format::civil_from_days(instant.div_euclid(86_400));
    let seconds = instant.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60
    )
}

#[derive(Clone, Copy)]
pub struct Scenario {
    pub id: &'static str,
    pub destination: &'static str,
    pub theme: &'static str,
}

pub const SCENARIOS: &[Scenario] = &[
    Scenario {
        id: "send-empty",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-scroll-end",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-files",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-attach-note",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-status",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-share-progress",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-share-ready",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-share-split",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-share-live",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-share-failed",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "error-toast",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "notes-populated",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "notes-scroll-end",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-lifetime",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-drag-over",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-drag-exit",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "receive-empty",
        destination: "receive",
        theme: "dark",
    },
    Scenario {
        id: "receive-scroll-end",
        destination: "receive",
        theme: "dark",
    },
    Scenario {
        id: "receive-key",
        destination: "receive",
        theme: "dark",
    },
    Scenario {
        id: "receive-password",
        destination: "receive",
        theme: "dark",
    },
    Scenario {
        id: "receive-consent",
        destination: "receive",
        theme: "dark",
    },
    Scenario {
        id: "receive-verified",
        destination: "receive",
        theme: "dark",
    },
    Scenario {
        id: "receive-export-failed",
        destination: "receive",
        theme: "dark",
    },
    Scenario {
        id: "receive-saved",
        destination: "receive",
        theme: "dark",
    },
    Scenario {
        id: "receive-expired",
        destination: "receive",
        theme: "dark",
    },
    Scenario {
        id: "receive-preview",
        destination: "receive",
        theme: "dark",
    },
    Scenario {
        id: "receive-note",
        destination: "receive",
        theme: "dark",
    },
    Scenario {
        id: "receive-note-opened",
        destination: "receive",
        theme: "dark",
    },
    Scenario {
        id: "receive-progress",
        destination: "receive",
        theme: "dark",
    },
    Scenario {
        id: "transfers-mixed",
        destination: "transfers",
        theme: "dark",
    },
    Scenario {
        id: "transfers-scroll-end",
        destination: "transfers",
        theme: "dark",
    },
    Scenario {
        id: "transfers-selected",
        destination: "transfers",
        theme: "dark",
    },
    Scenario {
        id: "transfers-receipt",
        destination: "transfers",
        theme: "dark",
    },
    Scenario {
        id: "transfers-failed",
        destination: "transfers",
        theme: "dark",
    },
    Scenario {
        id: "transfers-empty",
        destination: "transfers",
        theme: "dark",
    },
    Scenario {
        id: "inbox-signed-out",
        destination: "inbox",
        theme: "dark",
    },
    Scenario {
        id: "inbox-locked",
        destination: "inbox",
        theme: "dark",
    },
    Scenario {
        id: "inbox-empty",
        destination: "inbox",
        theme: "dark",
    },
    Scenario {
        id: "inbox-populated",
        destination: "inbox",
        theme: "dark",
    },
    Scenario {
        id: "inbox-many-scroll-end",
        destination: "inbox",
        theme: "dark",
    },
    Scenario {
        id: "inbox-no-key",
        destination: "inbox",
        theme: "dark",
    },
    Scenario {
        id: "inbox-locked-refused",
        destination: "inbox",
        theme: "dark",
    },
    Scenario {
        id: "inbox-selected",
        destination: "inbox",
        theme: "dark",
    },
    Scenario {
        id: "inbox-download-started",
        destination: "inbox",
        theme: "dark",
    },
    Scenario {
        id: "history-signed-out",
        destination: "history",
        theme: "dark",
    },
    Scenario {
        id: "history-empty",
        destination: "history",
        theme: "dark",
    },
    Scenario {
        id: "history-loading",
        destination: "history",
        theme: "dark",
    },
    Scenario {
        id: "history-populated",
        destination: "history",
        theme: "dark",
    },
    Scenario {
        id: "history-selected",
        destination: "history",
        theme: "dark",
    },
    Scenario {
        id: "history-failed",
        destination: "history",
        theme: "dark",
    },
    Scenario {
        id: "account-profile",
        destination: "account",
        theme: "dark",
    },
    Scenario {
        id: "account-scroll-end",
        destination: "account",
        theme: "dark",
    },
    Scenario {
        id: "account-key-wizard",
        destination: "account",
        theme: "dark",
    },
    Scenario {
        id: "account-auth",
        destination: "account",
        theme: "dark",
    },
    Scenario {
        id: "account-recovery",
        destination: "account",
        theme: "dark",
    },
    Scenario {
        id: "settings-light",
        destination: "settings",
        theme: "light",
    },
    Scenario {
        id: "settings-dark",
        destination: "settings",
        theme: "dark",
    },
    Scenario {
        id: "settings-scroll-end",
        destination: "settings",
        theme: "dark",
    },
    Scenario {
        id: "palette",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "dialogs",
        destination: "transfers",
        theme: "dark",
    },
    Scenario {
        id: "receive-secret",
        destination: "receive",
        theme: "dark",
    },
];

pub fn scenario(id: &str) -> Option<Scenario> {
    SCENARIOS.iter().copied().find(|scenario| scenario.id == id)
}

pub fn snapshot(id: &str) -> Option<DesktopSnapshot> {
    let scenario = scenario(id)?;
    let mut snapshot = base_snapshot();
    match scenario.id {
        "receive-key" => snapshot.pending_prompts.push(prompt(PromptKind::ShareKey)),
        "receive-password" | "receive-secret" => {
            snapshot.pending_prompts.push(prompt(PromptKind::Password))
        }
        "receive-consent" => snapshot
            .pending_prompts
            .push(prompt(PromptKind::PeerConsent)),
        "receive-verified" | "receive-export-failed" | "receive-saved" => {
            let mut job = receive_job("Product walkthrough.mp4", TransferState::Complete);
            job.capabilities.can_export = true;
            let result = VerifiedResult {
                transfer_id: job.id.clone(),
                private_path: "/fixture/private/receive-fixture-01".into(),
                verified_paths: vec![
                    "/fixture/private/receive-fixture-01/Product walkthrough.mp4".into(),
                ],
                exported_paths: if scenario.id == "receive-saved" {
                    vec!["/fixture/export/Product walkthrough.mp4".into()]
                } else {
                    Vec::new()
                },
                export_error: (scenario.id == "receive-export-failed").then(|| {
                    error(
                        ClientErrorCode::Storage,
                        "Could not save to the selected folder.",
                    )
                }),
            };
            snapshot.jobs.push(job);
            snapshot.verified_results.push(result);
        }
        "error-toast" => snapshot.messages.push(ClientMessage {
            id: 1,
            operation: "refresh contacts".into(),
            error: error(ClientErrorCode::Remote, "The server returned 503."),
        }),
        "receive-preview" => snapshot.receive_previews.push(ReceivePreview {
            attached_note: Some(filebeam_client_core::protocol::AttachedNote {
                text: "Final cut for review. The archive holds the layered sources.".into(),
                title: Some("Read first".into()),
                language: "markdown".into(),
            }),
            operation_id: Some("receive-fixture-01".into()),
            link: "https://studio.fixture.invalid/01JD8Q4XK7M2N9P3R5T6V8W0YZ".into(),
            kind: ReceiveKind::File,
            password_required: false,
            burn_on_read: false,
            items: [
                ("Product walkthrough.mp4", 482_344_960_u64),
                ("Launch assets.zip", 342_884_352),
                ("Brand guidelines.pdf", 3_758_080),
                ("release-notes.md", 1_946),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (name, size))| ReceiveItem {
                id: format!("item-{index:02}"),
                name: name.into(),
                size,
            })
            .collect(),
        }),
        "receive-note" => snapshot.receive_previews.push(ReceivePreview {
            attached_note: None,
            operation_id: None,
            link: "https://studio.fixture.invalid/01JD8Q4XK7M2N9P3R5T6V8W0YZ#k=v1.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            kind: ReceiveKind::Note,
            password_required: true,
            burn_on_read: true,
            items: Vec::new(),
        }),
        "receive-progress" => {
            let mut running = receive_job("Product walkthrough.mp4", TransferState::Running);
            running.progress.phase = "Downloading".into();
            running.progress.completed_bytes = 204_472_320;
            running.capabilities.can_pause = true;
            snapshot.jobs.push(running);
        }
        "send-share-progress"
        | "send-share-ready"
        | "send-share-split"
        | "send-share-live"
        | "send-share-failed" => {
            let mut share = job(
                "j-share",
                "Launch assets.zip",
                TransferDirection::Send,
                TransferState::Running,
                830_988_288,
                38,
                "Uploading",
            );
            match scenario.id {
                "send-share-progress" => share.capabilities.can_pause = true,
                "send-share-failed" => {
                    share.state = TransferState::Failed;
                    share.error = Some(error(
                        ClientErrorCode::Network,
                        "error sending request for url",
                    ));
                }
                other => {
                    share.state = TransferState::Complete;
                    share.progress.completed_bytes = 830_988_288;
                    share.progress.phase = "Link ready".into();
                    share.share_url = Some(
                        "https://studio.fixture.invalid/01JD8Q4XK7M2N9P3R5T6V8W0YZ#k=v1.fixture-key-material".into(),
                    );
                    share.cli_command = Some(
                        "beam down 'https://studio.fixture.invalid/01JD8Q4XK7M2N9P3R5T6V8W0YZ#k=v1.fixture-key-material'".into(),
                    );
                    if other == "send-share-split" {
                        share.share_url = Some(
                            "https://studio.fixture.invalid/01JD8Q4XK7M2N9P3R5T6V8W0YZ".into(),
                        );
                        share.separate_key = Some("v1.fixture-key-material".into());
                        share.cli_command = None;
                    }
                    if other == "send-share-live" {
                        share.state = TransferState::Running;
                        share.progress.completed_bytes = 0;
                        share.progress.phase = "Waiting for recipient".into();
                        share.capabilities.can_end_live = true;
                    }
                }
            }
            snapshot.jobs.push(share);
        }
        "transfers-mixed" | "transfers-selected" | "transfers-receipt" | "transfers-failed"
        | "dialogs" => {
            snapshot.jobs = mixed_jobs();
            snapshot.verified_results.push(VerifiedResult {
                transfer_id: "receive-fixture-01".into(),
                private_path: "/fixture/private/receive-fixture-01".into(),
                verified_paths: vec![
                    "/fixture/private/receive-fixture-01/Product walkthrough.mp4".into(),
                ],
                exported_paths: Vec::new(),
                export_error: None,
            });
        }
        "transfers-scroll-end" => snapshot.jobs = scroll_jobs(),
        "inbox-locked" | "inbox-locked-refused" => {
            snapshot.account = account(
                true,
                KeyCustody::Locked {
                    version: 2,
                    fingerprint: "ABCD 1234".into(),
                    custody_mode: "password".into(),
                },
            );
        }
        "inbox-no-key" => snapshot.account = account(true, KeyCustody::Unknown),
        "inbox-empty" => snapshot.account = account(true, configured_key()),
        "inbox-populated"
        | "inbox-selected"
        | "inbox-download-started"
        | "inbox-many-scroll-end" => {
            snapshot.account = account(true, configured_key());
            snapshot.inbox = if scenario.id == "inbox-many-scroll-end" {
                (1..=30)
                    .map(|index| {
                        inbox(
                            &format!("01JD8Q4XK7M2N9P3R5T6V8{index:04}"),
                            index * 2_097_152,
                            (index % 3) + 1,
                            -(index as i64) * 5,
                        )
                    })
                    .collect()
            } else {
                let mut first = inbox("01JD8Q4XK7M2N9P3R5T6V8W0A1", 72_351_744, 3, -2);
                first.item_ids = vec!["item-01".into(), "item-02".into(), "item-03".into()];
                vec![
                    first,
                    inbox("01JD8Q4XK7M2N9P3R5T6V8W0B2", 2_936_012, 1, -5),
                    inbox("01JD8Q4XK7M2N9P3R5T6V8W0C3", 418_204_160, 12, -26),
                    inbox("01JD8Q4XK7M2N9P3R5T6V8W0D4", 2_164, 1, -75),
                ]
            };
            if scenario.id != "inbox-many-scroll-end" {
                snapshot.staged_inbox = vec![
                    filebeam_client_core::services::inbox_receiver::StagedInbox {
                        id: "01JD8Q4XK7M2N9P3R5T6V8W0B2".into(),
                        bytes: 2_936_012,
                        state: "staged-locked".into(),
                        key_bundle_id: 2,
                    },
                    filebeam_client_core::services::inbox_receiver::StagedInbox {
                        id: "01JD8Q4XK7M2N9P3R5T6V8W0C3".into(),
                        bytes: 418_204_160,
                        state: "downloading".into(),
                        key_bundle_id: 2,
                    },
                ];
            }
        }
        "history-empty" | "history-loading" | "history-failed" => {
            snapshot.account = account(true, configured_key());
            snapshot.history.revision = 1;
            if scenario.id == "history-failed" {
                snapshot.history.error = Some("native account request returned 503".into());
            }
        }
        "history-populated" | "history-selected" => {
            snapshot.account = account(true, configured_key());
            snapshot.history.revision = 1;
            snapshot.history.page.next_cursor = Some("older".into());
            snapshot.history.page.data = history_entries();
        }
        "account-profile" | "account-scroll-end" | "account-key-wizard" | "account-recovery" => {
            snapshot.account = account(true, configured_key());
        }
        "settings-light" | "settings-dark" | "settings-scroll-end" => {
            snapshot.settings = SettingsSnapshot {
                memory_limit_mib: 512,
                max_concurrency: Some(2),
                relay_only: false,
                auto_update: true,
                restart_required: false,
            }
        }
        _ => {}
    }
    Some(snapshot)
}

fn base_snapshot() -> DesktopSnapshot {
    DesktopSnapshot {
        // This policy exists only in the visual-test feature. It is neither a
        // service default nor a production quota.
        instance: InstanceSnapshot {
            url: "https://studio.fixture.invalid".into(),
            connected: true,
            detail: None,
        },
        policy: PolicySnapshot {
            origin: "https://studio.fixture.invalid".into(),
            availability: PolicyAvailability::Available,
            reason: None,
            anonymous_uploads: true,
            default_transport: SendTransport::Http,
            http: DriverPolicy {
                enabled: true,
                maximum_ciphertext_bytes: Some(2_147_483_648),
                maximum_file_count: Some(20),
                maximum_note_bytes: Some(1_048_576),
            },
            webrtc: DriverPolicy {
                enabled: true,
                maximum_ciphertext_bytes: None,
                maximum_file_count: Some(20),
                maximum_note_bytes: Some(1_048_576),
            },
            default_retention_hours: Some(24),
            retention_options_hours: vec![1, 24, 168],
        },
        account: account(false, KeyCustody::Unknown),
        ..DesktopSnapshot::default()
    }
}

fn account(authenticated: bool, key_custody: KeyCustody) -> AccountSnapshot {
    AccountSnapshot {
        authenticated,
        email: authenticated.then(|| "mason@example.invalid".into()),
        username: authenticated.then(|| "mason".into()),
        profile_url: authenticated.then(|| "https://files.example/@mason".into()),
        email_verified: authenticated,
        inbox_enabled: authenticated,
        notification_channel: authenticated.then_some(NotificationChannel::Database),
        key_custody,
    }
}
fn configured_key() -> KeyCustody {
    KeyCustody::Configured {
        version: 2,
        fingerprint: "ABCD 1234".into(),
        custody_mode: "self".into(),
    }
}

fn history_entries() -> Vec<filebeam_client_core::services::HistoryEntry> {
    let entry = |index: u64, kind: &str, driver: &str, status: &str, items: u64, bytes: u64| {
        let open = matches!(status, "available" | "live" | "pending");
        // Open entries were shared within the last day; closed ones are older.
        let created = if open {
            -(index as i64) * 3
        } else {
            -(index as i64) * 30
        };
        filebeam_client_core::services::HistoryEntry {
            id: format!("01JD8Q4XK7M2N9P3R5T6V8H{index:03}"),
            kind: kind.into(),
            driver: driver.into(),
            delivery: if index % 3 == 0 { "inbox" } else { "link" }.into(),
            status: status.into(),
            item_count: items,
            ciphertext_bytes: bytes,
            declared_ciphertext_bytes: bytes,
            created_at: at(created),
            completed_at: Some(at(created)),
            published_at: None,
            expires_at: at(created + 24),
            removed_at: (!open).then(|| at(created + 24)),
            retention_hours: 24,
            maximum_retention_hours: 168,
            maximum_expires_at: Some(at(created + 168)),
            burn_on_read: status == "burned",
            can_delete: open,
            can_extend: status == "available",
        }
    };
    vec![
        entry(1, "files", "http", "available", 3, 342_884_352),
        entry(2, "note", "http", "available", 1, 1_946),
        entry(3, "files", "webrtc", "live", 1, 858_993_459),
        entry(4, "files", "http", "pending", 12, 96_468_992),
        entry(5, "note", "http", "burned", 1, 812),
        entry(6, "files", "http", "expired", 2, 3_758_080),
        entry(7, "files", "webrtc", "ended", 1, 192_937_984),
        entry(8, "files", "http", "deleting", 5, 26_214_400),
        entry(9, "files", "http", "deleted", 1, 482_344_960),
    ]
}

fn error(code: ClientErrorCode, detail: &str) -> ClientError {
    ClientError {
        code,
        detail: detail.into(),
    }
}
fn prompt(kind: PromptKind) -> PendingPrompt {
    PendingPrompt {
        transfer_id: "receive-fixture-01".into(),
        id: 7,
        kind,
        peer: Some("relay candidate".into()),
        directory: None,
    }
}
/// A delivery received `received` hours from now (negative) and kept for a week.
fn inbox(id: &str, ciphertext_bytes: u64, item_count: u64, received: i64) -> InboxItem {
    InboxItem {
        id: id.into(),
        ciphertext_bytes,
        item_count,
        completed_at: at(received),
        expires_at: at(received + 168),
        item_ids: vec!["item-01".into()],
    }
}
fn receive_job(name: &str, state: TransferState) -> JobSnapshot {
    JobSnapshot {
        id: "receive-fixture-01".into(),
        direction: TransferDirection::Receive,
        state,
        progress: TransferProgress {
            phase: "Verified on this device".into(),
            name: name.into(),
            item_index: 1,
            item_count: 1,
            total_bytes: Some(482_344_960),
            completed_bytes: 482_344_960,
            committed_bytes: 482_344_960,
            wire_bytes: 482_344_960,
        },
        share_url: None,
        separate_key: None,
        cli_command: None,
        checkpoint_id: Some("receive-fixture-01".into()),
        origin: Some("https://studio.fixture.invalid".into()),
        error: None,
        peer_warning: None,
        capabilities: TransferCapabilities::default(),
    }
}
fn mixed_jobs() -> Vec<JobSnapshot> {
    let mut running = job(
        "j-live",
        "Launch assets.zip",
        TransferDirection::Send,
        TransferState::Running,
        342_884_352,
        0,
        "Waiting for recipient",
    );
    running.share_url =
        Some("https://studio.fixture.invalid/01JD8Q4XK7M2N9P3R5T6V8W0YZ#k=v1.fixture".into());
    running.cli_command = Some(
        "beam receive 'https://studio.fixture.invalid/01JD8Q4XK7M2N9P3R5T6V8W0YZ#k=v1.fixture'"
            .into(),
    );
    running.capabilities.can_end_live = true;
    let mut paused = job(
        "j-paused",
        "Research archive.zip",
        TransferDirection::Send,
        TransferState::Paused,
        858_993_459,
        42,
        "Paused",
    );
    paused.capabilities.can_resume = true;
    paused.capabilities.can_discard = true;
    let mut complete = job(
        "j-note",
        "Deployment notes",
        TransferDirection::Note,
        TransferState::Complete,
        1_946,
        100,
        "Link ready",
    );
    complete.share_url = Some("https://studio.fixture.invalid/n/01JD8Q4XK7M2N9P3R5T6V8W0YZ".into());
    complete.separate_key = Some("v1.fixture-key".into());
    complete.capabilities.can_revoke = true;
    complete.capabilities.can_discard = true;
    let mut saved = job(
        "j-complete",
        "Brand guidelines.pdf",
        TransferDirection::Receive,
        TransferState::Complete,
        3_758_080,
        100,
        "Saved to folder",
    );
    saved.capabilities.can_discard = true;
    let mut failed = job(
        "j-failed",
        "Photography.zip",
        TransferDirection::Receive,
        TransferState::Failed,
        192_937_984,
        67,
        "Interrupted",
    );
    failed.error = Some(error(
        ClientErrorCode::Network,
        "Connection interrupted; retained work can be retried.",
    ));
    failed.capabilities.can_resume = true;
    failed.capabilities.can_discard = true;
    let mut verified = receive_job("Product walkthrough.mp4", TransferState::Complete);
    verified.capabilities.can_export = true;
    verified.capabilities.can_discard = true;
    vec![running, verified, paused, complete, saved, failed]
}

fn scroll_jobs() -> Vec<JobSnapshot> {
    let mut jobs = mixed_jobs();
    for index in 7..=16 {
        jobs.push(job(
            &format!("j-scroll-{index}"),
            &format!("Fixture transfer {index}.bin"),
            TransferDirection::Send,
            TransferState::Complete,
            16_777_216 * index,
            100,
            "Link ready",
        ));
    }
    jobs
}

fn job(
    id: &str,
    name: &str,
    direction: TransferDirection,
    state: TransferState,
    bytes: u64,
    progress: u64,
    phase: &str,
) -> JobSnapshot {
    let mut job = receive_job(name, state);
    job.id = id.into();
    job.checkpoint_id = Some(format!(
        "01JD8Q4XK7M2N9P3{:0>10}",
        id.replace('-', "").to_uppercase()
    ));
    job.direction = direction;
    job.progress.phase = phase.into();
    job.progress.total_bytes = Some(bytes);
    job.progress.completed_bytes = bytes.saturating_mul(progress) / 100;
    job.progress.committed_bytes = job.progress.completed_bytes;
    job.progress.wire_bytes = job.progress.completed_bytes;
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_declared_fixture_has_a_deterministic_snapshot() {
        assert!(
            SCENARIOS
                .iter()
                .all(|scenario| snapshot(scenario.id).is_some())
        );
    }

    #[test]
    fn mixed_rows_match_the_canonical_studio_fixture_order_and_sizes() {
        let rows = snapshot("transfers-mixed").unwrap().jobs;
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0].id, "j-live");
        assert_eq!(rows[2].progress.total_bytes, Some(858_993_459));
        assert_eq!(rows[5].id, "j-failed");
    }

    #[test]
    fn scroll_fixture_exceeds_the_visible_transfer_rows() {
        assert_eq!(snapshot("transfers-scroll-end").unwrap().jobs.len(), 16);
    }
}
