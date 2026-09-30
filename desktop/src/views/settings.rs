//! Retained native settings UI. Rendering only presents retained drafts and status.

use std::{path::PathBuf, sync::Arc, time::Duration};

use filebeam_client_config::{Appearance, Config, Theme as ConfigTheme, normalize_server_url};
use filebeam_client_updater::{Options, Product, Updater};
use gpui::prelude::FluentBuilder;
use gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    ScrollHandle, StatefulInteractiveElement, Styled, Window, div, px, rgb,
};
use gpui_component::{
    Disableable, Icon, Sizable, Size,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    select::SelectState,
    switch::Switch,
};

use crate::{
    VERSION,
    client::DesktopClient,
    model::{ClientCommand, DesktopSnapshot},
    platform::tray::PresenceAvailability,
    theme::{self, Palette},
};

const APPEARANCES: [&str; 3] = ["System", "Light", "Dark"];
const COLLAPSED_SIDEBAR_WIDTH: f32 = 78.;
const COMPACT_VIEWPORT_WIDTH: f32 = 1100.;
const COMPACT_PAGE_INSET: f32 = 21.;
const WIDE_PAGE_INSET: f32 = 36.;
const PRIMARY_COLUMN_WIDTH: f32 = 670.;
const SECONDARY_COLUMN_MIN_WIDTH: f32 = 290.;
const COLUMN_GAP: f32 = 22.;

/// Settings are an entity so drafts and native component state survive renders.
pub struct SettingsPanel {
    client: Arc<DesktopClient>,
    home: PathBuf,
    server: Entity<InputState>,
    memory_limit: Entity<InputState>,
    concurrency: Entity<InputState>,
    appearance: Entity<SelectState<Vec<&'static str>>>,
    scroll_handle: ScrollHandle,
    relay_only: bool,
    auto_update: bool,
    notifications: bool,
    background_transfers: bool,
    reduced_motion: bool,
    appearance_save_in_flight: bool,
    presence: Option<PresenceAvailability>,
    notifications_available: Option<bool>,
    snapshot: DesktopSnapshot,
    pending_operation: Option<&'static str>,
    storage: Option<String>,
    status: Option<String>,
}

impl SettingsPanel {
    pub fn new(
        client: Arc<DesktopClient>,
        home: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let config = Config::load(config_home(&home)).unwrap_or_default();
        let appearance_index = APPEARANCES
            .iter()
            .position(|name| *name == appearance_name(config.appearance.theme))
            .unwrap_or(0);
        let snapshot = client.snapshot();
        let mut panel = Self {
            client,
            home,
            server: cx.new(|cx| InputState::new(window, cx).placeholder("https://filebeam.io")),
            memory_limit: cx.new(|cx| InputState::new(window, cx).placeholder("512")),
            concurrency: cx.new(|cx| InputState::new(window, cx).placeholder("Default (4)")),
            appearance: cx.new(|cx| {
                SelectState::new(
                    APPEARANCES.to_vec(),
                    Some(gpui_component::IndexPath::default().row(appearance_index)),
                    window,
                    cx,
                )
            }),
            scroll_handle: ScrollHandle::default(),
            relay_only: config.webrtc.relay_only,
            auto_update: config.updates.auto_update,
            notifications: config.desktop.notifications,
            background_transfers: config.desktop.background_transfers,
            reduced_motion: config.appearance.reduced_motion,
            appearance_save_in_flight: false,
            presence: None,
            notifications_available: None,
            snapshot,
            pending_operation: None,
            storage: None,
            status: None,
        };
        panel.server.update(cx, |input, cx| {
            input.set_value(config.server.url, window, cx)
        });
        panel.memory_limit.update(cx, |input, cx| {
            input.set_value(config.transfers.memory_limit_mib.to_string(), window, cx)
        });
        panel.concurrency.update(cx, |input, cx| {
            input.set_value(
                config
                    .transfers
                    .max_concurrency
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                window,
                cx,
            )
        });
        panel.inspect_storage(cx);
        panel.watch_service(cx);
        panel
    }

    /// The shell supplies native availability after it constructs platform services.
    pub fn set_capabilities(
        &mut self,
        presence: PresenceAvailability,
        notifications_available: bool,
        cx: &mut Context<Self>,
    ) {
        self.presence = Some(presence);
        self.notifications_available = Some(notifications_available);
        cx.notify();
    }

