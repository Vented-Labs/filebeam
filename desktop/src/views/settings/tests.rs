use filebeam_client_config::normalize_server_url;
use gpui::{TestAppContext, WindowHandle, test::TestWindowExt as _};
use gpui_component::Root;

use super::*;
use crate::{
    model::{
        AccountSnapshot, ClientError, ClientErrorCode, ClientMessage, DeviceAvailability,
        DeviceSnapshot, DeviceTrust, OperationStatus,
    },
    testing::{self, Commands},
};

fn panel(
    cx: &mut TestAppContext,
    snapshot: DesktopSnapshot,
    home: PathBuf,
) -> (WindowHandle<Root>, Entity<SettingsPanel>, Commands) {
    testing::rooted(cx, snapshot, move |live, window, cx| {
        SettingsPanel::new(live, home, window, cx)
    })
}

fn set(
    cx: &mut TestAppContext,
    window: WindowHandle<Root>,
    input: &Entity<InputState>,
    value: &'static str,
) {
    testing::in_window(cx, window, |window, cx| {
        input.update(cx, |input, cx| input.set_value(value, window, cx));
    });
}

fn device(id: &str, name: &str, trust: DeviceTrust, local: bool) -> DeviceSnapshot {
    DeviceSnapshot {
        id: id.into(),
        name: name.into(),
        platform: "linux".into(),
        trust,
        availability: DeviceAvailability::Unknown,
        detail: local.then(|| "This device".into()),
        fingerprint: "ABCD 1234".into(),
    }
}

fn signed_in() -> DesktopSnapshot {
    DesktopSnapshot {
        account: AccountSnapshot {
            authenticated: true,
            ..AccountSnapshot::default()
        },
        ..DesktopSnapshot::default()
    }
}

#[test]
fn limits_are_checked_against_the_shared_configuration_range() {
    assert_eq!(parse_memory(" 512 "), Ok(512));
    assert!(parse_memory("63").is_err());
    assert!(parse_memory("4097").is_err());
    assert!(parse_memory("lots").is_err());
    assert_eq!(parse_concurrency(""), Ok(None));
    assert_eq!(parse_concurrency(" 4 "), Ok(Some(4)));
    assert!(parse_concurrency("0").is_err());
    assert!(parse_concurrency("65").is_err());
    assert!(parse_concurrency("-1").is_err());
}

#[test]
fn server_urls_use_the_shared_validator() {
    assert_eq!(
        normalize_server_url("https://example.test/path").unwrap(),
        "https://example.test"
    );
    assert!(normalize_server_url("https://user@example.test").is_err());
}

#[test]
fn appearance_save_preserves_unrelated_shared_settings() {
    let home = tempfile::tempdir().unwrap();
    Config::update(Some(home.path().to_path_buf()), |config| {
        config.desktop.notifications = true;
        config.transfers.memory_limit_mib = 768;
    })
    .unwrap();
    save_appearance_config(
        Some(home.path().to_path_buf()),
        Appearance {
            theme: ConfigTheme::Dark,
            reduced_motion: true,
        },
    )
    .unwrap();
    let config = Config::load(Some(home.path().to_path_buf())).unwrap();
    assert_eq!(config.appearance.theme, ConfigTheme::Dark);
    assert!(config.appearance.reduced_motion);
    assert!(config.desktop.notifications);
    assert!(config.updates.auto_update);
    assert_eq!(config.transfers.memory_limit_mib, 768);
}

