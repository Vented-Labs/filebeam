//! Development-only deterministic snapshots for native Studio visual capture.
//! This module is compiled only with `--features visual-test`.

use crate::model::*;

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
        id: "send-queue",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-errors",
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
        id: "send-preferences",
        destination: "send",
        theme: "dark",
    },
    Scenario {
        id: "send-preferences-scroll",
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
        id: "narrow-sheets",
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
        "receive-password" => snapshot.pending_prompts.push(prompt(PromptKind::Password)),
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
        "receive-expired" | "error-toast" => snapshot.messages.push(ClientMessage {
            id: 1,
            operation: if scenario.id == "error-toast" {
                "native account request".into()
            } else {
                "inspect receive".into()
            },
            error: error(
                ClientErrorCode::Remote,
                if scenario.id == "error-toast" {
                    "The server returned 404."
                } else {
                    "This transfer has expired."
                },
            ),
        }),
        "transfers-mixed" | "dialogs" => snapshot.jobs = mixed_jobs(),
        "transfers-scroll-end" => snapshot.jobs = scroll_jobs(),
        "inbox-locked" => {
            snapshot.account.authenticated = true;
            snapshot.account.key_custody = KeyCustody::Locked {
                version: 2,
                fingerprint: "ABCD 1234".into(),
                custody_mode: "password".into(),
            };
            snapshot
                .inbox
                .push(inbox("in-3", 0, 1, "2026-09-18T00:00:00Z"));
        }
        "inbox-populated" | "inbox-many-scroll-end" => {
            snapshot.account = account(
                true,
                KeyCustody::Configured {
                    version: 2,
                    fingerprint: "ABCD 1234".into(),
                    custody_mode: "self".into(),
                },
            );
            snapshot.inbox = if scenario.id == "inbox-many-scroll-end" {
                (1..=40)
                    .map(|index| {
                        inbox(
                            &format!("in-{index}"),
                            index * 2_097_152,
                            (index % 3) + 1,
                            &format!("2026-09-{index:02}T10:36:00Z"),
                        )
                    })
                    .collect()
            } else {
                vec![
                    inbox("in-1", 72_351_744, 2, "2026-09-18T10:24:00Z"),
                    inbox("in-2", 2_936_012, 1, "2026-09-18T09:36:00Z"),
                    inbox("in-3", 0, 1, "2026-09-18T00:00:00Z"),
                    inbox("in-4", 2_164, 1, "2026-09-17T10:36:00Z"),
                ]
            };
        }
        "account-profile" | "account-scroll-end" | "account-key-wizard" | "account-recovery" => {
            snapshot.account = account(
                true,
                KeyCustody::Configured {
                    version: 2,
                    fingerprint: "ABCD 1234".into(),
                    custody_mode: "self".into(),
                },
            );
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
fn inbox(id: &str, ciphertext_bytes: u64, item_count: u64, completed_at: &str) -> InboxItem {
    InboxItem {
        id: id.into(),
        ciphertext_bytes,
        item_count,
        completed_at: completed_at.into(),
        expires_at: "2026-09-25T10:36:00Z".into(),
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
    running.direction = TransferDirection::Send;
    running.capabilities.can_pause = true;
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
    let complete = job(
        "j-note",
        "Deployment notes",
        TransferDirection::Note,
        TransferState::Complete,
        1_946,
        100,
        "Link ready",
    );
    let saved = job(
        "j-complete",
        "Brand guidelines.pdf",
        TransferDirection::Receive,
        TransferState::Complete,
        3_758_080,
        100,
        "Saved to folder",
    );
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
    vec![
        running,
        receive_job("Product walkthrough.mp4", TransferState::Complete),
        paused,
        complete,
        saved,
        failed,
    ]
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
