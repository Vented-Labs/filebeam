//! Settings: the instance, how the desktop behaves, its look, transfer limits and
//! registered devices. Changes are written off the UI thread; rendering only shows
//! retained drafts and what the service last reported.

mod appearance;
mod devices;
mod general;

use std::{path::PathBuf, sync::Arc, time::Duration};

use filebeam_client_config::{
    Appearance, Config, MAX_CONCURRENCY, MAX_MEMORY_LIMIT_MIB, MIN_MEMORY_LIMIT_MIB,
    Theme as ConfigTheme,
};
use gpui::{
    AnyElement, AppContext, ClipboardItem, Context, Entity, EventEmitter, IntoElement,
    ParentElement, Render, ScrollHandle, Styled, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};

use crate::{
    client::DesktopClient,
    live::Live,
    model::{ClientCommand, DesktopSnapshot},
    platform::tray::PresenceAvailability,
    request::{Outcome, Request},
    theme::{self, ColorSource, Palette, tokens::geometry},
    ui::{
        MotionExt, Tone, feedback, format, motion, page,
        rail::{Segment, rail},
    },
};

/// Changes the shell has to act on, because it owns the services they configure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsEvent {
    Notifications(bool),
    BackgroundTransfers(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    General,
    Appearance,
    Network,
    Devices,
    About,
}

impl Tab {
    const ALL: [Self; 5] = [
        Self::General,
        Self::Appearance,
        Self::Network,
        Self::Devices,
        Self::About,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Network => "Network",
            Self::Devices => "Devices",
            Self::About => "About",
        }
    }

    const fn icon(self) -> &'static str {
        match self {
            Self::General => "settings",
            Self::Appearance => "brush",
            Self::Network => "globe",
            Self::Devices => "laptop",
            Self::About => "info",
        }
    }
}

/// Where the result of a change is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Area {
    Instance,
    Desktop,
    Updates,
    Appearance,
    Network,
    Devices,
    Revoke,
}

