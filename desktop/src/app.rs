use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use filebeam_client_config::Config;
#[cfg(feature = "visual-test")]
use filebeam_client_config::Theme as ConfigTheme;
use gpui::prelude::FluentBuilder;
#[cfg(feature = "visual-test")]
use gpui::test::TestWindowExt as _;
use gpui::{
    App, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Window, WindowBounds,
    WindowOptions, actions, div, point, px, size,
};
#[cfg(feature = "visual-test")]
use gpui::{ExternalPaths, FileDropEvent, InputEvent, ScrollDelta, ScrollWheelEvent};
use gpui_component::{
    Icon, Root, TitleBar, WindowExt,
    button::{Button, ButtonVariants},
    command::{Command, CommandItem, CommandState},
    theme::Theme,
};

use crate::{
    assets::Assets,
    client::DesktopClient,
    model::{ClientCommand, TransferState},
    platform::{
        ingress::{self, InstanceOwner},
        notifications::NotificationService,
        tray::{NativePresence, PresenceEvent},
    },
    theme,
    views::{
        account::{AccountPanel, InboxPanel},
        history::HistoryPanel,
        receive::ReceivePanel,
        send::SendPanel,
        settings::SettingsPanel,
        transfers::TransfersPanel,
    },
};

actions!(
    filebeam,
    [
        OpenCommandPalette,
        OpenSend,
        OpenReceive,
        OpenTransfers,
        OpenInbox,
        OpenSettings,
        Quit
    ]
);

pub fn run() -> Result<(), String> {
    let config = Config::load(None).map_err(|error| error.to_string())?;
    let client =
        Arc::new(DesktopClient::new(Some(config.home.clone())).map_err(|error| error.to_string())?);
    run_with_client(client, config.home, Vec::new())
}

pub fn run_with_client(
    client: Arc<DesktopClient>,
    home: PathBuf,
    inputs: Vec<String>,
) -> Result<(), String> {
    run_with_instance(client, home, inputs, None)
}

/// Development-only visual launch seam. It selects an existing production
/// workspace and accepts a snapshot-only DesktopClient; normal launches cannot
/// reach this API because the `visual-test` feature is absent.
#[cfg(feature = "visual-test")]
pub fn run_visual_with_client(
    client: Arc<DesktopClient>,
    home: PathBuf,
    destination: &str,
) -> Result<(), String> {
    let destination = match destination {
        "send" => Destination::Send,
        "receive" => Destination::Receive,
        "transfers" => Destination::Transfers,
        "inbox" => Destination::Inbox,
        "settings" => Destination::Settings,
        "account" => Destination::Account,
        other => return Err(format!("unknown visual destination: {other}")),
    };
    let title = std::env::var("FILEBEAM_VISUAL_WINDOW_TITLE").ok();
    run_with_instance_at(
        client,
        home,
        Vec::new(),
        None,
        destination,
        title,
        Some(visual_geometry()?),
    )
}

pub fn run_with_instance(
    client: Arc<DesktopClient>,
    home: PathBuf,
    inputs: Vec<String>,
    owner: Option<InstanceOwner>,
) -> Result<(), String> {
    run_with_instance_at(client, home, inputs, owner, Destination::Send, None, None)
}

