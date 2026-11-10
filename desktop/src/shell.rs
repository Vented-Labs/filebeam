//! The window shell: navigation, title bar, ambient backdrop and the active page.
//! Pages are retained entities; the shell only chooses which one is on screen.

use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use filebeam_client_config::Config;
use gpui::{
    App, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Task, Window, actions,
    div, prelude::FluentBuilder, px, relative,
};
use gpui_base::Button as BaseButton;
use gpui_component::{
    Root, TitleBar, WindowExt,
    command::{Command, CommandGroup, CommandItem, CommandState},
    tooltip::Tooltip,
};

use crate::{
    assets::icon,
    client::DesktopClient,
    live::Live,
    model::{ClientCommand, DesktopSnapshot, TransferState},
    platform::{
        ingress::{self, InstanceOwner},
        notifications::NotificationService,
        tray::{NativePresence, PresenceEvent},
    },
    theme::{
        self, Palette,
        tokens::{geometry, radius, text as size},
    },
    ui::{
        self, MotionExt, TextExt, brand, button, feedback, format, motion, progress::progress, text,
    },
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
        OpenHistory,
        OpenSettings,
        OpenAccount,
        Quit
    ]
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    Send,
    Receive,
    Transfers,
    Inbox,
    History,
    Settings,
    Account,
}

impl Destination {
    /// The pages reachable from the main navigation group, in order.
    pub const PRIMARY: [Self; 5] = [
        Self::Send,
        Self::Receive,
        Self::Transfers,
        Self::Inbox,
        Self::History,
    ];
    pub const ALL: [Self; 7] = [
        Self::Send,
        Self::Receive,
        Self::Transfers,
        Self::Inbox,
        Self::History,
        Self::Settings,
        Self::Account,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Receive => "receive",
            Self::Transfers => "transfers",
            Self::Inbox => "inbox",
            Self::History => "history",
            Self::Settings => "settings",
            Self::Account => "account",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|destination| destination.id() == id)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Send => "Send",
            Self::Receive => "Receive",
            Self::Transfers => "Transfers",
            Self::Inbox => "Inbox",
            Self::History => "History",
            Self::Settings => "Settings",
            Self::Account => "Account",
        }
    }

    const fn icon(self) -> &'static str {
        match self {
            Self::Send => "upload",
            Self::Receive => "download",
            Self::Transfers => "transfers",
            Self::Inbox => "archive",
            Self::History => "clock",
            Self::Settings => "settings",
            Self::Account => "user",
        }
    }

    const fn shortcut(self) -> Option<char> {
        match self {
            Self::Send => Some('1'),
            Self::Receive => Some('2'),
            Self::Transfers => Some('3'),
            Self::Inbox => Some('4'),
            Self::History => Some('5'),
            Self::Settings => Some(','),
            Self::Account => None,
        }
    }
}

pub fn modifier() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    }
}

fn modifier_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl"
    }
}