fn area(operation: &str) -> Area {
    match operation {
        "change instance" => Area::Instance,
        "reload settings" => Area::Network,
        _ => Area::Devices,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Notice {
    area: Area,
    tone: Tone,
    text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum UpdateCheck {
    Idle,
    Checking,
    Done(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Copied {
    ConfigPath,
    DataPath,
    Enrollment,
    Fingerprint,
}

/// A memory budget in MiB, or why the draft is not one.
fn parse_memory(draft: &str) -> Result<u64, String> {
    draft
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|value| (MIN_MEMORY_LIMIT_MIB..=MAX_MEMORY_LIMIT_MIB).contains(value))
        .ok_or_else(|| {
            format!(
                "Enter a whole number of MiB between {MIN_MEMORY_LIMIT_MIB} and \
                 {MAX_MEMORY_LIMIT_MIB}."
            )
        })
}

/// A transfer limit, where an empty draft leaves the choice to Filebeam.
fn parse_concurrency(draft: &str) -> Result<Option<u32>, String> {
    let draft = draft.trim();
    if draft.is_empty() {
        return Ok(None);
    }
    draft
        .parse::<u32>()
        .ok()
        .filter(|value| (1..=MAX_CONCURRENCY).contains(value))
        .map(Some)
        .ok_or_else(|| {
            format!("Enter a whole number from 1 to {MAX_CONCURRENCY}, or leave it empty.")
        })
}

fn config_home(home: &std::path::Path) -> Option<PathBuf> {
    (!home.as_os_str().is_empty()).then(|| home.to_path_buf())
}

/// Settings are an entity so drafts and native component state survive renders.
pub struct SettingsPanel {
    client: Arc<DesktopClient>,
    live: Entity<Live>,
    home: PathBuf,
    tab: Tab,
    server: Entity<InputState>,
    memory_limit: Entity<InputState>,
    concurrency: Entity<InputState>,
    custom_color: Entity<InputState>,
    device_name: Entity<InputState>,
    approval_payload: Entity<InputState>,
    approval_fingerprint: Entity<InputState>,
    rename_name: Entity<InputState>,
    verify_fingerprint: Entity<InputState>,
    revoke_password: Entity<InputState>,
    dropzone_alias: Entity<InputState>,
    theme: ConfigTheme,
    reduced_motion: bool,
    appearance_save_in_flight: bool,
    relay_only: bool,
    auto_update: bool,
    notifications: bool,
    background_transfers: bool,
    dropzone_auto: bool,
    presence: Option<PresenceAvailability>,
    notifications_available: bool,
    update: UpdateCheck,
    /// The device whose actions are open.
    expanded: Option<String>,
    requests: Vec<Request>,
    notice: Option<Notice>,
    copied: Option<Copied>,
    copy_generation: u64,
    page_scroll: ScrollHandle,
}

impl EventEmitter<SettingsEvent> for SettingsPanel {}

impl SettingsPanel {
    pub fn new(
        live: Entity<Live>,
        home: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let client = live.read(cx).client().clone();
        let config = Config::load(config_home(&home)).unwrap_or_default();
        let field = |placeholder: &'static str,
                     value: String,
                     masked: bool,
                     window: &mut Window,
                     cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut input = InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(masked);
                input.set_value(value, window, cx);
                input
            })
        };
        let server = field(
            "https://filebeam.io",
            config.server.url.clone(),
            false,
            window,
            cx,
        );
        let memory_limit = field(
            "512",
            config.transfers.memory_limit_mib.to_string(),
            false,
            window,
            cx,
        );
        let concurrency = field(
            "Automatic",
            config
                .transfers
                .max_concurrency
                .map(|value| value.to_string())
                .unwrap_or_default(),
            false,
            window,
            cx,
        );
        let custom = match theme::color_source(cx) {
            ColorSource::Custom(seed) => theme::color::format(seed),
            ColorSource::Preset(_) => String::new(),
        };
        let custom_color = field("#rrggbb", custom, false, window, cx);
        let device_name = field("This desktop", String::new(), false, window, cx);
        let approval_payload = field(
            "Paste the request from the new device",
            String::new(),
            false,
            window,
            cx,
        );
        let approval_fingerprint = field(
            "Fingerprint shown on the new device",
            String::new(),
            false,
            window,
            cx,
        );
        let rename_name = field("New name", String::new(), false, window, cx);
        let verify_fingerprint = field(
            "Fingerprint shown on that device",
            String::new(),
            false,
            window,
            cx,
        );
        let revoke_password = field("Current password", String::new(), true, window, cx);
        let dropzone_alias = field("downloads", String::new(), false, window, cx);

        cx.subscribe_in(
            &server,
            window,
            |this, _, event: &InputEvent, _, cx| match event {
                InputEvent::Change => this.clear_notice(Area::Instance, cx),
                InputEvent::PressEnter { .. } => this.save_instance(cx),
                _ => {}
            },
        )
        .detach();
        for input in [&memory_limit, &concurrency] {
            cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => this.clear_notice(Area::Network, cx),
                    InputEvent::PressEnter { .. } => this.save_limits(cx),
                    _ => {}
                },
            )
            .detach();
        }
        cx.subscribe_in(
            &custom_color,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => this.clear_notice(Area::Appearance, cx),
                InputEvent::PressEnter { .. } => this.apply_custom_color(window, cx),
                _ => {}
            },
        )
        .detach();
        cx.observe_in(&live, window, |this, live, window, cx| {
            let snapshot = live.read(cx).snapshot();
            this.on_snapshot(&snapshot, window, cx);
        })
        .detach();

        Self {
            client,
            live,
            home,
            tab: Tab::General,
            server,
            memory_limit,
            concurrency,
            custom_color,
            device_name,
            approval_payload,
            approval_fingerprint,
            rename_name,
            verify_fingerprint,
            revoke_password,
            dropzone_alias,
            theme: config.appearance.theme,
            reduced_motion: config.appearance.reduced_motion,
            appearance_save_in_flight: false,
            relay_only: config.webrtc.relay_only,
            auto_update: config.updates.auto_update,
            notifications: config.desktop.notifications,
            background_transfers: config.desktop.background_transfers,
            dropzone_auto: false,
            presence: None,
            notifications_available: false,
            update: UpdateCheck::Idle,
            expanded: None,
            requests: Vec::new(),
            notice: None,
            copied: None,
            copy_generation: 0,
            page_scroll: ScrollHandle::new(),
        }
    }

    /// The shell supplies native availability after it constructs platform services.
    pub fn set_capabilities(
        &mut self,
        presence: PresenceAvailability,
        notifications_available: bool,
        cx: &mut Context<Self>,
    ) {
        self.presence = Some(presence);
        self.notifications_available = notifications_available;
        cx.notify();
    }

    fn snapshot(&self, cx: &Context<Self>) -> Arc<DesktopSnapshot> {
        self.live.read(cx).snapshot()
    }

    fn waiting(&self, operation: &str) -> bool {
        self.requests
            .iter()
            .any(|request| request.operation() == operation)
    }

    fn text(input: &Entity<InputState>, cx: &Context<Self>) -> String {
        input.read(cx).value().trim().to_owned()
    }

    fn clear(input: &Entity<InputState>, window: &mut Window, cx: &mut Context<Self>) {
        input.update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn say(&mut self, area: Area, tone: Tone, text: impl Into<String>, cx: &mut Context<Self>) {
        self.notice = Some(Notice {
            area,
            tone,
            text: text.into(),
        });
        cx.notify();
    }

    fn clear_notice(&mut self, area: Area, cx: &mut Context<Self>) {
        if self
            .notice
            .as_ref()
            .is_some_and(|notice| notice.area == area)
        {
            self.notice = None;
            cx.notify();
        }
    }

    fn notice(&self, area: Area, p: Palette) -> Option<AnyElement> {
        self.notice
            .as_ref()
            .filter(|notice| notice.area == area)
            .map(|notice| feedback::notice(p, notice.text.clone(), notice.tone).into_any_element())
    }

    fn send(&mut self, command: ClientCommand, cx: &mut Context<Self>) {
        let operation = command.operation();
        match Request::send(&self.client, command) {
            Ok(request) => {
                self.clear_notice(area(operation), cx);
                self.requests.push(request);
            }
            Err(error) => self.say(
                area(operation),
                Tone::Danger,
                format::readable_error(&error.to_string()),
                cx,
            ),
        }
        cx.notify();
    }

    fn on_snapshot(
        &mut self,
        snapshot: &DesktopSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut settled = Vec::new();
        self.requests
            .retain(|request| match request.outcome(snapshot) {
                Outcome::Waiting => true,
                outcome => {
                    settled.push((request.operation(), outcome));
                    false
                }
            });
        for (operation, outcome) in settled {
            match outcome {
                Outcome::Failed(detail) => {
                    if operation == "revoke device" {
                        Self::clear(&self.revoke_password.clone(), window, cx);
                    }
                    self.say(
                        area(operation),
                        Tone::Danger,
                        format::readable_error(&detail),
                        cx,
                    );
                }
                Outcome::Completed => match operation {
                    "change instance" => {
                        // The new instance's policy and account are read straight away.
                        let _ = self.client.dispatch(ClientCommand::Refresh);
                        self.say(Area::Instance, Tone::Success, "Instance changed.", cx);
                    }
                    "reload settings" if snapshot.settings.restart_required => self.say(
                        Area::Network,
                        Tone::Warning,
                        "Saved. Restart Filebeam for the new limits to take effect.",
                        cx,
                    ),
                    "reload settings" => {
                        self.say(Area::Network, Tone::Success, "Saved.", cx);
                    }
                    "enroll device" => Self::clear(&self.device_name.clone(), window, cx),
                    "approve device enrollment" => {
                        Self::clear(&self.approval_payload.clone(), window, cx);
                        Self::clear(&self.approval_fingerprint.clone(), window, cx);
                        self.say(Area::Devices, Tone::Success, "Device approved.", cx);
                    }
                    "rename device" => Self::clear(&self.rename_name.clone(), window, cx),
                    "confirm device verification" => {
                        Self::clear(&self.verify_fingerprint.clone(), window, cx);
                        self.say(Area::Devices, Tone::Success, "Device verified.", cx);
                    }
                    "revoke device" => {
                        Self::clear(&self.revoke_password.clone(), window, cx);
                        self.expanded = None;
                        self.say(Area::Devices, Tone::Success, "Device revoked.", cx);
                    }
                    "add device dropzone" => {
                        Self::clear(&self.dropzone_alias.clone(), window, cx);
                    }
                    _ => {}
                },
                Outcome::Waiting => {}
            }
        }
        cx.notify();
    }

    fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
            if tab == Tab::Devices && self.snapshot(cx).account.authenticated {
                self.send(ClientCommand::RefreshDevices, cx);
            }
            cx.notify();
        }
    }

    /// Writes a change to the shared configuration off the UI thread, then asks the
    /// service to load it when `reload` is set.
    fn persist<F>(&mut self, area: Area, reload: bool, cx: &mut Context<Self>, change: F)
    where
        F: FnOnce(&mut Config) + Send + 'static,
    {
        let home = config_home(&self.home);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { Config::update(home, change) })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(_) if reload => this.send(ClientCommand::ReloadSettings, cx),
                Ok(_) => {}
                Err(error) => this.say(
                    area,
                    Tone::Danger,
                    format!("That could not be saved: {error}"),
                    cx,
                ),
            });
        })
        .detach();
        cx.notify();
    }

    fn current_appearance(&self) -> Appearance {
        Appearance {
            theme: self.theme,
            reduced_motion: self.reduced_motion,
        }
    }

    /// Saves the appearance, and saves again if it changed while the write ran.
    fn save_appearance(&mut self, cx: &mut Context<Self>) {
        if self.appearance_save_in_flight {
            return;
        }
        let appearance = self.current_appearance();
        let saved = appearance.clone();
        let home = config_home(&self.home);
        let executor = cx.background_executor().clone();
        self.appearance_save_in_flight = true;
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { save_appearance_config(home, saved) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.appearance_save_in_flight = false;
                if let Err(error) = &result {
                    this.say(
                        Area::Appearance,
                        Tone::Danger,
                        format!("Your choice applies now but could not be saved: {error}"),
                        cx,
                    );
                }
                // A later choice must still reach disk, whether or not this write did.
                if this.current_appearance() != appearance {
                    this.save_appearance(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn copy(&mut self, value: String, kind: Copied, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(value));
        self.copied = Some(kind);
        self.copy_generation = self.copy_generation.wrapping_add(1);
        let generation = self.copy_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1800))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.copy_generation == generation {
                    this.copied = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
}

fn save_appearance_config(home: Option<PathBuf>, appearance: Appearance) -> anyhow::Result<Config> {
    Config::update(home, move |config| config.appearance = appearance)
}

impl Render for SettingsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let snapshot = self.snapshot(cx);
        let header = page::header(
            p,
            "Settings",
            "How Filebeam looks, connects and behaves on this device.",
            window,
            cx,
        );
        let panel = cx.entity().downgrade();
        let tabs = rail(
            "settings-tabs",
            "Settings section",
            Tab::ALL
                .iter()
                .position(|tab| *tab == self.tab)
                .unwrap_or(0),
        )
        .segment_width(if page::is_compact(window) { 118. } else { 132. })
        .segments(
            Tab::ALL
                .iter()
                .map(|tab| Segment::new(tab.label()).icon(tab.icon())),
        )
        .on_select(move |index, _, cx| {
            let _ = panel.update(cx, |this, cx| {
                if let Some(tab) = Tab::ALL.get(index) {
                    this.select_tab(*tab, cx);
                }
            });
        });
        let body = match self.tab {
            Tab::General => self.general(&snapshot, p, cx),
            Tab::Appearance => self.appearance(p, cx),
            Tab::Network => self.network(&snapshot, p, cx),
            Tab::Devices => self.devices(&snapshot, p, cx),
            Tab::About => self.about(p, cx),
        };
        let tabs_entrance = motion::entrance("settings-tabs", 1, window, cx);
        let body_entrance = motion::entrance(("settings-body", self.tab.label()), 2, window, cx);
        page::scroller(
            "settings-scroll",
            &self.page_scroll,
            geometry::CONTENT_READING,
            false,
            window,
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(20.))
                .child(header)
                .child(div().flex().entering(tabs_entrance).child(tabs))
                .child(div().w_full().entering(body_entrance).child(body)),
        )
    }
}

#[cfg(feature = "visual-test")]
impl SettingsPanel {
    /// Puts the page in the state a capture scenario names.
    pub fn prepare_for_capture(
        &mut self,
        scenario: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.requests.clear();
        self.notice = None;
        match scenario {
            "settings-appearance" | "settings-light" | "settings-dark" => {
                self.tab = Tab::Appearance;
            }
            "settings-network" => self.tab = Tab::Network,
            "settings-devices" => {
                self.tab = Tab::Devices;
                self.expanded = self
                    .snapshot(cx)
                    .device_directory
                    .devices
                    .get(1)
                    .map(|device| device.id.clone());
            }
            "settings-about" => self.tab = Tab::About,
            "settings-instance-refused" => {
                self.server
                    .update(cx, |input, cx| input.set_value("filebeam", window, cx));
                self.save_instance(cx);
            }
            _ => {}
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