#[gpui::test]
fn the_instance_is_validated_and_refreshed_once_changed(cx: &mut TestAppContext) {
    let home = tempfile::tempdir().unwrap();
    let mut snapshot = DesktopSnapshot::default();
    snapshot.instance.url = "https://filebeam.io".into();
    let (window, panel, commands) = panel(cx, snapshot.clone(), home.path().to_path_buf());
    let server = panel.read_with(cx, |panel, _| panel.server.clone());

    set(cx, window, &server, "filebeam");
    panel.update(cx, |panel, cx| panel.save_instance(cx));
    assert!(
        commands.try_recv().is_err(),
        "an incomplete address is not sent"
    );
    panel.read_with(cx, |panel, _| {
        assert_eq!(
            panel.notice.as_ref().map(|notice| notice.area),
            Some(Area::Instance)
        );
    });

    set(cx, window, &server, "https://filebeam.io/");
    panel.update(cx, |panel, cx| panel.save_instance(cx));
    assert!(
        commands.try_recv().is_err(),
        "the current instance is not re-applied"
    );

    set(cx, window, &server, "https://files.example.test/some/page");
    panel.update(cx, |panel, cx| {
        panel.save_instance(cx);
        panel.save_instance(cx);
    });
    assert!(matches!(
        commands.try_recv(),
        Ok(ClientCommand::ChangeInstance { url }) if url == "https://files.example.test"
    ));
    assert!(commands.try_recv().is_err());

    snapshot.operations.push(OperationStatus {
        id: 1,
        operation: "change instance".into(),
    });
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| panel.on_snapshot(&snapshot, window, cx));
    });
    assert!(matches!(commands.try_recv(), Ok(ClientCommand::Refresh)));
}

#[gpui::test]
fn switches_tell_the_shell_and_reach_the_shared_configuration(cx: &mut TestAppContext) {
    let home = tempfile::tempdir().unwrap();
    let (window, panel, commands) =
        panel(cx, DesktopSnapshot::default(), home.path().to_path_buf());
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = events.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&panel, move |_, event: &SettingsEvent, _| {
            seen.borrow_mut().push(*event);
        })
    });
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| {
            panel.set_capabilities(
                PresenceAvailability {
                    available: true,
                    reason: None,
                },
                true,
                cx,
            );
            assert!(!panel.background_transfers() || panel.background_transfers);
        });
        window.render_frame(cx);
        window.click("notifications", cx);
    });
    cx.run_until_parked();
    let first = events.borrow().first().copied();
    assert!(matches!(first, Some(SettingsEvent::Notifications(_))));
    let Some(SettingsEvent::Notifications(enabled)) = first else {
        unreachable!()
    };
    assert_eq!(
        Config::load(Some(home.path().to_path_buf()))
            .unwrap()
            .desktop
            .notifications,
        enabled
    );
    assert!(
        commands.try_recv().is_err(),
        "the service does not use this setting"
    );
}

#[gpui::test]
fn limits_save_together_and_ask_the_service_to_reload(cx: &mut TestAppContext) {
    let home = tempfile::tempdir().unwrap();
    let (window, panel, commands) =
        panel(cx, DesktopSnapshot::default(), home.path().to_path_buf());
    let (memory, concurrency) = panel.read_with(cx, |panel, _| {
        (panel.memory_limit.clone(), panel.concurrency.clone())
    });
    set(cx, window, &memory, "12");
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| panel.save_limits(window, cx));
    });
    cx.run_until_parked();
    assert!(
        commands.try_recv().is_err(),
        "an out-of-range budget is not saved"
    );

    set(cx, window, &memory, "1024");
    set(cx, window, &concurrency, "3");
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| panel.save_limits(window, cx));
    });
    cx.run_until_parked();
    assert!(matches!(
        commands.try_recv(),
        Ok(ClientCommand::ReloadSettings)
    ));
    let config = Config::load(Some(home.path().to_path_buf())).unwrap();
    assert_eq!(config.transfers.memory_limit_mib, 1024);
    assert_eq!(config.transfers.max_concurrency, Some(3));
}

