//! Account: signing in, identity, receiving preferences, the receiving key and
//! contacts. Secrets are typed into retained inputs and handed straight to the
//! service; none is copied into view state or drawn.

mod auth;
mod contacts;
mod keys;
mod profile;
mod receiving;

use std::{sync::Arc, time::Duration};

use gpui::{
    AnyElement, AppContext, ClipboardItem, Context, Entity, IntoElement, ParentElement, Render,
    ScrollHandle, Styled, Window, div, px,
};
use gpui_component::{
    IndexPath,
    input::{InputEvent, InputState, TextareaState},
    select::SelectState,
};

use crate::{
    client::DesktopClient,
    live::Live,
    model::{ClientCommand, DesktopSnapshot},
    request::{Outcome, Request},
    theme::{self, Palette, tokens::geometry},
    ui::{
        MotionExt, Tone, button, feedback, format, motion, page,
        rail::{Segment, rail},
    },
};

/// The sections of a signed-in account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Profile,
    Receiving,
    Key,
    Contacts,
}

impl Tab {
    const ALL: [Self; 4] = [Self::Profile, Self::Receiving, Self::Key, Self::Contacts];

    const fn label(self) -> &'static str {
        match self {
            Self::Profile => "Profile",
            Self::Receiving => "Receiving",
            Self::Key => "Key",
            Self::Contacts => "Contacts",
        }
    }

    const fn icon(self) -> &'static str {
        match self {
            Self::Profile => "user",
            Self::Receiving => "archive",
            Self::Key => "key",
            Self::Contacts => "users",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthMode {
    SignIn,
    Register,
    Reset,
}

/// Where the result of a request is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Area {
    Auth,
    Profile,
    Receiving,
    Key,
    Contacts,
    Report,
    Delete,
}

fn area(operation: &str) -> Area {
    match operation {
        "login" | "register" | "request password reset" => Area::Auth,
        "delete account" => Area::Delete,
        "report transfer" => Area::Report,
        "set inbox enabled"
        | "set notification channel"
        | "receiving defaults"
        | "automatic receiving" => Area::Receiving,
        "generate receiving key"
        | "import receiving key"
        | "unlock receiving key"
        | "export receiving key"
        | "confirm receiving-key backup" => Area::Key,
        "refresh contacts" | "update contact" => Area::Contacts,
        _ => Area::Profile,
    }
}

/// Why a request was refused, in words that fit what was being attempted.
fn failure(operation: &str, detail: &str) -> String {
    let status = format::status_code(detail);
    match (operation, status) {
        ("login", Some(401 | 422)) => "That email and password do not match an account.".into(),
        ("register", Some(422)) => "Those details were not accepted. The username or email may \
                                    already be taken, or the password too short."
            .into(),
        ("register", Some(403)) => "This instance is not accepting new accounts.".into(),
        ("delete account", Some(403 | 422)) => {
            "That password was not accepted, so nothing was deleted.".into()
        }
        ("report transfer", Some(404 | 422)) => {
            "That report was not accepted. Check the transfer ID and describe the problem in a \
             sentence or two."
                .into()
        }
        ("update contact", Some(404)) => "Nobody on this instance has that username.".into(),
        ("update contact", Some(409 | 422)) => {
            "That could not be done for this person right now.".into()
        }
        ("unlock receiving key" | "generate receiving key" | "import receiving key", _)
            if detail.contains("withheld") =>
        {
            "That was not accepted. Check the password and try again.".into()
        }
        _ => format::readable_error(detail),
    }
}