/// Registers the shell's global shortcuts. Call once per application.
pub fn bind_keys(cx: &mut App) {
    let key = |suffix: &str| format!("{}-{suffix}", modifier());
    cx.bind_keys([
        gpui::KeyBinding::new(&key("k"), OpenCommandPalette, None),
        gpui::KeyBinding::new(&key("1"), OpenSend, None),
        gpui::KeyBinding::new(&key("2"), OpenReceive, None),
        gpui::KeyBinding::new(&key("3"), OpenTransfers, None),
        gpui::KeyBinding::new(&key("4"), OpenInbox, None),
        gpui::KeyBinding::new(&key("5"), OpenHistory, None),
        gpui::KeyBinding::new(&key(","), OpenSettings, None),
        gpui::KeyBinding::new(&key("q"), Quit, None),
    ]);
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

fn is_active(state: TransferState) -> bool {
    matches!(
        state,
        TransferState::Running | TransferState::PauseRequested
    )
}

pub struct DesktopShell {
    client: Arc<DesktopClient>,
    live: Entity<Live>,
    destination: Destination,
    pub(crate) send: Entity<SendPanel>,
    pub(crate) receive: Entity<ReceivePanel>,
    pub(crate) transfers: Entity<TransfersPanel>,
    pub(crate) inbox: Entity<InboxPanel>,
    pub(crate) history: Entity<HistoryPanel>,
    pub(crate) account: Entity<AccountPanel>,
    pub(crate) settings: Entity<SettingsPanel>,
    command_palette: Entity<CommandState>,
    pending_receive_link: Option<String>,
    presence: Option<NativePresence>,
    notifications: NotificationService,
    notification_acknowledgements: Vec<std::sync::mpsc::Receiver<Result<(), String>>>,
    seen_message_ids: HashSet<u64>,
    completed_jobs: HashSet<String>,
    completion_baselined: bool,
    policy_fingerprint: PolicyFingerprint,
    _subscriptions: Vec<Subscription>,
    _events: Task<()>,
}

impl DesktopShell {
    pub fn new(
        client: Arc<DesktopClient>,
        home: PathBuf,
        inputs: Vec<String>,
        owner: Option<InstanceOwner>,
        initial_destination: Destination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let presence = NativePresence::new().ok();
        let notifications_enabled = Config::load(Some(home.clone()))
            .map(|config| config.desktop.notifications)
            .unwrap_or(false);
        let notifications = NotificationService::new(notifications_enabled);
        let live = cx.new(|cx| Live::new(client.clone(), cx));
        let send = cx.new(|cx| SendPanel::new(live.clone(), window, cx));
        let receive = cx.new(|cx| ReceivePanel::new(client.clone(), home.clone(), window, cx));
        let transfers = cx.new(|cx| TransfersPanel::new(client.clone(), home.clone(), window, cx));
        let inbox = cx.new(|cx| InboxPanel::new(client.clone(), window, cx));
        let history = cx.new(|cx| HistoryPanel::new(client.clone(), window, cx));
        let account = cx.new(|cx| AccountPanel::new(client.clone(), window, cx));
        let settings = cx.new(|cx| SettingsPanel::new(client.clone(), home, window, cx));
        // The tray and notification adapters exist before the settings page asks
        // whether they can be offered.
        if let Some(availability) = presence
            .as_ref()
            .map(|presence| presence.availability().clone())
        {
            settings.update(cx, |settings, cx| {
                settings.set_capabilities(availability, true, cx);
            });
        }
        let command_palette = cx.new(|cx| CommandState::new(window, cx));
        let policy_fingerprint = PolicyFingerprint::from(&client.snapshot().policy);

        let subscriptions = vec![
            cx.observe_in(&live, window, |shell, live, window, cx| {
                let snapshot = live.read(cx).snapshot();
                shell.on_snapshot(&snapshot, window, cx);
            }),
            // "Follow system" tracks the operating system while the app runs.
            cx.observe_window_appearance(window, |_, _, cx| theme::follow_system(cx)),
        ];

        // Native presence and single-instance activations arrive on channels rather
        // than through the snapshot, so they keep a small poll of their own.
        let events = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
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
                        shell
                            .notification_acknowledgements
                            .retain(|acknowledgement| {
                                matches!(
                                    acknowledgement.try_recv(),
                                    Err(std::sync::mpsc::TryRecvError::Empty)
                                )
                            });
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        let mut shell = Self {
            client,
            live,
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
            presence,
            notifications,
            notification_acknowledgements: Vec::new(),
            seen_message_ids: HashSet::new(),
            completed_jobs: HashSet::new(),
            completion_baselined: false,
            policy_fingerprint,
            _subscriptions: subscriptions,
            _events: events,
        };
        shell.route_inputs(inputs, window, cx);
        shell
    }

    /// Reacts to a changed service snapshot: failures become toasts, policy changes
    /// reach the composer, and finished transfers raise a system notification.
    fn on_snapshot(
        &mut self,
        snapshot: &DesktopSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for message in
            crate::toast::take_operational_messages(&mut self.seen_message_ids, &snapshot.messages)
        {
            crate::toast::error(
                format::failure_title(&message.operation),
                format::readable_error(&message.error.detail),
                window,
                cx,
            );
        }
        let policy_fingerprint = PolicyFingerprint::from(&snapshot.policy);
        if policy_fingerprint != self.policy_fingerprint {
            self.policy_fingerprint = policy_fingerprint;
            self.send
                .update(cx, |send, cx| send.refresh_policy(window, cx));
        }
        let completed: HashSet<_> = snapshot
            .jobs
            .iter()
            .filter(|job| matches!(job.state, TransferState::Complete))
            .map(|job| job.id.clone())
            .collect();
        if self.completion_baselined {
            for _ in completed.difference(&self.completed_jobs) {
                if let Some(acknowledgement) = self.notifications.transfer_completed() {
                    self.notification_acknowledgements.push(acknowledgement);
                }
            }
        }
        self.completed_jobs = completed;
        self.completion_baselined = true;
        cx.notify();
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
                Err(error) => {
                    crate::toast::error(
                        "Could not open that",
                        format::readable_error(&error.to_string()),
                        window,
                        cx,
                    );
                }
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
        window.open_dialog(cx, move |dialog, _, cx| {
            let p = theme::palette(cx);
            let accept = shell.clone();
            let keep = shell.clone();
            dialog.title("Open this transfer?").child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .child(
                        text::body(
                            "Filebeam was asked to open a transfer link. Opening it replaces \
                             what is currently entered on Receive.",
                        )
                        .text_color(p.text_muted),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                button::quiet("keep-receive-draft")
                                    .label("Keep current entry")
                                    .on_click(move |_, window, cx| {
                                        let _ = keep.update(cx, |shell, _| {
                                            shell.pending_receive_link = None
                                        });
                                        window.close_dialog(cx);
                                    }),
                            )
                            .child(
                                button::primary("open-queued-receive")
                                    .label("Open transfer")
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
                            ),
                    ),
            )
        });
    }

    pub fn select(&mut self, destination: Destination, cx: &mut Context<Self>) {
        self.destination = destination;
        let command = match destination {
            Destination::Inbox => Some(ClientCommand::RefreshInbox),
            Destination::History => Some(ClientCommand::RefreshHistory {
                filter: self.client.snapshot().history.filter,
                cursor: None,
            }),
            Destination::Account => Some(ClientCommand::Refresh),
            _ => None,
        };
        if let Some(command) = command {
            let _ = self.client.dispatch(command);
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
            dialog.title("Go to").child(
                Command::new(&palette)
                    .placeholder("Search pages…")
                    .group(
                        CommandGroup::new()
                            .label("Pages")
                            .items(Destination::ALL.iter().map(|destination| {
                                CommandItem::new()
                                    .label(destination.label())
                                    .icon(icon(destination.icon()))
                            })),
                    )
                    .empty(|_, _, cx| {
                        let p = theme::palette(cx);
                        div()
                            .p(px(16.))
                            .child(text::small("Nothing matches that.").text_color(p.text_muted))
                    })
                    .on_confirm(move |index, window, cx| {
                        if let Some(destination) = Destination::ALL.get(index.row) {
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

    fn nav_item(
        &self,
        destination: Destination,
        compact: bool,
        badge: Option<usize>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let selected = self.destination == destination;
        let ink = if selected { p.text } else { p.text_muted };
        let label = destination.label();
        let accessible = badge.map_or_else(
            || label.to_owned(),
            |count| format!("{label}, {count} waiting"),
        );
        let hint = destination.shortcut().map_or_else(
            || label.to_owned(),
            |key| format!("{label}  ·  {} {key}", modifier_label()),
        );
        BaseButton::new(SharedString::from(format!("nav-{}", destination.id())))
            .accessibility_label(accessible)
            .selected(selected)
            .relative()
            .w_full()
            .h(px(NAV_HEIGHT))
            .flex_none()
            .rounded(px(radius::CONTROL))
            .gap(px(11.))
            .when(!compact, |this| this.justify_start().px(px(12.)))
            .text_color(ink)
            .when(!selected, |this| {
                this.cursor_pointer()
                    .hover(move |style| style.bg(p.hover).text_color(p.text))
                    .active(move |style| style.bg(p.active))
            })
            .focus_visible(move |style| style.bg(p.hover))
            .child(
                icon(destination.icon())
                    .size(px(18.))
                    .flex_none()
                    .text_color(if selected { p.accent_text } else { ink }),
            )
            .when(!compact, |this| {
                this.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(size::BODY))
                        .font_weight(if selected {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::MEDIUM
                        })
                        .line_height(relative(1.))
                        .single_line()
                        .child(label),
                )
                .when_some(badge, |this, count| {
                    this.child(feedback::count(p, count, false))
                })
            })
            .when(compact, |this| {
                this.when_some(badge, |this, _| {
                    this.child(
                        div()
                            .absolute()
                            .top(px(9.))
                            .right(px(13.))
                            .child(feedback::dot(p.accent_text)),
                    )
                })
                .tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
            })
            .on_click(cx.listener(move |this, _, _, cx| this.select(destination, cx)))
    }

    fn sidebar(
        &mut self,
        snapshot: &DesktopSnapshot,
        compact: bool,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let transfer_count = snapshot
            .jobs
            .iter()
            .filter(|job| is_active(job.state) || matches!(job.state, TransferState::Paused))
            .count();
        let inbox_count = snapshot.inbox.len();
        let selected_index = Destination::PRIMARY
            .iter()
            .position(|destination| *destination == self.destination);
        // The pill rests on the last primary page while Settings or Account is open,
        // so it fades rather than jumping when the user comes back.
        let resting = selected_index.unwrap_or(0) as f32 * (NAV_HEIGHT + NAV_GAP);
        let pill_top = motion::travel("nav-pill", px(resting), motion::SPATIAL, window, cx);
        let pill_opacity = motion::eased(
            "nav-pill-opacity",
            if selected_index.is_some() { 1. } else { 0. },
            motion::fade(),
            window,
            cx,
        );

        let active_job = snapshot
            .jobs
            .iter()
            .find(|job| is_active(job.state) || matches!(job.state, TransferState::Paused))
            .map(|job| {
                let fraction = job
                    .progress
                    .total_bytes
                    .filter(|total| *total > 0)
                    .map(|total| {
                        (job.progress.completed_bytes as f32 / total as f32).clamp(0., 1.)
                    });
                let state = match job.state {
                    TransferState::PauseRequested => "Pausing".to_owned(),
                    TransferState::Paused => "Paused".to_owned(),
                    _ if job.progress.phase.is_empty() => "In progress".to_owned(),
                    _ => job.progress.phase.clone(),
                };
                (job.progress.name.clone(), state, fraction, job.state)
            });

        let signed_in = snapshot.account.authenticated;
        let account_name = snapshot
            .account
            .username
            .clone()
            .or_else(|| snapshot.account.email.clone())
            .unwrap_or_else(|| "Sign in".into());
        let host = format::host(&snapshot.instance.url);
        let connected = snapshot.instance.connected;
        let account_selected = self.destination == Destination::Account;
        let settings_selected = self.destination == Destination::Settings;

        let selection_pill = |p: Palette| {
            div()
                .rounded(px(radius::CONTROL))
                .bg(p.selected_surface)
                .border_1()
                .border_color(p.choice_border.opacity(0.55))
        };

        div()
            .w(px(if compact {
                geometry::SIDEBAR_RAIL
            } else {
                geometry::SIDEBAR
            }))
            .flex_none()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(p.chrome)
            .border_r_1()
            .border_color(p.line_soft)
            .px(px(if compact { 12. } else { 14. }))
            .pt(px(14.))
            .pb(px(12.))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .gap(px(NAV_GAP))
                    .child(
                        selection_pill(p)
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(pill_top)
                            .h(px(NAV_HEIGHT))
                            .opacity(pill_opacity),
                    )
                    .children(Destination::PRIMARY.map(|destination| {
                        let badge = match destination {
                            Destination::Transfers => {
                                (transfer_count > 0).then_some(transfer_count)
                            }
                            Destination::Inbox => (inbox_count > 0).then_some(inbox_count),
                            _ => None,
                        };
                        self.nav_item(destination, compact, badge, p, cx)
                    })),
            )
            .child(div().flex_1())
            .when(!compact, |this| {
                this.when_some(active_job, |this, (name, state, fraction, job_state)| {
                    let tone = if matches!(job_state, TransferState::Paused) {
                        p.warning.ink
                    } else {
                        p.accent_text
                    };
                    this.child(
                        div()
                            .id("active-transfer-pocket")
                            .mb(px(10.))
                            .p(px(12.))
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .rounded(px(radius::CONTROL + 2.))
                            .bg(p.surface)
                            .border_1()
                            .border_color(p.border)
                            .cursor_pointer()
                            .hover(move |style| style.border_color(p.control_border))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(7.))
                                    .child(feedback::dot(tone))
                                    .child(text::caption(state).single_line().text_color(tone)),
                            )
                            .child(text::small_strong(name).single_line().text_color(p.text))
                            .child(progress("pocket-progress", fraction).height(4.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.select(Destination::Transfers, cx)
                            })),
                    )
                })
            })
            .child(
                div()
                    .relative()
                    .mb(px(8.))
                    .when(settings_selected, |this| {
                        this.child(selection_pill(p).absolute().inset_0())
                    })
                    .child(self.nav_item(Destination::Settings, compact, None, p, cx)),
            )
            .child(
                div()
                    .id("account-profile")
                    .aria_label(if signed_in {
                        format!("Account, signed in as {account_name}")
                    } else {
                        "Account, sign in".to_owned()
                    })
                    .flex()
                    .items_center()
                    .when(compact, |this| this.justify_center())
                    .gap(px(10.))
                    .p(px(if compact { 6. } else { 8. }))
                    .rounded(px(radius::CONTROL + 2.))
                    .border_1()
                    .when(account_selected, |this| {
                        this.bg(p.selected_surface)
                            .border_color(p.choice_border.opacity(0.55))
                    })
                    .when(!account_selected, |this| {
                        this.border_color(p.line_soft)
                            .cursor_pointer()
                            .hover(move |style| style.bg(p.hover))
                            .active(move |style| style.bg(p.active))
                    })
                    .child(if signed_in {
                        brand::avatar(p, &account_name, 32.)
                    } else {
                        brand::avatar_placeholder(p, 32.)
                    })
                    .when(!compact, |this| {
                        this.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(3.))
                                .child(
                                    text::small_strong(account_name.clone())
                                        .single_line()
                                        .text_color(p.text),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.))
                                        .child(feedback::dot(if connected {
                                            p.success.ink
                                        } else {
                                            p.warning.ink
                                        }))
                                        .child(
                                            text::caption(host.clone())
                                                .single_line()
                                                .text_color(p.text_muted),
                                        ),
                                ),
                        )
                    })
                    .tooltip(move |window, cx| {
                        Tooltip::new(if connected {
                            format!("Connected to {host}")
                        } else {
                            format!("{host} is not reachable")
                        })
                        .build(window, cx)
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.select(Destination::Account, cx))),
            )
    }
}

