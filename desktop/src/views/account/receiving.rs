//! Who may send to this account, what is fetched automatically and how it is announced.

use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, Window, div, px};

use super::{AccountPanel, Area};
use crate::{
    model::{ClientCommand, DesktopSnapshot, NotificationChannel},
    theme::Palette,
    ui::{
        Tone, feedback, field,
        rail::{Segment, rail},
        row, surface, text,
    },
};

/// The receiving policies the service knows, with what each means for the person.
const POLICIES: [(&str, &str, &str); 4] = [
    (
        "anyone",
        "Anyone",
        "Anyone with your receiving link can send to you.",
    ),
    (
        "authenticated",
        "Signed in",
        "Only people signed in to this instance can send to you.",
    ),
    ("friends", "Friends", "Only your friends can send to you."),
    (
        "nobody",
        "Nobody",
        "Nobody can send to you unless you allow them under Contacts.",
    ),
];

impl AccountPanel {
    pub(super) fn receiving(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let account = &snapshot.account;
        let handle = account
            .username
            .as_ref()
            .map_or_else(|| "your username".to_owned(), |name| format!("@{name}"));
        let inbox_on = self.shown("inbox", usize::from(account.inbox_enabled)) == 1;
        let device_on = self.shown("device", usize::from(snapshot.auto_receiving)) == 1;
        let by_mail = account.notification_channel == Some(NotificationChannel::Mail);
        let channel = self.shown("channel", usize::from(by_mail));

        let who = match &snapshot.contacts {
            Some(contacts) => {
                let actual = POLICIES
                    .iter()
                    .position(|(value, ..)| *value == contacts.settings.receiving_policy)
                    .unwrap_or(0);
                let policy = self.shown("policy", actual);
                let friends_on = self.shown(
                    "friends",
                    usize::from(contacts.settings.auto_download_friends),
                ) == 1;
                let (choose, toggle) = (cx.entity().downgrade(), cx.entity().downgrade());
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(field::label(p, "Who can send to me", false))
                    .child(
                        rail("receiving-policy", "Who can send to me", policy)
                            .fill()
                            .segments(POLICIES.iter().map(|(_, label, _)| Segment::new(*label)))
                            .on_select(move |index, _, cx| {
                                let _ = choose.update(cx, |this, cx| {
                                    this.change(
                                        "policy",
                                        index,
                                        ClientCommand::ReceivingDefaults {
                                            policy: POLICIES[index.min(3)].0.into(),
                                            auto_download: friends_on,
                                        },
                                        cx,
                                    );
                                });
                            }),
                    )
                    .child(field::hint(p, POLICIES[policy.min(3)].2, Tone::Neutral))
                    .child(surface::hairline(p))
                    .child(
                        row::switch_row(
                            "auto-friends",
                            "Fetch from friends without asking",
                            friends_on,
                        )
                        .description(
                            "Deliveries from friends are marked for automatic download. They \
                             stay encrypted until you save them.",
                        )
                        .on_toggle(move |on, _, cx| {
                            let on = *on;
                            let _ = toggle.update(cx, |this, cx| {
                                this.change(
                                    "friends",
                                    usize::from(on),
                                    ClientCommand::ReceivingDefaults {
                                        policy: POLICIES[policy.min(3)].0.into(),
                                        auto_download: on,
                                    },
                                    cx,
                                );
                            });
                        }),
                    )
            }
            None => div()
                .flex()
                .items_center()
                .gap(px(10.))
                .py(px(6.))
                .child(feedback::spinner(p.text_muted, 14.))
                .child(text::small("Loading who can send to you…").text_color(p.text_muted)),
        };

        let (inbox, device, notify) = (
            cx.entity().downgrade(),
            cx.entity().downgrade(),
            cx.entity().downgrade(),
        );
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .children(self.notice(Area::Receiving, p))
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(text::eyebrow("Inbox").text_color(p.text_subtle))
                    .child(
                        row::switch_row("inbox-enabled", "Accept deliveries", inbox_on)
                            .description(format!(
                                "Let people send encrypted files and notes to {handle}."
                            ))
                            .on_toggle(move |on, _, cx| {
                                let on = *on;
                                let _ = inbox.update(cx, |this, cx| {
                                    this.change(
                                        "inbox",
                                        usize::from(on),
                                        ClientCommand::SetInboxEnabled(on),
                                        cx,
                                    );
                                });
                            }),
                    )
                    .child(surface::hairline(p))
                    .child(who),
            )
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(text::eyebrow("This device").text_color(p.text_subtle))
                    .child(
                        row::switch_row(
                            "auto-this-client",
                            "Download in the background",
                            device_on,
                        )
                        .description(
                            "This desktop fetches deliveries marked for automatic download \
                                 and holds them, still encrypted, in your inbox.",
                        )
                        .on_toggle(move |on, _, cx| {
                            let on = *on;
                            let _ = device.update(cx, |this, cx| {
                                this.change(
                                    "device",
                                    usize::from(on),
                                    ClientCommand::AutomaticReceiving(on),
                                    cx,
                                );
                            });
                        }),
                    ),
            )
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(text::eyebrow("New delivery notices").text_color(p.text_subtle))
                    .child(
                        div().flex().child(
                            rail(
                                "notification-channel",
                                "How to hear about deliveries",
                                channel,
                            )
                            .segment_width(132.)
                            .segment(Segment::new("In the app"))
                            .segment(Segment::new("By email"))
                            .on_select(move |index, _, cx| {
                                let _ = notify.update(cx, |this, cx| {
                                    this.change(
                                        "channel",
                                        index,
                                        ClientCommand::SetNotificationChannel(if index == 1 {
                                            NotificationChannel::Mail
                                        } else {
                                            NotificationChannel::Database
                                        }),
                                        cx,
                                    );
                                });
                            }),
                        ),
                    )
                    .child(field::hint(
                        p,
                        if channel == 1 {
                            "The instance emails you when something arrives."
                        } else {
                            "Deliveries appear in your inbox on every device you are signed in to."
                        },
                        Tone::Neutral,
                    )),
            )
            .into_any_element()
    }
}