    fn save_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.server.read(cx).value().to_string();
        let url = match normalize_server_url(&url) {
            Ok(url) => url,
            Err(error) => {
                self.status = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        match self.client.dispatch(ClientCommand::ChangeInstance { url }) {
            Ok(()) => {
                self.pending_operation = Some("change instance");
                self.status = Some("Saving and changing instance...".into());
            }
            Err(error) => {
                self.status = None;
                crate::toast::error("Save instance failed", error.to_string(), window, cx);
            }
        }
        cx.notify();
    }

    fn save_transfer_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let memory = match self.memory_limit.read(cx).value().parse() {
            Ok(value) => value,
            Err(_) => {
                self.status = Some("Memory budget must be a number of MiB.".into());
                cx.notify();
                return;
            }
        };
        let concurrency = match self.concurrency.read(cx).value().trim() {
            "" => None,
            value => match value.parse() {
                Ok(value) => Some(value),
                Err(_) => {
                    self.status = Some("Concurrency must be blank or a whole number.".into());
                    cx.notify();
                    return;
                }
            },
        };
        let relay_only = self.relay_only;
        self.persist(window.window_handle(), cx, move |config| {
            config.transfers.memory_limit_mib = memory;
            config.transfers.max_concurrency = concurrency;
            config.webrtc.relay_only = relay_only;
        });
    }

    fn current_appearance(&self, cx: &Context<Self>) -> Appearance {
        let theme = match self
            .appearance
            .read(cx)
            .selected_value()
            .copied()
            .unwrap_or("System")
        {
            "Light" => ConfigTheme::Light,
            "Dark" => ConfigTheme::Dark,
            _ => ConfigTheme::System,
        };
        Appearance {
            theme,
            reduced_motion: self.reduced_motion,
        }
    }

