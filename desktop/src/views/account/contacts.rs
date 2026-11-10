//! Friends on this instance, requests in both directions and people who are blocked.

use filebeam_client_core::services::{Contact, Contacts};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use super::{AccountPanel, Area};
use crate::{
    assets::icon,
    model::{ClientCommand, DesktopSnapshot},
    theme::{Palette, tokens::radius},
    ui::{
        TextExt, Tone, brand, button, dialog, feedback, field,
        rail::{Segment, rail},
        surface, text,
    },
};

/// What a friend may do, as one line: the effective result of defaults and overrides.
fn summary(contact: &Contact) -> String {
    match (contact.effective.can_send, contact.effective.auto_download) {
        (false, _) => "Cannot send to you".into(),
        (true, true) => "Can send to you · fetched automatically".into(),
        (true, false) => "Can send to you".into(),
    }
}

/// Turns a chosen override into the pair the service stores for a contact.
type Apply = fn(Option<bool>, &Contact) -> (Option<bool>, Option<bool>);

/// A three-way override as a segment index: follow the default, force on, force off.
fn override_index(value: Option<bool>) -> usize {
    match value {
        None => 0,
        Some(true) => 1,
        Some(false) => 2,
    }
}

fn override_value(index: usize) -> Option<bool> {
    match index {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// A username as the service accepts it, or why it cannot be one.
pub(super) fn contact_name(input: &str) -> Result<String, &'static str> {
    let name = input.trim().trim_start_matches('@').to_lowercase();
    if name.is_empty() {
        return Err("Enter a username.");
    }
    if !(3..=24).contains(&name.len())
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err("Usernames are 3 to 24 lowercase letters, numbers or underscores.");
    }
    Ok(name)
}

impl AccountPanel {
    fn contact_action(
        &mut self,
        username: String,
        action: &'static str,
        can_send: Option<bool>,
        auto_download: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        if self.waiting("update contact") {
            return;
        }
        self.send(
            ClientCommand::ContactAction {
                username,
                action: action.into(),
                can_send,
                auto_download,
            },
            cx,
        );
    }

    pub(super) fn request_contact(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        match contact_name(&Self::text(&self.contact, cx)) {
            Ok(username) => {
                self.contact_feedback = Some(format!("Request sent to @{username}."));
                self.contact_action(username, "request", None, None, cx);
            }
            Err(problem) => self.say(Area::Contacts, Tone::Danger, problem, cx),
        }
    }

    fn confirm_contact(
        &mut self,
        username: String,
        action: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (title, body, confirm) = if action == "block" {
            (
                format!("Block @{username}?"),
                "They can no longer send to you or ask to be friends. You can unblock them later.",
                "Block",
            )
        } else {
            (
                format!("Remove @{username}?"),
                "You stop being friends, and your overrides for them are forgotten.",
                "Remove friend",
            )
        };
        let entity = cx.entity();
        dialog::prompt(title)
            .body(body)
            .confirm(confirm)
            .destructive()
            .open(window, cx, move |_, cx| {
                entity.update(cx, |this, cx| {
                    this.contact_action(username.clone(), action, None, None, cx);
                });
            });
    }

    pub(super) fn contacts(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.waiting("update contact");
        let add = surface::panel(p)
            .p(px(22.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(text::eyebrow("Add a friend").text_color(p.text_subtle))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div().flex_1().min_w_0().child(
                            field::input(p, &self.contact)
                                .id("contact-username")
                                .prefix(text::body("@").text_color(p.text_subtle)),
                        ),
                    )
                    .child(
                        button::primary("contacts-request")
                            .icon("plus")
                            .label("Send request")
                            .loading(busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.request_contact(window, cx)),
                            ),
                    ),
            )
            .child(field::hint(
                p,
                "Friends are people on this instance. They have to accept before either of you \
                 is listed.",
                Tone::Neutral,
            ));

