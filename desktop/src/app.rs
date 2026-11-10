//! Application bootstrap: configuration, theme, the main window and its lifecycle.

use std::{path::PathBuf, sync::Arc, time::Duration};

use filebeam_client_config::Config;
#[cfg(feature = "visual-test")]
use filebeam_client_config::Theme as ConfigTheme;
#[cfg(feature = "visual-test")]
use gpui::test::TestWindowExt as _;
use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, point, px, size};
#[cfg(feature = "visual-test")]
use gpui::{ExternalPaths, FileDropEvent, InputEvent, ScrollDelta, ScrollWheelEvent, Window};
#[cfg(feature = "visual-test")]
use gpui_component::theme::Theme;
use gpui_component::{Root, TitleBar};

use crate::{
    assets::Assets,
    client::DesktopClient,
    model::{ClientCommand, TransferState},
    platform::ingress::InstanceOwner,
    shell::{self, DesktopShell, Destination, Quit},
    theme::{self, ColorSource},
};

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
    let destination = Destination::from_id(destination)
        .ok_or_else(|| format!("unknown visual destination: {destination}"))?;
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
        if let Err(error) = cx.text_system().add_fonts(crate::assets::fonts()) {
            log::warn!("bundled fonts are unavailable; using system fonts: {error}");
        }
        let ui_state = crate::platform::preferences::load(&home);
        theme::set_color_source(ColorSource::decode(ui_state.accent.as_deref()), cx);
        theme::apply(&config.appearance, cx);
        // Visual fixtures open native controls before KWin activates their window.
        // Resolve Kit enter animations immediately so captures observe settled
        // popup surfaces rather than their initial transparent animation frame.
        #[cfg(feature = "visual-test")]
        if visual_geometry.is_some() {
            cx.set_reduce_motion(true);
        }
        shell::bind_keys(cx);
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
        // A first launch is spacious but not unexpectedly maximized. Saved bounds
        // are retained only as desktop geometry and are normalized on load.
        let bounds = visual_geometry.map_or_else(
            || {
                ui_state.window.as_ref().map_or_else(
                    || Bounds::centered(None, size(px(1280.), px(820.)), cx),
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
                    Ok("send-files") | Ok("send-attach-note") | Ok("send-status") => {
                        let scenario = std::env::var("FILEBEAM_VISUAL_SCENARIO").unwrap_or_default();
                        shell.update(cx, |shell, cx| {
                            shell.send.update(cx, |send, cx| {
                                send.seed_files_for_capture(scenario == "send-attach-note", cx);
                                if scenario == "send-status" {
                                    send.seed_status_for_capture(
                                        "Recipient delivery is only available via Server; choose Server or clear the recipient before sharing.",
                                        cx,
                                    );
                                }
                            });
                        });
                    }
                    Ok("send-share-ready") | Ok("send-share-split") | Ok("send-share-progress")
                    | Ok("send-share-failed") | Ok("send-share-live") => shell.update(cx, |shell, cx| {
                        shell.send.update(cx, |send, cx| {
                            send.seed_files_for_capture(false, cx);
                            send.seed_share_for_capture("j-share", cx);
                        });
                    }),
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
