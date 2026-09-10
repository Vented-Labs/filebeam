use filebeam_transfer::*;

#[test]
fn layout_has_tags_and_rejects_unsafe_bounds() {
    assert_eq!(chunk_count(10, 4), Ok(3));
    assert_eq!(ciphertext_bytes(10, 4), Ok(58));
    assert_eq!(chunk_count(0, 4), Ok(0));
    assert!(chunk_count(1, 0).is_err());
    assert!(chunk_count(u64::MAX, 1).is_err());
    assert!(ciphertext_bytes(u64::MAX - 1, MAX_CIPHERTEXT_BYTES - AEAD_TAG_BYTES).is_err());
}

#[test]
fn layout_properties_hold_across_small_untrusted_inputs() {
    for chunk in 1..=64 {
        for plaintext in 0..=1_024 {
            let count = chunk_count(plaintext, chunk).unwrap();
            let expected_count = plaintext.div_ceil(chunk);
            assert_eq!(count, expected_count);
            assert_eq!(
                ciphertext_bytes(plaintext, chunk).unwrap(),
                plaintext + count * AEAD_TAG_BYTES
            );
        }
    }
}

#[test]
fn memory_limit_counts_all_live_copies() {
    let chunk = 25_000_000u64 - AEAD_TAG_BYTES;
    assert_eq!(concurrency_limit(8, chunk, 128 * 1024 * 1024, 8), 2);
    assert_eq!(concurrency_limit(8, chunk, 64 * 1024 * 1024, 8), 1);
    let explicit = TransferMemoryBudget::new(200_000, 20_000, 2, 3);
    assert_eq!(explicit.slot_bytes(10_000), Some(50_048));
    assert_eq!(concurrency_limit_with_memory(8, 10_000, explicit, 8), 3);
    assert_eq!(concurrency_limit(99, chunk, 1, 99), 1);
    assert_eq!(concurrency_limit(2, u64::MAX, u64::MAX, 8), 1);
}

#[test]
fn retry_policy_is_bounded_and_stage_specific() {
    assert!(retryable_status(423, true));
    assert!(!retryable_status(423, false));
    assert!(!retryable_status(422, true));
    assert_eq!(retry_delay_ms(0, None, 17), 267);
    assert_eq!(retry_delay_ms(u32::MAX, None, u64::MAX), 60_000);
    assert_eq!(retry_delay_ms(0, Some(90_000), 0), 60_000);
}

#[test]
fn transport_rejects_untrusted_policy_and_bounds_sizing() {
    let bad = UploadTransport {
        version: 2,
        part_min_bytes: 1,
        part_max_bytes: 2,
        request_target_ms: 1,
        request_budget_ms: 1,
        part_max_count: None,
    };
    assert!(bad.validate().is_err());
    assert_eq!(bad.part_bytes(1.0), 0);
    let policy = UploadTransport {
        version: 1,
        part_min_bytes: 100,
        part_max_bytes: 1_000,
        request_target_ms: 100,
        request_budget_ms: 200,
        part_max_count: Some(8),
    };
    assert_eq!(policy.part_bytes(20.0), 1_000);
    assert_eq!(policy.grow_part(100, 10_000, 1), 150);
    assert_eq!(policy.shrink_part(101), 100);
    assert!(policy.should_stage(1_000, 4.0));
    assert!(policy.should_abandon_direct(10_000, 1_000, 3_000));
    assert!(policy.validate_part_count(801, 100).is_err());
    assert!(policy.validate_part_count(800, 100).is_ok());
    assert_eq!(policy.remaining_part_count(800, 300, 100), Ok(5));
    assert!(policy.remaining_part_count(800, 801, 100).is_err());
}