        let Some(Contacts {
            contacts, blocked, ..
        }) = snapshot.contacts.clone()
        else {
            return div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(16.))
                .children(self.notice(Area::Contacts, p))
                .child(add)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(10.))
                        .py(px(28.))
                        .child(feedback::spinner(p.text_muted, 14.))
                        .child(text::small("Loading your contacts…").text_color(p.text_muted)),
                )
                .into_any_element();
        };
        let (requests, friends): (Vec<_>, Vec<_>) = contacts
            .into_iter()
            .partition(|contact| contact.status != "accepted");
        let nobody = requests.is_empty() && friends.is_empty() && blocked.is_empty();

        let section = |title: &'static str, count: usize| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(4.))
                .child(text::eyebrow(title).text_color(p.text_subtle))
                .child(text::caption(count.to_string()).text_color(p.text_subtle))
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .children(self.notice(Area::Contacts, p))
            .child(add)
            .when(!requests.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(section("Requests", requests.len()))
                        .child(
                            surface::panel(p).p(px(6.)).flex().flex_col().children(
                                requests
                                    .into_iter()
                                    .map(|contact| self.request_row(contact, p, cx)),
                            ),
                        ),
                )
            })
            .when(!friends.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(section("Friends", friends.len()))
                        .child(
                            surface::panel(p).p(px(6.)).flex().flex_col().children(
                                friends
                                    .into_iter()
                                    .map(|contact| self.friend_row(contact, p, cx)),
                            ),
                        ),
                )
            })
            .when(!blocked.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(section("Blocked", blocked.len()))
                        .child(surface::panel(p).p(px(6.)).flex().flex_col().children(
                            blocked.into_iter().map(|person| {
                                let username = person.username.clone();
                                self.person(p, &person.name, &person.username, None).child(
                                    button::secondary(SharedString::from(format!(
                                        "contact-unblock-{}",
                                        person.username
                                    )))
                                    .small()
                                    .label("Unblock")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.contact_action(
                                            username.clone(),
                                            "unblock",
                                            None,
                                            None,
                                            cx,
                                        )
                                    })),
                                )
                            }),
                        )),
                )
            })
            .when(nobody, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(6.))
                        .py(px(28.))
                        .child(icon("users").size(px(26.)).text_color(p.text_subtle))
                        .child(text::body_strong("No friends yet").text_color(p.text))
                        .child(
                            text::small(
                                "Add someone by username to choose what they may send you.",
                            )
                            .text_color(p.text_muted),
                        ),
                )
            })
            .into_any_element()
    }

    /// Avatar, name and handle: the leading part of every contact row.
    fn person(&self, p: Palette, name: &str, username: &str, caption: Option<String>) -> gpui::Div {
        let display = if name.trim().is_empty() {
            format!("@{username}")
        } else {
            name.to_owned()
        };
        let detail = match caption {
            Some(caption) => format!("@{username} · {caption}"),
            None => format!("@{username}"),
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(12.))
            .py(px(10.))
            .child(brand::avatar(p, &display, 36.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(text::body_strong(display).single_line().text_color(p.text))
                    .child(text::caption(detail).single_line().text_color(p.text_muted)),
            )
    }

    fn request_row(&self, contact: Contact, p: Palette, cx: &mut Context<Self>) -> gpui::Div {
        let incoming = contact.status == "incoming";
        let act = |action: &'static str| {
            let username = contact.username.clone();
            cx.listener(move |this, _, _, cx| {
                this.contact_action(username.clone(), action, None, None, cx)
            })
        };
        let id =
            |action: &str| SharedString::from(format!("contact-{action}-{}", contact.username));
        let row = self.person(
            p,
            &contact.name,
            &contact.username,
            Some(if incoming {
                "wants to be friends".into()
            } else {
                "waiting for them to accept".into()
            }),
        );
        if incoming {
            row.child(
                button::ghost(id("decline"))
                    .small()
                    .label("Decline")
                    .on_click(act("decline")),
            )
            .child(
                button::primary(id("accept"))
                    .small()
                    .label("Accept")
                    .on_click(act("accept")),
            )
        } else {
            row.child(
                button::secondary(id("cancel"))
                    .small()
                    .label("Cancel request")
                    .on_click(act("cancel")),
            )
        }
    }

    /// A friend, and when opened the two things that can be overridden for them.
    fn friend_row(&self, contact: Contact, p: Palette, cx: &mut Context<Self>) -> gpui::Div {
        let open = self.expanded.as_deref() == Some(contact.username.as_str());
        let toggle = contact.username.clone();
        let header = self
            .person(p, &contact.name, &contact.username, Some(summary(&contact)))
            .id(SharedString::from(format!("contact-{}", contact.username)))
            .rounded(px(radius::CONTROL))
            .cursor_pointer()
            .hover(move |style| style.bg(p.hover))
            .child(
                icon(if open { "chevron-up" } else { "chevron-down" })
                    .size(px(16.))
                    .flex_none()
                    .text_color(p.text_subtle),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.expanded =
                    (this.expanded.as_deref() != Some(toggle.as_str())).then(|| toggle.clone());
                cx.notify();
            }));

        let choice = |id: &'static str,
                      title: &'static str,
                      labels: [&'static str; 3],
                      selected: Option<bool>,
                      apply: Apply| {
            let panel = cx.entity().downgrade();
            let subject = contact.clone();
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(text::small(title).text_color(p.text_muted))
                .child(
                    rail(
                        SharedString::from(format!("{id}-{}", contact.username)),
                        title,
                        override_index(selected),
                    )
                    .segment_width(86.)
                    .segments(labels.into_iter().map(Segment::new))
                    .on_select(move |index, _, cx| {
                        let (can_send, auto_download) = apply(override_value(index), &subject);
                        let username = subject.username.clone();
                        let _ = panel.update(cx, |this, cx| {
                            this.contact_action(
                                username,
                                "preferences",
                                can_send,
                                auto_download,
                                cx,
                            )
                        });
                    }),
                )
        };
        let (remove, block) = (contact.username.clone(), contact.username.clone());
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .when(open, |this| {
                this.child(
                    div()
                        .mx(px(12.))
                        .mb(px(10.))
                        .p(px(14.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .rounded(px(radius::CONTROL))
                        .bg(p.sunken)
                        .border_1()
                        .border_color(p.border)
                        .child(choice(
                            "contact-can-send",
                            "Can send to me",
                            ["Default", "Allow", "Deny"],
                            contact.can_send,
                            |value, contact| (value, contact.auto_download),
                        ))
                        .child(choice(
                            "contact-auto",
                            "Fetch automatically",
                            ["Default", "On", "Off"],
                            contact.auto_download,
                            |value, contact| (contact.can_send, value),
                        ))
                        .child(surface::hairline(p))
                        .child(
                            div()
                                .flex()
                                .gap(px(4.))
                                .child(
                                    button::danger(SharedString::from(format!(
                                        "contact-remove-{}",
                                        contact.username
                                    )))
                                    .small()
                                    .label("Remove friend")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.confirm_contact(remove.clone(), "remove", window, cx)
                                    })),
                                )
                                .child(
                                    button::danger(SharedString::from(format!(
                                        "contact-block-{}",
                                        contact.username
                                    )))
                                    .small()
                                    .icon("slash")
                                    .label("Block")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.confirm_contact(block.clone(), "block", window, cx)
                                    })),
                                ),
                        ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use filebeam_client_core::services::contacts::EffectiveReceiving;

    use super::*;

    #[test]
    fn overrides_round_trip_and_summaries_follow_the_effective_result() {
        for value in [None, Some(true), Some(false)] {
            assert_eq!(override_value(override_index(value)), value);
        }
        let mut contact = Contact {
            id: 1,
            username: "ada".into(),
            name: "Ada".into(),
            status: "accepted".into(),
            can_send: None,
            auto_download: None,
            effective: EffectiveReceiving {
                can_send: true,
                auto_download: true,
            },
        };
        assert_eq!(summary(&contact), "Can send to you · fetched automatically");
        contact.effective.auto_download = false;
        assert_eq!(summary(&contact), "Can send to you");
        contact.effective.can_send = false;
        assert_eq!(summary(&contact), "Cannot send to you");
    }

    #[test]
    fn usernames_are_normalised_or_explained() {
        assert_eq!(contact_name(" @Ada_1 ").as_deref(), Ok("ada_1"));
        assert_eq!(contact_name(""), Err("Enter a username."));
        assert!(contact_name("al").is_err());
        assert!(contact_name("ada@remote").is_err());
        assert!(contact_name("../ada").is_err());
    }
}