fn run_with_instance_at(
    client: Arc<DesktopClient>,
    home: PathBuf,
    inputs: Vec<String>,
    owner: Option<InstanceOwner>,
    initial_destination: Destination,
    window_title: Option<String>,
    visual_geometry: Option<VisualGeometry>,
) -> Result<(), String> {
    let config = Config::load(Some(home.clone())).map_err(|error| error.to_string())?;
    let application = gpui::application().with_assets(Assets);
    application.on_reopen(|cx| {
        cx.activate(true);
        for handle in cx.windows() {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        }
    });
    application.run(move |cx: &mut App| {
        gpui_component::init(cx);
        theme::apply(&config.appearance, cx);
        // Visual fixtures open native controls before KWin activates their window.
        // Resolve Kit enter animations immediately so captures observe settled
        // popup surfaces rather than their initial transparent animation frame.
        #[cfg(feature = "visual-test")]
        if visual_geometry.is_some() {
            cx.set_reduce_motion(true);
        }
        let modifier = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        cx.bind_keys([
            gpui::KeyBinding::new(&format!("{modifier}-k"), OpenCommandPalette, None),
            gpui::KeyBinding::new(&format!("{modifier}-1"), OpenSend, None),
            gpui::KeyBinding::new(&format!("{modifier}-2"), OpenReceive, None),
            gpui::KeyBinding::new(&format!("{modifier}-3"), OpenTransfers, None),
            gpui::KeyBinding::new(&format!("{modifier}-4"), OpenInbox, None),
            gpui::KeyBinding::new(&format!("{modifier}-5"), OpenSettings, None),
            gpui::KeyBinding::new(&format!("{modifier}-q"), Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.set_menus(vec![gpui::Menu {
            name: "Filebeam".into(),
            items: vec![gpui::MenuItem::action("Quit Filebeam", Quit)],
            disabled: false,
        }]);
        let quitting_client = client.clone();
        cx.on_app_quit(move |cx| {
            let client = quitting_client.clone();
            cx.background_executor().spawn(async move {
                let _ = client.dispatch(ClientCommand::Shutdown { wait_ms: 3_000 });
                let until = std::time::Instant::now() + Duration::from_secs(4);
                while std::time::Instant::now() < until {
                    if !client.snapshot().jobs.iter().any(|job| {
                        matches!(
                            job.state,
                            TransferState::Running | TransferState::PauseRequested
                        )
                    }) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(40));
                }
            })
        })
        .detach();
        let ui_state = crate::platform::preferences::load(&home);
        // A first launch is spacious but not unexpectedly maximized. Saved bounds
        // are retained only as desktop geometry and are normalized on load.
        let bounds = visual_geometry.map_or_else(
            || {
                ui_state.window.as_ref().map_or_else(
                    || Bounds::centered(None, size(px(1200.), px(800.)), cx),
                    |saved| {
                        Bounds::new(
                            point(px(saved.x), px(saved.y)),
                            size(px(saved.width), px(saved.height)),
                        )
                    },
                )
            },
            |geometry| {
                Bounds::centered(
                    None,
                    size(px(geometry.width as f32), px(geometry.height as f32)),
                    cx,
                )
            },
        );
        let window_bounds = if ui_state
            .window
            .as_ref()
            .is_some_and(|saved| saved.maximized)
        {
            WindowBounds::Maximized(bounds)
        } else {
            WindowBounds::Windowed(bounds)
        };
        let mut titlebar = TitleBar::title_bar_options();
        titlebar.title = Some(window_title.unwrap_or_else(|| "Filebeam".into()).into());
        let result = cx.open_window(
            WindowOptions {
                app_id: Some("io.filebeam.desktop".to_owned()),
                window_bounds: Some(window_bounds),
                window_min_size: Some(size(px(960.), px(640.))),
                titlebar: Some(titlebar),
                ..Default::default()
            },
            move |window, cx| {
                if let Some(geometry) = visual_geometry {
                    let viewport = window.viewport_size();
                    eprintln!(
                        "applied logical content geometry={}x{} viewport={}x{} scale={}",
                        geometry.width,
                        geometry.height,
                        viewport.width.as_f32(),
                        viewport.height.as_f32(),
                        window.scale_factor(),
                    );
                }
                let closing_client = client.clone();
                let geometry_home = home.clone();
                window.on_window_should_close(cx, move |window, cx| {
                    let active = closing_client.snapshot().jobs.iter().any(|job| {
                        matches!(
                            job.state,
                            TransferState::Running | TransferState::PauseRequested
                        )
                    });
                    if active {
                        window.minimize_window();
                        false
                    } else {
                        let bounds = window.bounds();
                        let mut state = crate::platform::preferences::load(&geometry_home);
                        state.window = Some(crate::ui_state::WindowGeometry {
                            x: bounds.origin.x.as_f32(),
                            y: bounds.origin.y.as_f32(),
                            width: bounds.size.width.as_f32(),
                            height: bounds.size.height.as_f32(),
                            maximized: window.is_maximized(),
                        });
                        let home = geometry_home.clone();
                        cx.background_executor()
                            .spawn(async move {
                                let _ = crate::platform::preferences::save(&home, &state);
                            })
                            .detach();
                        cx.quit();
                        true
                    }
                });
                #[cfg(feature = "visual-test")]
                let visual_home = home.clone();
                let shell = cx.new(|cx| {
                    DesktopShell::new(client, home, inputs, owner, initial_destination, window, cx)
                });
                // Ensure the requested visual fixture route is applied after all retained
                // panels have been initialized. Normal launches start at Send as before.
                shell.update(cx, |shell, cx| shell.select(initial_destination, cx));
                #[cfg(feature = "visual-test")]
                match std::env::var("FILEBEAM_VISUAL_SCENARIO").as_deref() {
                    Ok("notes-populated") | Ok("notes-scroll-end") => shell.update(cx, |shell, cx| {
                        shell.send.update(cx, |send, cx| {
                            send.show_notes_for_capture(cx);
                            send.seed_notes_for_capture(window, cx);
                        });
                        if std::env::var("FILEBEAM_VISUAL_SCENARIO")
                            .is_ok_and(|scenario| scenario == "notes-scroll-end")
                        {
                            window.on_next_frame(|window, cx| {
                                scroll_visual_target(window, "send-page-scroll", cx);
                            });
                        }
                    }),
                    Ok("send-scroll-end") => window.on_next_frame(|window, cx| {
                        scroll_visual_target(window, "send-page-scroll", cx);
                    }),
                    Ok("send-lifetime") => shell.update(cx, |shell, cx| {
                        shell.send.update(cx, |send, cx| {
                            send.open_lifetime_dropdown(window, cx);
                        });
                        // This invokes the real Select click handler through GPUI's native
                        // input dispatcher after the trigger has completed its first frame.
                        window.on_next_frame(|window, cx| {
                            if window.try_find("transfer-lifetime").is_some() {
                                window.click("transfer-lifetime", cx);
                                eprintln!("visual interaction=send-lifetime native-select-click=dispatched");
                            } else {
                                eprintln!("visual interaction=send-lifetime native-select-click=target-missing");
                            }
                        });
                    }),
                    Ok("send-preferences") | Ok("send-preferences-scroll") => {
                        let scroll = std::env::var("FILEBEAM_VISUAL_SCENARIO")
                            .is_ok_and(|scenario| scenario == "send-preferences-scroll");
                        window.on_next_frame(move |window, cx| {
                        if window.try_find("open-sharing-preferences").is_some() {
                            window.click("open-sharing-preferences", cx);
                                if scroll {
                                    window.on_next_frame(|window, cx| {
                                        window.dispatch_event(
                                            ScrollWheelEvent {
                                                position: point(px(780.), px(500.)),
                                                delta: ScrollDelta::Pixels(point(px(0.), px(-960.))),
                                                ..Default::default()
                                            }
                                            .to_platform_input(),
                                            cx,
                                        );
                                        eprintln!("visual interaction=send-preferences native-wheel=dispatched");
                                    });
                                }
                            eprintln!("visual interaction=send-preferences native-click=dispatched");
                        } else {
                            eprintln!("visual interaction=send-preferences target-missing");
                        }
                        });
                    }
                    Ok("send-drag-over") | Ok("send-drag-exit") => {
                        let exit = std::env::var("FILEBEAM_VISUAL_SCENARIO")
                            .is_ok_and(|scenario| scenario == "send-drag-exit");
                        window.on_next_frame(move |window, cx| {
                            window.dispatch_event(
                                FileDropEvent::Entered {
                                    position: point(px(600.), px(500.)),
                                    paths: ExternalPaths(
                                        [PathBuf::from("/fixture/drop/brief.pdf")]
                                            .into_iter()
                                            .collect(),
                                    ),
                                }
                                .to_platform_input(),
                                cx,
                            );
                            window.dispatch_event(
                                FileDropEvent::Pending {
                                    position: point(px(600.), px(500.)),
                                }
                                .to_platform_input(),
                                cx,
                            );
                            if exit {
                                window.dispatch_event(FileDropEvent::Exited.to_platform_input(), cx);
                            }
                            eprintln!(
                                "visual interaction=send-drag native-file-drop={} ",
                                if exit { "entered-exited" } else { "entered" }
                            );
                        });
                    }
                    Ok("transfers-scroll-end") => window.on_next_frame(|window, _cx| {
                        window.on_next_frame(|window, cx| {
                            window.dispatch_event(
                                ScrollWheelEvent {
                                    position: point(px(400.), px(500.)),
                                    delta: ScrollDelta::Pixels(point(px(0.), px(-10_000.))),
                                    ..Default::default()
                                }
                                .to_platform_input(),
                                cx,
                            );
                            eprintln!("visual interaction=transfers-scroll native-wheel=dispatched");
                        });
                    }),
                    Ok("receive-scroll-end") => window.on_next_frame(|window, cx| {
                        scroll_visual_target(window, "receive-page-scroll", cx);
                    }),
                    Ok("settings-scroll-end") => window.on_next_frame(|window, cx| {
                        scroll_visual_target(window, "settings-scroll", cx);
                    }),
                    Ok("account-scroll-end") => {
                        shell.update(cx, |shell, cx| {
                            shell.account.update(cx, |account, cx| {
                                account.apply_visual_scenario("account-profile", window, cx);
                            });
                        });
                        window.on_next_frame(|window, cx| {
                            scroll_visual_target(window, "account-page-scroll", cx);
                        });
                    }
                    Ok("inbox-many-scroll-end") => window.on_next_frame(|window, cx| {
                        scroll_visual_target(window, "inbox-list-scroll", cx);
                    }),
                    _ => {}
                }
                #[cfg(feature = "visual-test")]
                if let Some(theme) = visual_switch_theme() {
                    // Register after building the retained shell: the callback runs after its
                    // first dark frame and only changes app-global theme/configuration state.
                    window.on_next_frame(move |_, cx| {
                        match Config::update(Some(visual_home.clone()), move |config| {
                            config.appearance.theme = theme;
                        }) {
                            Ok(config) => {
                                crate::theme::apply(&config.appearance, cx);
                                let applied_theme = config.appearance.theme;
                                let persisted_theme = Config::load(Some(visual_home.clone()))
                                    .map(|config| config.appearance.theme)
                                    .unwrap_or(applied_theme);
                                eprintln!(
                                    "visual theme-switch applied={} persisted={} app_mode={}",
                                    visual_theme_name(applied_theme),
                                    visual_theme_name(persisted_theme),
                                    if Theme::global(cx).mode.is_dark() {
                                        "dark"
                                    } else {
                                        "light"
                                    },
                                );
                            }
                            Err(error) => {
                                eprintln!("visual theme-switch persistence failed: {error}");
                            }
                        }
                    });
                }
                cx.new(|cx| Root::new(shell, window, cx))
            },
        );
        if let Err(error) = result {
            eprintln!("Filebeam could not open its native window: {error}");
            cx.quit();
        }
    });
    Ok(())
}

#[derive(Clone, Copy)]
struct VisualGeometry {
    width: u32,
    height: u32,
}

#[cfg(feature = "visual-test")]
fn visual_geometry() -> Result<VisualGeometry, String> {
    fn dimension(name: &str, default: u32) -> Result<u32, String> {
        match std::env::var(name) {
            Ok(value) => value
                .parse::<u32>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| format!("{name} must be a positive integer")),
            Err(std::env::VarError::NotPresent) => Ok(default),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(format!("{name} must contain valid UTF-8"))
            }
        }
    }

    Ok(VisualGeometry {
        width: dimension("FILEBEAM_VISUAL_LOGICAL_WIDTH", 1440)?,
        height: dimension("FILEBEAM_VISUAL_LOGICAL_HEIGHT", 858)?,
    })
}