#[test]
fn transport_rejects_every_unsupported_server_bound() {
    let valid = UploadTransport {
        version: 1,
        part_min_bytes: 10,
        part_max_bytes: 100,
        request_target_ms: 10,
        request_budget_ms: 10,
        part_max_count: None,
    };
    let cases = [
        UploadTransport {
            version: 0,
            ..valid.clone()
        },
        UploadTransport {
            part_min_bytes: 0,
            ..valid.clone()
        },
        UploadTransport {
            part_max_bytes: 9,
            ..valid.clone()
        },
        UploadTransport {
            part_max_bytes: MAX_CIPHERTEXT_BYTES + 1,
            ..valid.clone()
        },
        UploadTransport {
            request_target_ms: 0,
            ..valid.clone()
        },
        UploadTransport {
            request_budget_ms: 9,
            ..valid.clone()
        },
        UploadTransport {
            part_max_count: Some(0),
            ..valid
        },
    ];
    assert!(cases.iter().all(|policy| policy.validate().is_err()));
}

#[test]
fn controller_learns_short_requests_and_respects_cooldown() {
    let mut c = AdaptiveConcurrency::new(4);
    c.observe(600_000, 1_000, 0);
    c.observe(600_000, 1_000, 1);
    assert_eq!(c.limit(), 2);
    c.congested(2);
    assert_eq!(c.limit(), 1);
    c.observe(1_000_000, 1_000, 3_000);
    c.observe(1_000_000, 1_000, 4_000);
    assert_eq!(c.limit(), 1);
    c.observe(1_000_000, 1_000, 8_003);
    c.observe(1_000_000, 1_000, 8_004);
    assert_eq!(c.limit(), 2);
}

#[test]
fn sampled_rates_do_not_count_resets_or_idle_time() {
    let mut c = AdaptiveConcurrency::new(2);
    c.sample("a", 100, 0);
    c.sample("a", 200, 1_000);
    c.sample("a", 300, 1_500);
    assert_eq!(c.rate(), 0.0); // The sample window has not elapsed.
    c.sample("a", 400, 2_000);
    assert!(c.rate() > 0.0);
    let before = c.rate();
    c.sample("a", 0, 100_000);
    c.forget("a");
    assert_eq!(c.rate(), before);
}

#[test]
fn stage_recovers_lost_ack_but_rejects_conflicts_and_bounds_recovery() {
    let mut session = StageSession::new("one".into(), 100, "sum".into()).unwrap();
    let status = StageStatus {
        id: "one".into(),
        state: StageState::Receiving,
        offset: 25,
        ciphertext_bytes: 100,
        checksum: "sum".into(),
    };
    assert_eq!(
        session.acknowledge_part(0, 25, &status),
        Ok(StageAction::Continue { offset: 25 })
    );
    assert!(session.reprobe_after_recovery());
    assert!(!session.reprobe_after_recovery());
    let recovered = StageStatus {
        offset: 50,
        ..status.clone()
    };
    assert_eq!(
        session.reconcile(&recovered),
        Ok(StageAction::Continue { offset: 50 })
    );
    let backwards = StageStatus {
        offset: 49,
        ..recovered.clone()
    };
    assert!(session.reconcile(&backwards).is_err());
    let complete = StageStatus {
        state: StageState::Complete,
        offset: 100,
        ..recovered.clone()
    };
    session.reconcile(&complete).unwrap();
    assert!(session.reconcile(&status).is_err());
    for _ in 0..8 {
        session.record_retry().unwrap();
    }
    assert!(session.record_retry().is_err());
    assert_eq!(session.retries, 8);
    session.reset("two".into()).unwrap();
    session.reset("three".into()).unwrap();
    assert!(session.reset("four".into()).is_err());
    assert_eq!(session.resets, 2);
}

#[test]
fn immediate_ack_is_exact_and_invalid_status_does_not_mutate_session() {
    let mut session = StageSession::new("stage".into(), 100, "checksum".into()).unwrap();
    let advanced = StageStatus {
        id: "stage".into(),
        state: StageState::Receiving,
        offset: 51,
        ciphertext_bytes: 100,
        checksum: "checksum".into(),
    };
    assert!(session.acknowledge_part(0, 50, &advanced).is_err());
    assert_eq!(session.offset, 0);
    let wrong_checksum = StageStatus {
        checksum: "other".into(),
        offset: 50,
        ..advanced
    };
    assert!(session.reconcile(&wrong_checksum).is_err());
    assert_eq!(session.offset, 0);
}