const NAV_HEIGHT: f32 = 40.;
const NAV_GAP: f32 = 4.;

impl Render for DesktopShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let compact = ui::page::is_compact(window);
        let snapshot = self.live.read(cx).snapshot();
        let sidebar_width = if compact {
            geometry::SIDEBAR_RAIL
        } else {
            geometry::SIDEBAR
        };
        let working = snapshot.jobs.iter().any(|job| is_active(job.state));
        let energy = motion::eased(
            "ambient-energy",
            if working { 1. } else { 0. },
            motion::reveal(),
            window,
            cx,
        );
        let page_entrance = motion::entrance_with(
            ("workspace", self.destination.id()),
            0,
            8.,
            motion::enter_quick(),
            window,
            cx,
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
        let navigation = self.sidebar(&snapshot, compact, p, window, cx);

        div()
            .key_context("Filebeam")
            .size_full()
            .flex()
            .flex_col()
            .bg(p.background)
            .text_color(p.text)
            .font_family(text::SANS)
            .text_size(px(size::BODY))
            .child(
                TitleBar::new().h(px(geometry::TITLEBAR_HEIGHT)).child(
                    div()
                        .flex()
                        .items_center()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .w(px(sidebar_width - 14.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .when(compact, |this| this.justify_center())
                                .gap(px(9.))
                                .child(brand::mark(p, 22.))
                                .when(!compact, |this| {
                                    this.child(
                                        div()
                                            .font_family(text::DISPLAY)
                                            .text_size(px(17.))
                                            .font_weight(FontWeight::BOLD)
                                            .line_height(relative(1.))
                                            .text_color(p.text)
                                            .child("Filebeam"),
                                    )
                                }),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .id("commands")
                                .aria_label("Search pages")
                                .w(px(if compact { 190. } else { 260. }))
                                .h(px(30.))
                                .mr(px(12.))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .px(px(10.))
                                .rounded(px(radius::SMALL))
                                .bg(p.sunken)
                                .border_1()
                                .border_color(p.border)
                                .cursor_pointer()
                                .hover(move |style| style.border_color(p.control_border))
                                .child(icon("search").size(px(14.)).text_color(p.text_subtle))
                                .child(
                                    text::small("Go to…")
                                        .flex_1()
                                        .single_line()
                                        .text_color(p.text_subtle),
                                )
                                .child(
                                    div()
                                        .px(px(6.))
                                        .py(px(2.))
                                        .rounded(px(5.))
                                        .bg(p.hover)
                                        .child(
                                            text::caption(format!("{} K", modifier_label()))
                                                .text_color(p.text_muted),
                                        ),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_palette(&OpenCommandPalette, window, cx)
                                })),
                        ),
                ),
            )
            .child(
                div().flex().flex_1().min_h_0().child(navigation).child(
                    div()
                        .relative()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .child(brand::ambient(p, energy))
                        // A finite viewport, not a shell-owned scroller: retained pages
                        // own their scroll handles, insets and scrollbar placement.
                        .child(
                            div()
                                .relative()
                                .size_full()
                                .min_h_0()
                                .flex()
                                .flex_col()
                                .entering(page_entrance)
                                .child(workspace),
                        ),
                ),
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
                cx.listener(|this, _: &OpenHistory, _, cx| this.select(Destination::History, cx)),
            )
            .on_action(
                cx.listener(|this, _: &OpenSettings, _, cx| this.select(Destination::Settings, cx)),
            )
            .on_action(
                cx.listener(|this, _: &OpenAccount, _, cx| this.select(Destination::Account, cx)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destinations_round_trip_through_their_identifiers() {
        for destination in Destination::ALL {
            assert_eq!(Destination::from_id(destination.id()), Some(destination));
        }
        assert_eq!(Destination::from_id("nowhere"), None);
        assert!(
            Destination::PRIMARY
                .iter()
                .all(|d| Destination::ALL.contains(d))
        );
    }
}