/// What a finished request is worth saying, if anything.
fn confirmation(operation: &str) -> Option<&'static str> {
    match operation {
        "request password reset" => {
            Some("If that address has an account, a reset link is on its way.")
        }
        "resend verification" => Some("Verification email sent. Check your inbox."),
        "report transfer" => Some("Report sent. Thank you for telling us."),
        "export receiving key" => Some("Backup saved. Keep it somewhere only you can reach."),
        "import receiving key" => Some("Key restored on this device."),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Notice {
    area: Area,
    tone: Tone,
    text: String,
}

/// A switch or choice that was just changed, until the service confirms or refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Intended {
    control: &'static str,
    operation: &'static str,
    value: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Copied {
    Address,
    Fingerprint,
}

const REPORT_CATEGORIES: [(&str, &str); 6] = [
    ("spam", "Spam"),
    ("malware", "Malware or phishing"),
    ("illegal_content", "Illegal content"),
    ("privacy", "Privacy violation"),
    ("copyright", "Copyright"),
    ("other", "Something else"),
];

/// Retained account forms and account-scoped presentation state.
pub struct AccountPanel {
    client: Arc<DesktopClient>,
    live: Entity<Live>,
    tab: Tab,
    auth: AuthMode,
    email: Entity<InputState>,
    username: Entity<InputState>,
    name: Entity<InputState>,
    password: Entity<InputState>,
    /// The account or key password a key action asks for.
    key_password: Entity<InputState>,
    key_import: Entity<InputState>,
    contact: Entity<InputState>,
    report_id: Entity<InputState>,
    report_description: Entity<TextareaState>,
    report_email: Entity<InputState>,
    report_category: Entity<SelectState<Vec<&'static str>>>,
    /// The password and typed confirmation a deletion or key replacement needs.
    confirm_password: Entity<InputState>,
    confirm_phrase: Entity<InputState>,
    requests: Vec<Request>,
    /// Values a control shows while the change it asked for is still being made.
    intended: Vec<Intended>,
    notice: Option<Notice>,
    /// What to say once the contact request that was just sent goes through.
    contact_feedback: Option<String>,
    /// The friend whose overrides are open.
    expanded: Option<String>,
    was_signed_in: bool,
    copied: Option<Copied>,
    copy_generation: u64,
    page_scroll: ScrollHandle,
}

impl AccountPanel {
    pub fn new(live: Entity<Live>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client = live.read(cx).client().clone();
        let field = |placeholder: &'static str,
                     masked: bool,
                     window: &mut Window,
                     cx: &mut Context<Self>| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(masked)
            })
        };
        let email = field("you@example.com", false, window, cx);
        let username = field("username", false, window, cx);
        let name = field("How your name appears", false, window, cx);
        let password = field("Password", true, window, cx);
        let key_password = field("Password", true, window, cx);
        let key_import = field("Paste your key backup", true, window, cx);
        let contact = field("username", false, window, cx);
        let report_id = field("https://… or a transfer ID", false, window, cx);
        let report_description = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder("What is wrong with this transfer?")
        });
        let report_email = field("you@example.com", false, window, cx);
        let report_category = cx.new(|cx| {
            SelectState::new(
                REPORT_CATEGORIES
                    .iter()
                    .map(|(_, label)| *label)
                    .collect::<Vec<_>>(),
                Some(IndexPath::default()),
                window,
                cx,
            )
        });
        let confirm_password = field("Current password", true, window, cx);
        let confirm_phrase = field("DELETE", false, window, cx);

        // Enter submits the form a field belongs to; typing clears its stale error.
        for input in [&email, &username, &name, &password] {
            cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => this.clear_notice(Area::Auth, cx),
                    InputEvent::PressEnter { .. } => this.submit_auth(window, cx),
                    _ => {}
                },
            )
            .detach();
        }
        cx.subscribe_in(
            &contact,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => this.clear_notice(Area::Contacts, cx),
                InputEvent::PressEnter { .. } => this.request_contact(window, cx),
                _ => {}
            },
        )
        .detach();
        cx.subscribe_in(
            &key_password,
            window,
            |this, _, event: &InputEvent, _, cx| match event {
                InputEvent::Change => this.clear_notice(Area::Key, cx),
                InputEvent::PressEnter { .. } => this.submit_key_password(cx),
                _ => {}
            },
        )
        .detach();
        cx.observe_in(&live, window, |this, live, window, cx| {
            let snapshot = live.read(cx).snapshot();
            this.on_snapshot(&snapshot, window, cx);
        })
        .detach();

        let was_signed_in = live.read(cx).snapshot().account.authenticated;
        Self {
            client,
            live,
            tab: Tab::Profile,
            auth: AuthMode::SignIn,
            email,
            username,
            name,
            password,
            key_password,
            key_import,
            contact,
            report_id,
            report_description,
            report_email,
            report_category,
            confirm_password,
            confirm_phrase,
            requests: Vec::new(),
            intended: Vec::new(),
            notice: None,
            contact_feedback: None,
            expanded: None,
            was_signed_in,
            copied: None,
            copy_generation: 0,
            page_scroll: ScrollHandle::new(),
        }
    }

    /// Reloads the session, key state and contacts. The shell calls this when the
    /// page opens.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let _ = self.client.dispatch(ClientCommand::Refresh);
        cx.notify();
    }

    /// Opens the key section, for pages that need the receiving key set up.
    pub fn show_key(&mut self, cx: &mut Context<Self>) {
        self.tab = Tab::Key;
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

    fn secret(input: &Entity<InputState>, cx: &Context<Self>) -> String {
        input.read(cx).value().to_string()
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

    /// The notice for `area`, drawn where that area lives.
    fn notice(&self, area: Area, p: Palette) -> Option<AnyElement> {
        self.notice
            .as_ref()
            .filter(|notice| notice.area == area)
            .map(|notice| feedback::notice(p, notice.text.clone(), notice.tone).into_any_element())
    }

    /// What `control` shows: the value just chosen, or the service's once it settles.
    fn shown(&self, control: &'static str, actual: usize) -> usize {
        self.intended
            .iter()
            .find(|intended| intended.control == control)
            .map_or(actual, |intended| intended.value)
    }

    /// Sends a change and shows its result straight away instead of snapping back.
    fn change(
        &mut self,
        control: &'static str,
        value: usize,
        command: ClientCommand,
        cx: &mut Context<Self>,
    ) {
        let operation = command.operation();
        if self.waiting(operation) {
            return;
        }
        self.intended.retain(|intended| intended.control != control);
        self.send(command, cx);
        if self.waiting(operation) {
            self.intended.push(Intended {
                control,
                operation,
                value,
            });
        }
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

    /// Settles answered requests. Secrets are cleared as soon as they have been
    /// used, and whenever the signed-in account changes.
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
            self.intended
                .retain(|intended| intended.operation != operation);
            // Reports and deletions are confirmed in a dialog that has closed by
            // now, so their outcome is shown on the page behind it.
            let shown = match area(operation) {
                Area::Report | Area::Delete if snapshot.account.authenticated => Area::Profile,
                Area::Report | Area::Delete => Area::Auth,
                other => other,
            };
            match outcome {
                Outcome::Failed(detail) => {
                    self.contact_feedback = None;
                    // A refused password is never left sitting in its field.
                    if matches!(operation, "delete account" | "generate receiving key") {
                        Self::clear(&self.confirm_password.clone(), window, cx);
                        Self::clear(&self.confirm_phrase.clone(), window, cx);
                    }
                    self.say(shown, Tone::Danger, failure(operation, &detail), cx);
                }
                Outcome::Completed => {
                    match operation {
                        "login" | "register" | "delete account" => {
                            for input in
                                [&self.password, &self.confirm_password, &self.confirm_phrase]
                            {
                                Self::clear(&input.clone(), window, cx);
                            }
                        }
                        "generate receiving key" | "unlock receiving key" => {
                            Self::clear(&self.key_password.clone(), window, cx);
                            Self::clear(&self.confirm_password.clone(), window, cx);
                        }
                        "import receiving key" => {
                            Self::clear(&self.key_import.clone(), window, cx);
                            Self::clear(&self.key_password.clone(), window, cx);
                        }
                        "update contact" => {
                            if let Some(text) = self.contact_feedback.take() {
                                Self::clear(&self.contact.clone(), window, cx);
                                self.say(Area::Contacts, Tone::Success, text, cx);
                            }
                        }
                        "report transfer" => {
                            Self::clear(&self.report_id.clone(), window, cx);
                            self.report_description
                                .update(cx, |input, cx| input.set_value("", window, cx));
                        }
                        _ => {}
                    }
                    if let Some(text) = confirmation(operation) {
                        self.say(shown, Tone::Success, text, cx);
                    }
                    // Key changes are read back from the service rather than assumed.
                    if area(operation) == Area::Key {
                        let _ = self.client.dispatch(ClientCommand::Refresh);
                    }
                }
                Outcome::Waiting => {}
            }
        }

        let signed_in = snapshot.account.authenticated;
        if signed_in != self.was_signed_in {
            self.was_signed_in = signed_in;
            self.tab = Tab::Profile;
            self.auth = AuthMode::SignIn;
            if self
                .notice
                .as_ref()
                .is_some_and(|notice| notice.tone == Tone::Danger)
            {
                self.notice = None;
            }
            for input in [
                &self.password,
                &self.key_password,
                &self.key_import,
                &self.confirm_password,
                &self.confirm_phrase,
            ] {
                Self::clear(&input.clone(), window, cx);
            }
        }
        cx.notify();
    }

    fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
            if tab == Tab::Contacts || tab == Tab::Receiving {
                self.send(ClientCommand::RefreshContacts, cx);
            }
            cx.notify();
        }
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