#[cfg(feature = "visual-test")]
fn visual_switch_theme() -> Option<ConfigTheme> {
    match std::env::var("FILEBEAM_VISUAL_SWITCH_THEME")
        .ok()
        .as_deref()
    {
        Some("light") => Some(ConfigTheme::Light),
        Some("dark") => Some(ConfigTheme::Dark),
        Some(value) => {
            eprintln!("visual theme-switch ignored invalid theme={value}");
            None
        }
        None => None,
    }
}

#[cfg(feature = "visual-test")]
fn visual_theme_name(theme: ConfigTheme) -> &'static str {
    match theme {
        ConfigTheme::Light => "light",
        ConfigTheme::Dark => "dark",
        ConfigTheme::System => "system",
    }
}

#[cfg(feature = "visual-test")]
fn scroll_visual_target(window: &mut Window, id: &'static str, _cx: &mut App) {
    // The first frame installs retained scroll input handlers. Dispatch on the
    // following frame so a production fixture receives the real wheel event.
    window.on_next_frame(move |window, cx| scroll_visual_target_now(window, id, cx));
}

#[cfg(feature = "visual-test")]
fn scroll_visual_target_now(window: &mut Window, id: &'static str, cx: &mut App) {
    // Populate TestWindowExt's public observation registry before resolving the
    // retained page identity in a real visual fixture window.
    window.render_frame(cx);
    if window.try_find(id).is_some() {
        window.scroll(id, ScrollDelta::Pixels(point(px(0.), px(-10_000.))), cx);
        eprintln!(
            "visual interaction=page-scroll target={id} observed=true method=test-window-end"
        );
    } else {
        // Native fixture windows do not install TestWindowExt's observation
        // backend. Dispatch the identical platform wheel event at the page's
        // visible center and preserve that distinction in the evidence log.
        window.dispatch_event(
            ScrollWheelEvent {
                position: point(px(500.), px(400.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(-10_000.))),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        eprintln!(
            "visual interaction=page-scroll target={id} observed=false method=platform-fallback-end"
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Destination {
    Send,
    Receive,
    Transfers,
    Inbox,
    History,
    Settings,
    Account,
}

const DESTINATIONS: [(&str, &str, Destination); 7] = [
    ("Send", "icons/arrow-up.svg", Destination::Send),
    ("Receive", "icons/arrow-down.svg", Destination::Receive),
    ("Transfers", "icons/transfers.svg", Destination::Transfers),
    ("Inbox", "icons/archive.svg", Destination::Inbox),
    ("History", "icons/transfers.svg", Destination::History),
    ("Settings", "icons/menu.svg", Destination::Settings),
    ("Account", "icons/user.svg", Destination::Account),
];

const PRIMARY_DESTINATIONS: [(&str, &str, Destination); 5] = [
    ("Send", "icons/arrow-up.svg", Destination::Send),
    ("Receive", "icons/arrow-down.svg", Destination::Receive),
    ("Transfers", "icons/transfers.svg", Destination::Transfers),
    ("Inbox", "icons/archive.svg", Destination::Inbox),
    ("History", "icons/transfers.svg", Destination::History),
];

pub struct DesktopShell {
    client: Arc<DesktopClient>,
    destination: Destination,
    send: Entity<SendPanel>,
    receive: Entity<ReceivePanel>,
    transfers: Entity<TransfersPanel>,
    inbox: Entity<InboxPanel>,
    history: Entity<HistoryPanel>,
    account: Entity<AccountPanel>,
    settings: Entity<SettingsPanel>,
    command_palette: Entity<CommandState>,
    pending_receive_link: Option<String>,
    notice: Option<String>,
    summary: (String, usize, usize),
    presence: Option<NativePresence>,
    notifications: NotificationService,
    notification_acknowledgements: Vec<std::sync::mpsc::Receiver<Result<(), String>>>,
    seen_message_ids: HashSet<u64>,
    completed_jobs: HashSet<String>,
    completion_baselined: bool,
    policy_fingerprint: PolicyFingerprint,
    _subscriptions: Vec<Subscription>,
}

#[derive(PartialEq, Eq)]
struct PolicyFingerprint {
    origin: String,
    availability: crate::model::PolicyAvailability,
    reason: Option<String>,
    anonymous_uploads: bool,
    default_transport: crate::model::SendTransport,
    http: DriverPolicyFingerprint,
    webrtc: DriverPolicyFingerprint,
    default_retention_hours: Option<u64>,
    retention_options_hours: Vec<u64>,
}

#[derive(PartialEq, Eq)]
struct DriverPolicyFingerprint {
    enabled: bool,
    maximum_ciphertext_bytes: Option<u64>,
    maximum_file_count: Option<usize>,
    maximum_note_bytes: Option<u64>,
}

impl From<&crate::model::PolicySnapshot> for PolicyFingerprint {
    fn from(policy: &crate::model::PolicySnapshot) -> Self {
        let driver = |driver: &crate::model::DriverPolicy| DriverPolicyFingerprint {
            enabled: driver.enabled,
            maximum_ciphertext_bytes: driver.maximum_ciphertext_bytes,
            maximum_file_count: driver.maximum_file_count,
            maximum_note_bytes: driver.maximum_note_bytes,
        };
        Self {
            origin: policy.origin.clone(),
            availability: policy.availability,
            reason: policy.reason.clone(),
            anonymous_uploads: policy.anonymous_uploads,
            default_transport: policy.default_transport,
            http: driver(&policy.http),
            webrtc: driver(&policy.webrtc),
            default_retention_hours: policy.default_retention_hours,
            retention_options_hours: policy.retention_options_hours.clone(),
        }
    }
}

impl DesktopShell {
    fn new(
        client: Arc<DesktopClient>,
        home: PathBuf,
        inputs: Vec<String>,
        owner: Option<InstanceOwner>,
        initial_destination: Destination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let presence = NativePresence::new().ok();
        let notifications = NotificationService::new(
            Config::load(Some(home.clone()))
                .map(|config| config.desktop.notifications)
                .unwrap_or(false),
        );
        let send = cx.new(|cx| SendPanel::new(client.clone(), home.clone(), window, cx));
        let receive = cx.new(|cx| ReceivePanel::new(client.clone(), home.clone(), window, cx));
        let transfers = cx.new(|cx| TransfersPanel::new(client.clone(), home.clone(), window, cx));
        let inbox = cx.new(|cx| InboxPanel::new(client.clone(), window, cx));
        let history = cx.new(|cx| HistoryPanel::new(client.clone(), window, cx));
        let account = cx.new(|cx| AccountPanel::new(client.clone(), window, cx));
        let settings = cx.new(|cx| SettingsPanel::new(client.clone(), home, window, cx));
        let command_palette = cx.new(|cx| CommandState::new(window, cx));
        let policy_fingerprint = PolicyFingerprint::from(&client.snapshot().policy);
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let mut activations = Vec::new();
                if let Some(owner) = &owner {
                    while let Ok(Some(activation)) = owner.try_recv() {
                        activations.push(activation.inputs);
                    }
                }
                if this
                    .update_in(cx, |shell, window, cx| {
                        while let Some(event) =
                            shell.presence.as_ref().and_then(NativePresence::try_event)
                        {
                            match event {
                                PresenceEvent::Show => {
                                    window.activate_window();
                                    cx.activate(true);
                                }
                                PresenceEvent::Quit => cx.quit(),
                            }
                        }
                        for inputs in activations {
                            shell.route_inputs(inputs, window, cx);
                            window.activate_window();
                            cx.activate(true);
                        }
                        let snapshot = shell.client.snapshot();
                        for message in crate::toast::take_operational_messages(
                            &mut shell.seen_message_ids,
                            &snapshot.messages,
                        ) {
                            crate::toast::error(
                                format!("{} failed", message.operation),
                                message.error.detail.clone(),
                                window,
                                cx,
                            );
                        }
                        let policy_fingerprint = PolicyFingerprint::from(&snapshot.policy);
                        if policy_fingerprint != shell.policy_fingerprint {
                            shell.policy_fingerprint = policy_fingerprint;
                            shell
                                .send
                                .update(cx, |send, cx| send.refresh_policy(window, cx));
                        }
                        let active = snapshot
                            .jobs
                            .iter()
                            .filter(|job| {
                                matches!(
                                    job.state,
                                    TransferState::Running | TransferState::PauseRequested
                                )
                            })
                            .count();
                        let summary = (snapshot.instance.url, active, snapshot.inbox.len());
                        let completed: HashSet<_> = snapshot
                            .jobs
                            .iter()
                            .filter(|job| matches!(job.state, TransferState::Complete))
                            .map(|job| job.id.clone())
                            .collect();
                        if shell.completion_baselined {
                            for _ in completed.difference(&shell.completed_jobs) {
                                if let Some(acknowledgement) =
                                    shell.notifications.transfer_completed()
                                {
                                    shell.notification_acknowledgements.push(acknowledgement);
                                }
                            }
                        }
                        shell
                            .notification_acknowledgements
                            .retain(|acknowledgement| {
                                matches!(
                                    acknowledgement.try_recv(),
                                    Err(std::sync::mpsc::TryRecvError::Empty)
                                )
                            });
                        shell.completed_jobs = completed;
                        shell.completion_baselined = true;
                        if summary != shell.summary {
                            shell.summary = summary;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let mut shell = Self {
            client,
            destination: initial_destination,
            send,
            receive,
            transfers,
            inbox,
            history,
            account,
            settings,
            command_palette,
            pending_receive_link: None,
            notice: None,
            summary: (String::new(), 0, 0),
            presence,
            notifications,
            notification_acknowledgements: Vec::new(),
            seen_message_ids: HashSet::new(),
            completed_jobs: HashSet::new(),
            completion_baselined: false,
            policy_fingerprint,
            _subscriptions: Vec::new(),
        };
        shell.route_inputs(inputs, window, cx);
        shell
    }

    fn route_inputs(&mut self, inputs: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(origin) = url::Url::parse(&self.client.snapshot().instance.url) else {
            return;
        };
        let mut paths = Vec::new();
        for input in inputs {
            match ingress::parse(&input, &origin) {
                Ok(ingress::Ingress::Path(path)) => paths.push(path),
                Ok(ingress::Ingress::Link { link }) => {
                    let mut value = format!("{}/{}", link.instance, link.id);
                    if let Some(key) = link.key {
                        value.push_str("#k=v1.");
                        value.push_str(&URL_SAFE_NO_PAD.encode(key));
                    }
                    self.queue_receive_link(value, window, cx);
                }
                Err(error) => self.notice = Some(error.to_string()),
            }
        }
        if !paths.is_empty() {
            self.send
                .update(cx, |send, cx| send.append_paths(paths, cx));
            self.destination = Destination::Send;
        }
        cx.notify();
    }

    fn queue_receive_link(&mut self, link: String, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_receive_link = Some(link);
        let shell = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _cx| {
            let accept = shell.clone();
            let keep = shell.clone();
            dialog.title("Open received transfer?").child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        "A received link is queued. Opening it replaces the current Receive entry.",
                    )
                    .child(
                        Button::new("open-queued-receive")
                            .label("Open transfer")
                            .primary()
                            .on_click(move |_, window, cx| {
                                let _ = accept.update(cx, |shell, cx| {
                                    if let Some(link) = shell.pending_receive_link.take() {
                                        shell.receive.update(cx, |receive, cx| {
                                            receive.set_link(link, window, cx)
                                        });
                                        shell.destination = Destination::Receive;
                                        cx.notify();
                                    }
                                });
                                window.close_dialog(cx);
                            }),
                    )
                    .child(
                        Button::new("keep-receive-draft")
                            .label("Keep current entry")
                            .ghost()
                            .on_click(move |_, window, cx| {
                                let _ =
                                    keep.update(cx, |shell, _| shell.pending_receive_link = None);
                                window.close_dialog(cx);
                            }),
                    ),
            )
        });
    }

    fn select(&mut self, destination: Destination, cx: &mut Context<Self>) {
        self.destination = destination;
        if destination == Destination::Inbox {
            let _ = self.client.dispatch(ClientCommand::RefreshInbox);
        }
        if destination == Destination::History {
            let filter = self.client.snapshot().history.filter;
            let _ = self.client.dispatch(ClientCommand::RefreshHistory {
                filter,
                cursor: None,
            });
        }
        if destination == Destination::Account {
            let _ = self.client.dispatch(ClientCommand::Refresh);
        }
        cx.notify();
    }

    fn open_palette(
        &mut self,
        _: &OpenCommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let shell = cx.entity().downgrade();
        let palette = self.command_palette.clone();
        window.open_dialog(cx, move |dialog, _, _cx| {
            let on_confirm = shell.clone();
            dialog.title("Go to workspace").child(
                Command::new(&palette)
                    .placeholder("Find a command…")
                    .items(
                        DESTINATIONS
                            .iter()
                            .map(|(label, _, _)| CommandItem::new().label(*label)),
                    )
                    .empty(|_, _, _| div().p_3().child("No matching commands"))
                    .on_confirm(move |index, window, cx| {
                        if let Some((_, _, destination)) = DESTINATIONS.get(index.row) {
                            let _ =
                                on_confirm.update(cx, |shell, cx| shell.select(*destination, cx));
                        }
                        window.close_dialog(cx);
                    }),
            )
        });
        self.command_palette
            .update(cx, |palette, cx| palette.focus(window, cx));
    }
}

impl Render for DesktopShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette_for(Theme::global(cx).mode);
        let narrow = window.viewport_size().width < px(1100.);
        let sidebar_width = if narrow {
            78.
        } else {
            theme::tokens::geometry::STUDIO_SIDEBAR
        };
        let snapshot = self.client.snapshot();
        let instance_name = url::Url::parse(&snapshot.instance.url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| snapshot.instance.url.clone());
        let account_name = snapshot
            .account
            .username
            .clone()
            .or(snapshot.account.email.clone())
            .unwrap_or_else(|| "Sign in".into());
        let account_detail = if snapshot.account.authenticated {
            snapshot
                .account
                .email
                .clone()
                .unwrap_or_else(|| "Account settings".into())
        } else {
            "Account & receiving keys".into()
        };
        let initials = account_name
            .split_whitespace()
            .filter_map(|part| part.chars().next())
            .take(2)
            .collect::<String>()
            .to_uppercase();
        let initials = if initials.is_empty() {
            "?".to_owned()
        } else {
            initials
        };
        let transfer_count = snapshot
            .jobs
            .iter()
            .filter(|job| !matches!(job.state, TransferState::Complete))
            .count();
        let inbox_count = snapshot.inbox.len();
        let active_job = snapshot
            .jobs
            .iter()
            .find(|job| {
                matches!(
                    job.state,
                    TransferState::Running | TransferState::PauseRequested | TransferState::Paused
                )
            })
            .map(|job| {
                (
                    job.progress.name.clone(),
                    match job.state {
                        TransferState::Running => {
                            if job.progress.phase.is_empty() {
                                "Transfer active".to_owned()
                            } else {
                                job.progress.phase.clone()
                            }
                        }
                        TransferState::PauseRequested => "Pause requested".to_owned(),
                        TransferState::Paused => "Paused".to_owned(),
                        _ => unreachable!("active job is filtered to a live state"),
                    },
                    job.progress.total_bytes.and_then(|total| {
                        (total > 0).then(|| {
                            ((job.progress.completed_bytes as f32 / total as f32) * 100.)
                                .clamp(0., 100.)
                        })
                    }),
                )
            });
        let navigation = div()
            .w(px(sidebar_width))
            .flex_none()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(p.sidebar)
            .border_r_1()
            .border_color(p.border)
            .px(px(if narrow { 8. } else { 13. }))
            .pt(px(if narrow { 19. } else { 23. }))
            .pb(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .when(narrow, |this| this.justify_center())
                    .gap(px(if narrow { 9. } else { 6. }))
                    .px(px(if narrow { 0. } else { 4. }))
                    .py(px(if narrow { 7. } else { 10. }))
                    .mb(px(if narrow { 20. } else { 23. }))
                    .rounded(px(9.))
                    .child(
                        div()
                            .size(px(31.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(9.))
                            .bg(p.surface)
                            .border_1()
                            .border_color(p.border)
                            .text_color(p.accent)
                            .child(Icon::default().path("icons/globe.svg").size(px(17.))),
                    )
                    .when(!narrow, |this| {
                        this.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(px(12.))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .child("Personal workspace"),
                                )
                                .child(
                                    div()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(px(10.))
                                        .text_color(p.subtle)
                                        .child(instance_name),
                                ),
                        )
                        .child(Icon::default().path("icons/chevron-down.svg").size(px(15.)))
                    }),
            )
            .when(!narrow, |this| {
                this.child(
                    div()
                        .px(px(12.))
                        .mb(px(10.))
                        .text_size(px(9.))
                        .text_color(p.subtle)
                        .child("WORKSPACE"),
                )
            })
            .children(
                PRIMARY_DESTINATIONS
                    .iter()
                    .map(|(label, icon_path, destination)| {
                        let badge = match destination {
                            Destination::Transfers => {
                                (transfer_count > 0).then_some(transfer_count)
                            }
                            Destination::Inbox => (inbox_count > 0).then_some(inbox_count),
                            _ => None,
                        };
                        shell_navigation_item(
                            label,
                            icon_path,
                            self.destination == *destination,
                            badge,
                            narrow,
                            p,
                            cx.listener(move |this: &mut DesktopShell, _, _, cx| {
                                this.select(*destination, cx)
                            }),
                        )
                    }),
            )
            .child(div().flex_1())
            .when(!narrow, |this| {
                this.when_some(active_job, |this, (name, state, progress)| {
                    this.child(
                        div()
                            .id("active-transfer-pocket")
                            .mb(px(12.))
                            .p(px(12.))
                            .rounded(px(10.))
                            .bg(p.inset)
                            .border_1()
                            .border_color(p.border)
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .text_size(px(10.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_color(p.accent)
                                    .child("●")
                                    .child(state),
                            )
                            .child(
                                div()
                                    .text_color(p.text)
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(name),
                            )
                            .when_some(progress, |this, progress| {
                                this.child(
                                    div().h(px(3.)).rounded(px(2.)).bg(p.border).child(
                                        div()
                                            .h_full()
                                            .w(px((progress / 100.) * 166.))
                                            .rounded(px(2.))
                                            .bg(p.primary),
                                    ),
                                )
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.select(Destination::Transfers, cx)
                            })),
                    )
                })
            })
            .child(
                div()
                    .mb(px(8.))
                    .child(shell_navigation_item(
                        "Settings",
                        "icons/menu.svg",
                        self.destination == Destination::Settings,
                        None,
                        narrow,
                        p,
                        cx.listener(|this: &mut DesktopShell, _, _, cx| {
                            this.select(Destination::Settings, cx)
                        }),
                    ))
                    .child(shell_navigation_item(
                        "Shortcuts",
                        "icons/code.svg",
                        false,
                        None,
                        narrow,
                        p,
                        cx.listener(|this: &mut DesktopShell, _, window, cx| {
                            this.open_palette(&OpenCommandPalette, window, cx)
                        }),
                    )),
            )
            .child(
                div()
                    .id("account-profile")
                    .border_t_1()
                    .border_color(p.border)
                    .pt(px(14.))
                    .flex()
                    .items_center()
                    .when(narrow, |this| this.justify_center())
                    .gap(px(9.))
                    .px(px(if narrow { 0. } else { 8. }))
                    .child(
                        div()
                            .size(px(31.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(9.))
                            .bg(p.selected)
                            .text_color(p.accent)
                            .text_size(px(10.))
                            .child(initials),
                    )
                    .when(!narrow, |this| {
                        this.child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .child(account_name),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(p.subtle)
                                        .child(account_detail),
                                ),
                        )
                        .child(Icon::default().path("icons/arrow-right.svg").size(px(15.)))
                    })
                    .hover(move |style| style.bg(p.hover))
                    .active(move |style| style.bg(p.selected))
                    .focusable()
                    .focus(move |style| style.border_color(p.focus))
                    .on_click(cx.listener(|this, _, _, cx| this.select(Destination::Account, cx))),
            );
        let workspace = match self.destination {
            Destination::Send => self.send.clone().into_any_element(),
            Destination::Receive => self.receive.clone().into_any_element(),
            Destination::Transfers => self.transfers.clone().into_any_element(),
            Destination::Inbox => self.inbox.clone().into_any_element(),
            Destination::History => self.history.clone().into_any_element(),
            Destination::Settings => self.settings.clone().into_any_element(),
            Destination::Account => self.account.clone().into_any_element(),
        };
        div()
            .key_context("Filebeam")
            .size_full()
            .flex()
            .flex_col()
            .bg(p.page)
            .text_color(p.text)
            .child(
                TitleBar::new()
                    .h(px(theme::tokens::geometry::TITLEBAR_HEIGHT))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(16.))
                            .flex_1()
                            .child(
                                div()
                                    .w(px(sidebar_width - 17.))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(9.))
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(crate::assets::mark().size(px(17.)).h(px(25.)))
                                    .child("Filebeam"),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .id("commands")
                                    .w(px(228.))
                                    .h(px(27.))
                                    .mr(px(16.))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .px(px(8.))
                                    .rounded(px(7.))
                                    .bg(p.inset)
                                    .border_1()
                                    .border_color(p.border)
                                    .text_size(px(11.))
                                    .text_color(p.subtle)
                                    .child(Icon::default().path("icons/search.svg").size(px(13.)))
                                    .child("Find a command…")
                                    .child(div().flex_1())
                                    .child(if cfg!(target_os = "macos") {
                                        "cmd K"
                                    } else {
                                        "ctrl K"
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_palette(&OpenCommandPalette, window, cx)
                                    })),
                            ),
                    ),
            )
            .child(
                div().flex().flex_1().min_h_0().child(navigation).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        // This is a finite viewport, not a shell-owned scroller. Retained
                        // pages own their ScrollHandles, insets, and scrollbar placement.
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .size_full()
                                .min_h_0()
                                .flex()
                                .flex_col()
                                .child(workspace),
                        ),
                ),
            )
            .child(
                div()
                    .h(px(theme::tokens::geometry::STATUSBAR_HEIGHT))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .px(px(13.))
                    .bg(p.chrome)
                    .border_t_1()
                    .border_color(p.border)
                    .text_size(px(10.))
                    .text_color(p.subtle)
                    .child(self.summary.0.clone())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(Icon::default().path("icons/lock.svg").size(px(12.)))
                            .child("End-to-end encrypted"),
                    )
                    .child(div().flex_1())
                    .child(self.notice.clone().unwrap_or_else(|| {
                        format!(
                            "{} active transfer{}",
                            self.summary.1,
                            if self.summary.1 == 1 { "" } else { "s" }
                        )
                    })),
            )
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
            .on_action(cx.listener(Self::open_palette))
            .on_action(cx.listener(|this, _: &OpenSend, _, cx| this.select(Destination::Send, cx)))
            .on_action(
                cx.listener(|this, _: &OpenReceive, _, cx| this.select(Destination::Receive, cx)),
            )
            .on_action(
                cx.listener(|this, _: &OpenTransfers, _, cx| {
                    this.select(Destination::Transfers, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &OpenInbox, _, cx| this.select(Destination::Inbox, cx)),
            )
            .on_action(
                cx.listener(|this, _: &OpenSettings, _, cx| this.select(Destination::Settings, cx)),
            )
    }
}