#[test]
fn retry_backoff_trace_is_deterministic_and_capped() {
    let delays: Vec<_> = (0..8)
        .map(|attempt| retry_delay_ms(attempt, None, attempt as u64 * 13))
        .collect();
    assert_eq!(
        delays,
        vec![250, 513, 1_026, 2_039, 4_052, 8_065, 16_078, 32_091]
    );
    assert_eq!(retry_delay_ms(8, None, 0), 60_000);
    assert_eq!(retry_delay_ms(3, Some(12), 99), 12);
}

#[test]
fn short_request_trace_probes_gradually_without_completion_based_rejection() {
    let mut controller = AdaptiveConcurrency::new(4);
    controller.observe(600_000, 1_000, 0);
    controller.observe(600_000, 1_000, 1);
    assert_eq!(controller.limit(), 2);
    for now in 2_001..2_005 {
        controller.observe(600_000, 1_000, now);
    }
    assert_eq!(controller.limit(), 3);
    for now in 4_004..4_010 {
        controller.observe(600_000, 1_000, now);
    }
    assert_eq!(controller.limit(), 4);
    for now in 6_010..6_030 {
        controller.observe(600_000, 1_000, now);
    }
    assert_eq!(controller.limit(), 4);
}

#[test]
fn aggregate_probe_trace_accepts_improvement_then_backs_off_once_when_slow() {
    let mut controller = AdaptiveConcurrency::new(3);
    controller.observe(600_000, 1_000, 0);
    controller.observe(600_000, 1_000, 1);
    assert_eq!(controller.limit(), 2);
    controller.sample("a", 600_000, 0);
    controller.sample("b", 600_000, 1_000);
    controller.sample("c", 400_000, 2_001);
    assert_eq!(controller.limit(), 2); // Accepted probe; no immediate re-probe.
    for now in 4_002..4_006 {
        controller.observe(600_000, 1_000, now);
    }
    assert_eq!(controller.limit(), 3);
    controller.sample("a", 1_000_000, 6_006);
    controller.sample("b", 1_000_000, 7_006);
    controller.sample("c", 600_000, 8_007);
    assert_eq!(controller.limit(), 2);
}

#[test]
fn samples_ignore_duplicates_and_handle_resets_and_distinct_late_ids() {
    let mut controller = AdaptiveConcurrency::new(2);
    controller.sample("first", 100, 0);
    controller.sample("first", 100, 2_000);
    assert_eq!(controller.rate(), 0.0);
    controller.sample("first", 0, 2_001);
    controller.sample("late", 100, 2_001);
    controller.sample("first", 100, 3_000);
    controller.sample("late", 200, 4_001);
    assert!(controller.rate() > 0.0);
    let before = controller.rate();
    controller.forget("first");
    controller.forget("late");
    controller.sample("first", 999_999, 1_000_000); // Late callback: retired ID.
    assert_eq!(controller.rate(), before);
    controller.sample("first", 0, 1_000_001); // Explicit new lifetime.
    controller.sample("first", 100, 1_001_001);
    controller.sample("new-id", 0, 1_000_000);
    assert_eq!(controller.rate(), before);
}

#[test]
fn stage_session_round_trips_for_suspend_resume() {
    let mut session = StageSession::new("id".into(), 100, "sum".into()).unwrap();
    session.record_retry().unwrap();
    let encoded = serde_json::to_string(&session).unwrap();
    let mut restored: StageSession = serde_json::from_str(&encoded).unwrap();
    let status = StageStatus {
        id: "id".into(),
        state: StageState::Receiving,
        offset: 20,
        ciphertext_bytes: 100,
        checksum: "sum".into(),
    };
    assert_eq!(
        restored.reconcile(&status),
        Ok(StageAction::Continue { offset: 20 })
    );
    assert_eq!(restored.retries, 1);
}

#[test]
fn progress_tracks_ciphertext_without_double_counting_active_work() {
    let mut p = TransferProgress::new(100, 116).unwrap();
    p.set_active(16).unwrap();
    p.complete(16).unwrap();
    assert_eq!((p.completed_ciphertext, p.active_ciphertext), (16, 0));
    assert!(p.set_active(101).is_err());
    assert!(p.complete(101).is_err());
}