impl Render for AccountPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let snapshot = self.snapshot(cx);
        if !snapshot.account.authenticated {
            let form = self.auth_page(&snapshot, p, window, cx);
            return page::scroller(
                "account-page-scroll",
                &self.page_scroll,
                440.,
                true,
                window,
                form,
            )
            .into_any_element();
        }

        let host = format::host(&snapshot.instance.url);
        let signing_out = self.waiting("logout");
        let header = page::header_with(
            p,
            "Account",
            format!("Your identity and inbox on {host}."),
            button::secondary("account-logout")
                .icon("logout")
                .label("Sign out")
                .loading(signing_out)
                .on_click(cx.listener(|this, _, _, cx| {
                    if !this.waiting("logout") {
                        this.send(ClientCommand::Logout, cx);
                    }
                })),
            window,
            cx,
        );
        let panel = cx.entity().downgrade();
        let tabs = rail(
            "account-tabs",
            "Account section",
            Tab::ALL
                .iter()
                .position(|tab| *tab == self.tab)
                .unwrap_or(0),
        )
        .segment_width(124.)
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
            Tab::Profile => self.profile(&snapshot, p, window, cx),
            Tab::Receiving => self.receiving(&snapshot, p, window, cx),
            Tab::Key => self.keys(&snapshot, p, window, cx),
            Tab::Contacts => self.contacts(&snapshot, p, window, cx),
        };
        let tabs_entrance = motion::entrance("account-tabs", 1, window, cx);
        let body_entrance = motion::entrance(("account-body", self.tab.label()), 2, window, cx);
        page::scroller(
            "account-page-scroll",
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
        .into_any_element()
    }
}

#[cfg(feature = "visual-test")]
impl AccountPanel {
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
            "account-register" => self.auth = AuthMode::Register,
            "account-recovery" => self.auth = AuthMode::Reset,
            "account-auth-refused" => {
                self.email.update(cx, |input, cx| {
                    input.set_value("mason@example.invalid", window, cx)
                });
                self.say(
                    Area::Auth,
                    Tone::Danger,
                    failure("login", "native login returned 422 Unprocessable Entity"),
                    cx,
                );
            }
            "account-receiving" => self.tab = Tab::Receiving,
            "account-contacts" => {
                self.tab = Tab::Contacts;
                self.expanded = Some("grace".into());
            }
            scenario if scenario.starts_with("account-key") => self.tab = Tab::Key,
            _ => {}
        }
        cx.notify();
    }

    pub fn open_report_for_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_report(window, cx);
    }

    pub fn open_delete_for_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.confirm_delete(window, cx);
    }
}

#[cfg(test)]
mod tests;