#[gpui::test]
fn choosing_a_colour_repaints_and_is_remembered_for_this_device(cx: &mut TestAppContext) {
    use crate::theme::engine::Preset;
    let home = tempfile::tempdir().unwrap();
    let (window, panel, _commands) =
        panel(cx, DesktopSnapshot::default(), home.path().to_path_buf());
    panel.update(cx, |panel, cx| {
        panel.select_tab(Tab::Appearance, cx);
        panel.select_source(ColorSource::Preset(Preset::Teal), cx);
    });
    testing::in_window(cx, window, |window, cx| window.render_frame(cx));
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(theme::color_source(cx), ColorSource::Preset(Preset::Teal));
        let dark = theme::palette(cx).dark;
        assert!(theme::is_applied(
            ColorSource::Preset(Preset::Teal),
            dark,
            cx
        ));
    });
    assert_eq!(
        crate::platform::preferences::load(home.path())
            .accent
            .as_deref(),
        Some("preset:teal")
    );

    // A malformed colour is explained and leaves the look alone.
    let custom = panel.read_with(cx, |panel, _| panel.custom_color.clone());
    set(cx, window, &custom, "#12");
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| panel.apply_custom_color(window, cx));
    });
    cx.update(|cx| assert_eq!(theme::color_source(cx), ColorSource::Preset(Preset::Teal)));
    set(cx, window, &custom, "#2f6feb");
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| panel.apply_custom_color(window, cx));
    });
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(theme::color_source(cx), ColorSource::Custom(0x2f6feb)));
    assert_eq!(
        crate::platform::preferences::load(home.path())
            .accent
            .as_deref(),
        Some("custom:2f6feb")
    );
    // Later tests start from the default look again.
    cx.update(|cx| theme::set_color_source(ColorSource::default(), cx));
}

#[gpui::test]
fn revoking_a_device_takes_its_password_in_a_prompt(cx: &mut TestAppContext) {
    let home = tempfile::tempdir().unwrap();
    let mut snapshot = signed_in();
    snapshot.device_directory.devices = vec![
        device("local", "Studio", DeviceTrust::Verified, true),
        device("phone", "Pixel", DeviceTrust::Registered, false),
    ];
    let (window, panel, commands) = panel(cx, snapshot.clone(), home.path().to_path_buf());
    panel.update(cx, |panel, cx| panel.select_tab(Tab::Devices, cx));
    assert!(matches!(
        commands.try_recv(),
        Ok(ClientCommand::RefreshDevices)
    ));
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| {
            panel.requests.clear();
            panel.expanded = Some("phone".into());
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.try_find("approve-device").is_some());
        assert!(window.try_find("confirm-device-peer").is_some());
        window.click("revoke-device", cx);
    });
    let confirm = |cx: &mut TestAppContext| {
        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            window.click("dialog-confirm", cx);
        });
    };
    confirm(cx);
    assert!(commands.try_recv().is_err(), "revoking needs the password");
    let password = panel.read_with(cx, |panel, _| panel.revoke_password.clone());
    set(cx, window, &password, "hunter2");
    confirm(cx);
    assert!(matches!(
        commands.try_recv(),
        Ok(ClientCommand::RevokeDevice { id, current_password })
            if id == "phone" && current_password == "hunter2"
    ));

    snapshot.messages.push(ClientMessage {
        id: 1,
        operation: "revoke device".into(),
        error: ClientError {
            code: ClientErrorCode::Remote,
            detail: "native account request returned 422".into(),
        },
    });
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| {
            panel.on_snapshot(&snapshot, window, cx);
            assert_eq!(
                panel.notice.as_ref().map(|notice| notice.area),
                Some(Area::Devices)
            );
            assert!(panel.revoke_password.read(cx).value().is_empty());
        });
    });
}

#[gpui::test]
fn every_tab_renders_signed_in_and_out(cx: &mut TestAppContext) {
    let home = tempfile::tempdir().unwrap();
    let mut pending = signed_in();
    pending.device_directory.enrollment_payload = Some("{\"request\":true}".into());
    pending.device_directory.pending_fingerprint = Some("ABCD 1234".into());
    for snapshot in [DesktopSnapshot::default(), signed_in(), pending] {
        let (window, panel, _commands) = panel(cx, snapshot, home.path().to_path_buf());
        for tab in Tab::ALL {
            panel.update(cx, |panel, cx| {
                panel.tab = tab;
                cx.notify();
            });
            testing::in_window(cx, window, |window, cx| window.render_frame(cx));
        }
    }
}