fn shell_navigation_item(
    label: &'static str,
    icon_path: &'static str,
    active: bool,
    badge: Option<usize>,
    narrow: bool,
    p: theme::Palette,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let aria_label = badge.map_or_else(
        || label.to_owned(),
        |count| format!("{label}, {count} unread"),
    );
    div()
        .id(SharedString::from(format!("navigation-{label}")))
        .aria_label(aria_label)
        .relative()
        .h(px(if narrow { 59. } else { 41. }))
        .mb(px(if narrow { 6. } else { 3. }))
        .flex()
        .items_center()
        .justify_start()
        .gap(px(if narrow { 4. } else { 10. }))
        .px(px(if narrow { 2. } else { 12. }))
        .rounded(px(8.))
        .border_1()
        .border_color(gpui::transparent_black())
        .bg(if active { p.selected } else { p.sidebar })
        .text_color(if active { p.selection_text } else { p.muted })
        .hover(move |style| style.bg(if active { p.raised } else { p.hover }))
        .active(move |style| style.bg(p.selected).border_color(p.focus))
        .focusable()
        .focus_visible(move |style| style.border_color(p.focus))
        .when(narrow, |this| this.flex_col().justify_center())
        .when(active, |this| {
            this.child(
                div()
                    .absolute()
                    .left(px(0.))
                    .top(px(if narrow { 21. } else { 12.5 }))
                    .w(px(3.))
                    .h(px(16.))
                    .rounded(px(2.))
                    .bg(p.accent),
            )
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(if narrow { 4. } else { 10. }))
                .when(narrow, |this| this.flex_col().justify_center())
                .when(!narrow, |this| this.flex_1().min_w_0())
                .child(
                    Icon::default()
                        .path(icon_path)
                        .size(px(if narrow { 19. } else { 18. })),
                )
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(if narrow { 9. } else { 12. }))
                        .child(label),
                ),
        )
        .when(!narrow, |this| {
            this.child(
                div()
                    .w(px(20.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when_some(badge, |this, badge| {
                        this.child(
                            div()
                                .min_w(px(16.))
                                .h(px(16.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .bg(p.inset)
                                .text_size(px(9.))
                                .text_color(p.subtle)
                                .child(badge.to_string()),
                        )
                    }),
            )
        })
        .when(narrow, |this| {
            this.when_some(badge, |this, badge| {
                this.child(
                    div()
                        .absolute()
                        .top(px(4.))
                        .right(px(4.))
                        .min_w(px(15.))
                        .h(px(15.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .bg(p.inset)
                        .text_size(px(8.))
                        .text_color(p.subtle)
                        .child(badge.to_string()),
                )
            })
        })
        .on_click(on_click)
}