    fn save_appearance(&mut self, window: gpui::AnyWindowHandle, cx: &mut Context<Self>) {
        if self.appearance_save_in_flight {
            return;
        }

        let appearance = self.current_appearance(cx);
        let saved_appearance = appearance.clone();
        let home = config_home(&self.home);
        let executor = cx.background_executor().clone();
        self.appearance_save_in_flight = true;
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { save_appearance_config(home, saved_appearance) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.appearance_save_in_flight = false;
                match result {
                    Ok(_) if this.current_appearance(cx) != appearance => {
                        this.save_appearance(window, cx);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        let detail = error.to_string();
                        let _ = window.update(cx, |_, window, cx| {
                            crate::toast::error("Appearance was not saved", detail, window, cx);
                        });
                        // A later user choice must still reach disk when an older write fails.
                        if this.current_appearance(cx) != appearance {
                            this.save_appearance(window, cx);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn select_appearance(
        &mut self,
        name: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.appearance.update(cx, |state, cx| {
            state.set_selected_value(&name, window, cx);
        });
        let appearance = self.current_appearance(cx);
        theme::apply(&appearance, cx);
        self.save_appearance(window.window_handle(), cx);
        cx.notify();
    }

    fn set_reduced_motion(
        &mut self,
        reduced_motion: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reduced_motion = reduced_motion;
        let appearance = self.current_appearance(cx);
        theme::apply(&appearance, cx);
        self.save_appearance(window.window_handle(), cx);
        cx.notify();
    }

    fn persist<F>(&mut self, window: gpui::AnyWindowHandle, cx: &mut Context<Self>, change: F)
    where
        F: FnOnce(&mut Config) + Send + 'static,
    {
        let home = config_home(&self.home);
        let client = self.client.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { Config::update(home, change) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(_) => match client.dispatch(ClientCommand::ReloadSettings) {
                        Ok(()) => {
                            this.pending_operation = Some("reload settings");
                        }
                        Err(error) => {
                            let _ = window.update(cx, |_, window, cx| {
                                crate::toast::error(
                                    "Apply settings failed",
                                    error.to_string(),
                                    window,
                                    cx,
                                );
                            });
                        }
                    },
                    Err(error) => {
                        let _ = window.update(cx, |_, window, cx| {
                            crate::toast::error(
                                "Save settings failed",
                                error.to_string(),
                                window,
                                cx,
                            );
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn inspect_storage(&mut self, cx: &mut Context<Self>) {
        let home = self.home.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let summary = executor.spawn(async move { storage_summary(home) }).await;
            let _ = this.update(cx, |this, cx| {
                this.storage = Some(summary);
                cx.notify();
            });
        })
        .detach();
    }

    fn watch_service(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if this.update(cx, |this, cx| this.sync_service(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn sync_service(&mut self, cx: &mut Context<Self>) {
        self.snapshot = self.client.snapshot();
        if let Some(operation) = self.pending_operation
            && let Some(completion) = self
                .snapshot
                .operations
                .iter()
                .rev()
                .find(|item| item.operation == operation)
        {
            self.pending_operation = None;
            self.status = Some(format!("{}: {}", completion.operation, completion.detail));
        } else if let Some(operation) = self.pending_operation
            && self
                .snapshot
                .messages
                .iter()
                .rev()
                .any(|item| item.operation == operation)
        {
            self.pending_operation = None;
            self.status = None;
        }
        cx.notify();
    }

    fn run_manual_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let home = self.home.clone();
        let executor = cx.background_executor().clone();
        let window = window.window_handle();
        cx.spawn(async move |this, cx| {
            let result = executor.spawn(async move { manual_update(home) }).await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(stage) => {
                        this.status = Some(format!(
                            "Update staged: {stage}. It will apply on next launch."
                        ));
                    }
                    Err(error) => {
                        let _ = window.update(cx, |_, window, cx| {
                            crate::toast::error(
                                "Check for updates failed",
                                error.to_string(),
                                window,
                                cx,
                            );
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn set_notifications(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.notifications = enabled;
        self.persist(window.window_handle(), cx, move |config| {
            config.desktop.notifications = enabled
        });
    }

    fn set_background_transfers(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.background_transfers = enabled;
        self.persist(window.window_handle(), cx, move |config| {
            config.desktop.background_transfers = enabled
        });
    }

    fn refresh_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.client.dispatch(ClientCommand::Refresh) {
            Ok(()) => {
                self.status = Some(
                    "Connection refresh queued; the service will report discovery results.".into(),
                )
            }
            Err(error) => {
                self.status = None;
                crate::toast::error("Check connection failed", error.to_string(), window, cx);
            }
        }
        cx.notify();
    }

    #[cfg(feature = "visual-test")]
    pub fn apply_visual_scenario(
        &mut self,
        id: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.status = match id {
            "settings-error" => {
                Some("The instance could not be saved. Check the origin and try again.".into())
            }
            "settings-saving" => Some("Saving shared configuration...".into()),
            _ => None,
        };
        cx.notify();
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette_for(gpui_component::theme::Theme::global(cx).mode);
        let notifications_available = self.notifications_available.unwrap_or(false);
        let presence_available = self
            .presence
            .as_ref()
            .is_some_and(|presence| presence.available);
        let notification_detail = if notifications_available {
            "Available"
        } else {
            "Unavailable"
        };
        let presence_detail = if presence_available {
            "Available"
        } else {
            "Unavailable"
        };
        let selected_appearance = self
            .appearance
            .read(cx)
            .selected_value()
            .copied()
            .unwrap_or("System");
        let two_columns = supports_two_columns(window.viewport_size().width.as_f32());
        let connection = group(p, "globe", "Connection")
            .child(label(p, "Filebeam instance"))
            .child(
                Input::new(&self.server)
                    .with_size(Size::Large)
                    .w_full()
                    .min_w_0(),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child("A default for new transfers. Existing jobs keep their own origin."),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(
                        Button::new("save-server")
                            .label("Save instance")
                            .with_size(Size::Large)
                            .primary()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.save_connection(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("check-connection")
                            .label("Check connection")
                            .with_size(Size::Large)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.refresh_connection(window, cx)
                            })),
                    ),
            )
            .child(
                Switch::new("relay-only")
                    .label("Use TURN relay only for WebRTC")
                    .checked(self.relay_only)
                    .on_click(cx.listener(|this, checked, _, cx| {
                        this.relay_only = *checked;
                        cx.notify();
                    })),
            );
        let appearance = group(p, "monitor", "Appearance")
            .child(div().flex().flex_wrap().gap(px(10.)).children([
                appearance_preview(p, "System", selected_appearance == "System", cx),
                appearance_preview(p, "Light", selected_appearance == "Light", cx),
                appearance_preview(p, "Dark", selected_appearance == "Dark", cx),
            ]))
            .child(div().text_size(px(11.)).text_color(p.muted).child("Follow your operating system by default. System accessibility settings take precedence."))
            .child(Switch::new("reduced-motion").label("Reduce motion").checked(self.reduced_motion).on_click(cx.listener(|this, checked, window, cx| this.set_reduced_motion(*checked, window, cx))));
        let workspace = group(p, "code", "Keyboard & workspace")
            .child(meta(p, "Command palette", "Ctrl-K"))
            .child(meta(
                p,
                "Panel width",
                "Restored widths are clamped to this window.",
            ));
        let account = group(p, "user", "Account")
            .child(meta(
                p,
                "Account and receiving keys",
                "Manage identity, Inbox delivery, and key custody from Account.",
            ))
            .child(meta(p, "Notifications", notification_detail))
            .child(meta(p, "Background availability", presence_detail))
            .child(
                Switch::new("notifications")
                    .label("Desktop notifications")
                    .checked(self.notifications)
                    .disabled(!notifications_available)
                    .on_click(cx.listener(|this, checked, window, cx| {
                        this.set_notifications(*checked, window, cx)
                    })),
            )
            .child(
                Switch::new("background-transfers")
                    .label("Continue after closing window")
                    .checked(self.background_transfers)
                    .disabled(!presence_available)
                    .on_click(cx.listener(|this, checked, window, cx| {
                        this.set_background_transfers(*checked, window, cx)
                    })),
            );
        let storage = group(p, "storage", "Local storage")
            .child(meta(
                p,
                "Transfer data",
                self.storage.as_deref().unwrap_or("Inspecting storage..."),
            ))
            .child(meta(p, "Configuration", &config_path(&self.home)));
        let about = group(p, "info", "Support & about")
            .child(meta(p, "Filebeam", VERSION))
            .child(
                Switch::new("auto-update")
                    .label("Automatically install updates")
                    .checked(self.auto_update)
                    .on_click(cx.listener(|this, checked, window, cx| {
                        this.auto_update = *checked;
                        let enabled = *checked;
                        this.persist(window.window_handle(), cx, move |config| {
                            config.updates.auto_update = enabled
                        });
                    })),
            )
            .child(
                Button::new("manual-update")
                    .label("Check for updates")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.run_manual_update(window, cx)),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child("Configuration is shared with the CLI."),
            );
        let network = group(p, "transfers", "Network & transfer behavior")
            .child(label(p, "Memory budget (MiB)"))
            .child(
                Input::new(&self.memory_limit)
                    .with_size(Size::Large)
                    .w_full()
                    .min_w_0(),
            )
            .child(label(p, "Concurrency (blank uses service default)"))
            .child(
                Input::new(&self.concurrency)
                    .with_size(Size::Large)
                    .w_full()
                    .min_w_0(),
            )
            .child(
                Button::new("save-transfers")
                    .label("Save transfer settings")
                    .with_size(Size::Large)
                    .primary()
                    .on_click(
                        cx.listener(|this, _, window, cx| this.save_transfer_settings(window, cx)),
                    ),
            );
        crate::views::page::scroller(
            "settings-scroll",
            &self.scroll_handle,
            window,
            div()
                .w_full()
                .min_w_0()
                .bg(p.page)
                .text_color(p.text)
                .child(
                    div()
                        .max_w(px(1200.))
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(crate::views::page::HEADER_GAP))
                        .child(
                            div()
                                .min_w_0()
                                .child(crate::views::page::heading(
                                    p,
                                    "Settings",
                                    "Shared desktop and CLI configuration.",
                                ))
                                .child(
                                    div()
                                        .mt(px(6.))
                                        .flex()
                                        .flex_wrap()
                                        .gap(px(6.))
                                        .text_size(px(11.))
                                        .child(div().text_color(p.subtle).child("Status"))
                                        .child(div().min_w_0().text_color(p.muted).child(
                                            self.status.as_deref().unwrap_or("Ready.").to_owned(),
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(22.))
                                .min_w_0()
                                .when(!two_columns, |this| this.flex_col())
                                .when(two_columns, |this| this.flex_row())
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(20.))
                                        .w_full()
                                        .min_w_0()
                                        .when(two_columns, |this| {
                                            this.w(px(PRIMARY_COLUMN_WIDTH)).flex_none()
                                        })
                                        .child(connection)
                                        .child(appearance)
                                        .child(workspace)
                                        .child(network),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(20.))
                                        .w_full()
                                        .min_w_0()
                                        .when(two_columns, |this| {
                                            this.flex_1().min_w(px(SECONDARY_COLUMN_MIN_WIDTH))
                                        })
                                        .child(account)
                                        .child(storage)
                                        .child(about),
                                ),
                        ),
                ),
        )
    }
}

/// Compatibility for shells that have not yet retained the settings entity.
#[derive(Default)]
pub struct SettingsState {
    pub appearance_override: Option<bool>,
}
pub fn render(_state: &mut SettingsState, p: Palette) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(20.))
        .child(div().text_size(px(28.)).child("Settings"))
        .child(
            div()
                .max_w(px(760.))
                .p(px(24.))
                .rounded(px(18.))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .child(div().text_size(px(16.)).child("Loading settings...")),
        )
}

fn group(p: Palette, icon: &str, name: &str) -> gpui::Div {
    div()
        .w_full()
        .min_w_0()
        .p(px(23.))
        .flex()
        .flex_col()
        .gap(px(18.))
        .rounded(px(12.))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(10.))
                .child(
                    Icon::default()
                        .path(format!("icons/{icon}.svg"))
                        .size(px(18.))
                        .text_color(p.accent),
                )
                .child(div().min_w_0().text_size(px(15.)).child(name.to_owned())),
        )
}
fn appearance_preview(
    p: Palette,
    name: &'static str,
    selected: bool,
    cx: &mut Context<SettingsPanel>,
) -> impl IntoElement + use<> {
    let select = cx.listener(move |this, _, window, cx| this.select_appearance(name, window, cx));
    let frame = match name {
        "System" => div()
            .h(px(59.))
            .flex()
            .rounded(px(5.))
            .overflow_hidden()
            .child(mini_theme_frame(0xF9F7FC, 0xE5DFEA, 0x7B43C2))
            .child(mini_theme_frame(0x17131E, 0x2A2433, 0x8555BB)),
        "Dark" => mini_theme_frame(0x17131E, 0x2A2433, 0x8555BB),
        _ => mini_theme_frame(0xF9F7FC, 0xE5DFEA, 0x7B43C2),
    };
    div()
        .id(format!("appearance-{}", name.to_lowercase()))
        .flex_1()
        .min_w(px(78.))
        .p(px(8.))
        .rounded(px(9.))
        .border_1()
        .border_color(if selected { p.accent } else { p.border })
        .bg(if selected { p.selected } else { p.inset })
        .cursor_pointer()
        .hover(move |style| {
            style
                .bg(if selected { p.selected } else { p.hover })
                .border_color(if selected { p.focus } else { p.muted })
        })
        .child(frame)
        .child(
            div()
                .mt(px(7.))
                .text_size(px(11.))
                .text_color(if selected { p.accent } else { p.muted })
                .child(
                    if name == "System" {
                        "Follow system"
                    } else {
                        name
                    }
                    .to_owned(),
                ),
        )
        .on_click(select)
}
fn mini_theme_frame(background: u32, sidebar: u32, accent: u32) -> gpui::Div {
    div()
        .h_full()
        .flex_1()
        .p(px(8.))
        .flex()
        .gap(px(6.))
        .bg(rgb(background))
        .child(div().w(px(20.)).rounded(px(2.)).bg(rgb(sidebar)))
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(div().w(px(48.)).h(px(20.)).rounded(px(3.)).bg(rgb(accent))),
        )
}
fn label(p: Palette, text: &str) -> gpui::Div {
    div()
        .text_size(px(11.))
        .text_color(p.muted)
        .child(text.to_owned())
}
fn meta(p: Palette, name: &str, value: &str) -> gpui::Div {
    div()
        .w_full()
        .min_w_0()
        .p(px(9.))
        .rounded(px(7.))
        .bg(p.inset)
        .child(
            div()
                .text_size(px(10.))
                .text_color(p.subtle)
                .child(name.to_owned()),
        )
        .child(div().min_w_0().text_size(px(12.)).child(value.to_owned()))
}
fn supports_two_columns(viewport_width: f32) -> bool {
    let compact = viewport_width < COMPACT_VIEWPORT_WIDTH;
    let sidebar_width = if compact {
        COLLAPSED_SIDEBAR_WIDTH
    } else {
        crate::views::studio::SIDEBAR_WIDTH
    };
    let page_inset = if compact {
        COMPACT_PAGE_INSET
    } else {
        WIDE_PAGE_INSET
    };
    let usable_content_width = viewport_width - sidebar_width - (page_inset * 2.);

    usable_content_width >= PRIMARY_COLUMN_WIDTH + COLUMN_GAP + SECONDARY_COLUMN_MIN_WIDTH
}
fn config_home(home: &std::path::Path) -> Option<PathBuf> {
    (!home.as_os_str().is_empty()).then(|| home.to_path_buf())
}
fn config_path(home: &std::path::Path) -> String {
    config_home(home)
        .map(|path| path.join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("~/.filebeam/config.toml"))
        .display()
        .to_string()
}
fn appearance_name(theme: ConfigTheme) -> &'static str {
    match theme {
        ConfigTheme::System => "System",
        ConfigTheme::Light => "Light",
        ConfigTheme::Dark => "Dark",
    }
}
fn save_appearance_config(home: Option<PathBuf>, appearance: Appearance) -> anyhow::Result<Config> {
    Config::update(home, move |config| config.appearance = appearance)
}
fn storage_summary(home: PathBuf) -> String {
    let root = if home.as_os_str().is_empty() {
        return "Unavailable until the shared home is supplied.".into();
    } else {
        home.join("transfers")
    };
    match std::fs::metadata(&root) {
        Ok(metadata) => format!(
            "{} ({})",
            root.display(),
            if metadata.is_dir() {
                "directory present"
            } else {
                "not a directory"
            }
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            format!("{} (not created yet)", root.display())
        }
        Err(error) => format!("Unavailable: {error}"),
    }
}

fn manual_update(home: PathBuf) -> anyhow::Result<String> {
    let config = Config::load(config_home(&home))?;
    let key = option_env!("FILEBEAM_RELEASE_PUBLIC_KEY")
        .filter(|key| !key.is_empty())
        .ok_or_else(|| anyhow::anyhow!("this build has no embedded release key"))?;
    Updater::new(Options {
        home: config.home.clone(),
        product: Product::Desktop,
        version: VERSION.into(),
        public_key: key.into(),
        executable: config.home.join("bin").join(if cfg!(windows) {
            "filebeam.exe"
        } else {
            "filebeam"
        }),
    })
    .run_manual()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn server_urls_use_the_shared_validator() {
        assert_eq!(
            normalize_server_url("https://example.test/path").unwrap(),
            "https://example.test"
        );
        assert!(normalize_server_url("https://user@example.test").is_err());
    }
    #[test]
    fn empty_shell_home_uses_the_shared_default_location() {
        assert_eq!(config_path(&PathBuf::new()), "~/.filebeam/config.toml");
    }
    #[test]
    fn config_defaults_to_automatic_updates() {
        assert!(Config::default().updates.auto_update);
    }
    #[test]
    fn settings_columns_follow_usable_workspace_width() {
        assert!(!supports_two_columns(960.));
        assert!(!supports_two_columns(1100.));
        assert!(!supports_two_columns(1120.));
        assert!(!supports_two_columns(1200.));
        assert!(supports_two_columns(1270.));
    }
    #[test]
    fn isolated_update_preserves_auto_update() {
        let home = std::env::temp_dir().join(format!("filebeam-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        Config::update(Some(home.clone()), |config| {
            config.desktop.notifications = true
        })
        .unwrap();
        assert!(
            Config::load(Some(home.clone()))
                .unwrap()
                .updates
                .auto_update
        );
        let _ = std::fs::remove_dir_all(home);
    }
    #[test]
    fn appearance_save_preserves_unrelated_shared_settings() {
        let home = std::env::temp_dir().join(format!(
            "filebeam-appearance-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        Config::update(Some(home.clone()), |config| {
            config.desktop.notifications = true;
            config.transfers.memory_limit_mib = 768;
        })
        .unwrap();

        save_appearance_config(
            Some(home.clone()),
            Appearance {
                theme: ConfigTheme::Dark,
                reduced_motion: true,
            },
        )
        .unwrap();

        let config = Config::load(Some(home.clone())).unwrap();
        assert_eq!(config.appearance.theme, ConfigTheme::Dark);
        assert!(config.appearance.reduced_motion);
        assert!(config.desktop.notifications);
        assert_eq!(config.transfers.memory_limit_mib, 768);
        let _ = std::fs::remove_dir_all(home);
    }
}
