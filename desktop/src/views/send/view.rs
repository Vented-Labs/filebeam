//! How the composer is drawn. Every value comes from retained state; nothing here
//! performs I/O or dispatches work outside an event handler.

use gpui::{
    AnyElement, Context, ExternalPaths, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{
    Sizable, Size,
    input::{Editor, Input},
    scroll::ScrollableElement,
    select::Select,
};

use super::{
    ATTACHED_NOTE_BYTES, ComposerMode, Copied, SendPanel, Share, Transport,
    compose::{driver_policy_label, retention_label, unavailable_lifetime_error},
};
use crate::{
    assets::icon,
    model::{
        DesktopSnapshot, DeviceTrust, JobSnapshot, PolicyAvailability, TransferDirection,
        TransferState,
    },
    theme::{
        self, Palette,
        tokens::{geometry, radius},
    },
    ui::{
        Hugged, MotionExt, TextExt, Tone,
        brand::emblem,
        button,
        feedback::{self, spinner},
        field::{self, copy_field},
        format, motion, page,
        progress::{percent_label, progress},
        rail::{Segment, rail},
        row::{self, switch_row},
        surface, text,
    },
};

/// Below this content width the options move under the composer.
const STACK_BELOW: f32 = 940.;
/// Between the note editor's outer edge and the language select in its header.
const NOTE_HEADER_GAP: f32 = 10.;
const NOTE_SELECT_HEIGHT: f32 = 30.;

impl Render for SendPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let snapshot = self.live.read(cx).snapshot();
        let stacked = page::content_width(window) < STACK_BELOW;
        let header = page::header(
            p,
            "Send",
            "Files and notes, encrypted on this device before they leave it.",
            window,
            cx,
        );
        let composer = self.composer(&snapshot, p, window, cx);
        let options = self.options(&snapshot, p, window, cx);
        page::scroller(
            "send-page-scroll",
            &self.page_scroll,
            geometry::CONTENT_MAX,
            false,
            window,
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(18.))
                .child(header)
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_start()
                        .gap(px(20.))
                        .when(stacked, |this| this.flex_col())
                        .child(div().flex_1().min_w_0().w_full().child(composer))
                        .child(
                            div()
                                .flex_none()
                                .when(stacked, |this| this.w_full())
                                .when(!stacked, |this| this.w(px(geometry::INSPECTOR)))
                                .child(options),
                        ),
                ),
        )
    }
}

impl SendPanel {
    fn composer(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let entrance = motion::entrance("send-composer", 1, window, cx);
        let panel = cx.entity().downgrade();
        let modes = rail(
            "send-mode-tabs",
            "What to send",
            usize::from(self.mode == ComposerMode::Notes),
        )
        .segment_width(112.)
        .segment(Segment::new("Files").icon("folder"))
        .segment(Segment::new("Notes").icon("note"))
        .on_select(move |index, _, cx| {
            let _ = panel.update(cx, |this, cx| {
                // Composing something else leaves the finished share to Transfers.
                this.share = None;
                this.set_mode(
                    if index == 0 {
                        ComposerMode::Files
                    } else {
                        ComposerMode::Notes
                    },
                    cx,
                );
            });
        });
        let body = if self.is_sharing() {
            self.share_view(snapshot, p, window, cx)
        } else {
            self.draft_view(snapshot, p, window, cx)
        };
        surface::panel(p)
            .w_full()
            .flex()
            .flex_col()
            .entering(entrance)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .px(px(20.))
                    .py(px(14.))
                    .border_b_1()
                    .border_color(p.line_soft)
                    .child(modes)
                    .child(feedback::pill_with_icon(
                        p,
                        "lock",
                        "End-to-end encrypted",
                        Tone::Neutral,
                    )),
            )
            .child(body)
    }

    fn draft_view(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let policy = &snapshot.policy;
        let transport = self.active_preferences().transport;
        let panel = cx.entity().downgrade();
        let transports = rail(
            "send-transport",
            "Transfer method",
            usize::from(transport == Transport::Live),
        )
        .fill()
        .segment(
            Segment::new("Server")
                .icon("http-server")
                .caption("Send via Filebeam server.")
                .enabled(policy.http.enabled),
        )
        .segment(
            Segment::new("P2P")
                .icon("webrtc-p2p")
                .caption("Send via WebRTC.")
                .enabled(policy.webrtc.enabled),
        )
        .on_select(move |index, _, cx| {
            let _ = panel.update(cx, |this, cx| {
                this.set_transport(
                    if index == 0 {
                        Transport::Http
                    } else {
                        Transport::Live
                    },
                    cx,
                );
            });
        });
        let driver = if transport == Transport::Http {
            &policy.http
        } else {
            &policy.webrtc
        };
        let unavailable = policy.availability == PolicyAvailability::Unavailable;
        let limits = div()
            .flex()
            .items_center()
            .gap(px(7.))
            .child(icon("storage").size(px(14.)).text_color(p.text_subtle))
            .child(
                text::caption(if unavailable {
                    "Limits appear once the instance responds.".to_owned()
                } else {
                    driver_policy_label(driver)
                })
                .text_color(p.text_muted),
            );

        let content = if self.mode == ComposerMode::Files {
            self.files_view(snapshot, p, window, cx)
        } else {
            self.note_view(p, cx)
        };

        let status = match (&self.status, &policy.reason) {
            (Some(status), _) => div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    icon("alert")
                        .size(px(15.))
                        .flex_none()
                        .text_color(p.danger.ink),
                )
                .child(
                    text::small(status.clone())
                        .flex_1()
                        .min_w_0()
                        .text_color(p.danger.ink),
                ),
            (None, Some(reason)) => div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    icon("info")
                        .size(px(15.))
                        .flex_none()
                        .text_color(p.warning.ink),
                )
                .child(
                    text::small(format::readable_error(reason))
                        .flex_1()
                        .min_w_0()
                        .text_color(p.warning.ink),
                ),
            (None, None) => div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    icon("shield")
                        .size(px(15.))
                        .flex_none()
                        .text_color(p.text_subtle),
                )
                .child(
                    text::small("Encrypted on this device before it is sent.")
                        .text_color(p.text_muted),
                ),
        };

        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .px(px(20.))
                    .pt(px(16.))
                    .pb(px(16.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(transports)
                            .child(limits),
                    )
                    .child(content),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.))
                    .px(px(20.))
                    .py(px(12.))
                    .border_t_1()
                    .border_color(p.line_soft)
                    .child(status.flex_1().min_w_0())
                    .child(
                        button::primary("encrypt-share")
                            .large()
                            .icon("lock")
                            .label("Encrypt and share")
                            .disabled(!self.can_submit(cx) || self.preparing_paths || unavailable)
                            .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    ),
            )
            .into_any_element()
    }

    fn files_view(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_files = !self.paths.is_empty();
        let engaged = self.dropzone_hovered || cx.has_active_drag();
        let devices: Vec<_> = snapshot
            .device_directory
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.trust,
                    DeviceTrust::Registered | DeviceTrust::Verified
                )
            })
            .collect();

        let zone = div()
            .id("send-dropzone")
            .w_full()
            .rounded(px(radius::CARD))
            .border_1()
            .drag_over::<ExternalPaths>(move |style, _, _, _| {
                style.bg(p.accent_surface).border_color(p.choice_border)
            })
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.receive_drop(paths, cx)))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if this.dropzone_hovered != *hovered {
                    this.dropzone_hovered = *hovered;
                    cx.notify();
                }
            }));

        let zone = if has_files {
            let count = format::plural(self.paths.len() as u64, "item");
            let folders = self
                .details
                .values()
                .filter(|detail| detail.directory)
                .count();
            let known: u64 = self.details.values().filter_map(|detail| detail.size).sum();
            let summary = match (self.total_size(), folders) {
                (Some(total), _) => format!("{count} ready to protect · {}", format::bytes(total)),
                (None, folders) if folders > 0 && known > 0 => format!(
                    "{count} ready to protect · {} plus {}",
                    format::bytes(known),
                    format::plural(folders as u64, "folder")
                ),
                (None, _) => format!("{count} ready to protect"),
            };
            let rows = self.paths.iter().enumerate().map(|(index, path)| {
                let name = format::file_name(path);
                let detail = self.details.get(path).copied();
                let directory = detail.is_some_and(|detail| detail.directory);
                let caption = match detail {
                    Some(detail) if detail.directory => "Folder".to_owned(),
                    Some(detail) => detail
                        .size
                        .map_or_else(|| "Size unavailable".to_owned(), format::bytes),
                    None => "Reading…".to_owned(),
                };
                row::slot().child(
                    row::hugging_frame(row::Fit::CARD)
                        .hover(move |style| style.bg(p.hover))
                        .child(row::lead_tile(
                            p,
                            row::Fit::CARD,
                            if directory {
                                "folder"
                            } else {
                                format::file_icon(&name)
                            },
                            true,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(1.))
                                .child(
                                    text::body_strong(name.clone())
                                        .single_line()
                                        .text_color(p.text),
                                )
                                .child(text::caption(caption).text_color(p.text_muted)),
                        )
                        .child(
                            button::icon_button(("remove-path", index), "x", "Remove")
                                .small()
                                .accessibility_label(format!("Remove {name}"))
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.remove_path(index, cx)),
                                ),
                        ),
                )
            });
            zone.bg(p.sunken)
                .border_color(p.border)
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .px(px(16.))
                        .pt(px(14.))
                        .pb(px(10.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(1.))
                                .child(text::subtitle("Your files").text_color(p.text))
                                .child(text::caption(summary).text_color(p.text_muted)),
                        )
                        .child(
                            button::secondary("choose-files")
                                .small()
                                .icon("plus")
                                .label("Add files")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.choose_paths(false, window, cx)
                                })),
                        )
                        .child(
                            button::secondary("choose-folder")
                                .small()
                                .icon("folder")
                                .label("Add folder")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.choose_paths(true, window, cx)
                                })),
                        )
                        .child(
                            button::ghost("remove-all-paths")
                                .small()
                                .label("Clear")
                                .on_click(cx.listener(|this, _, _, cx| this.clear_paths(cx))),
                        ),
                )
                .child(
                    div()
                        .id("send-file-list")
                        .max_h(px(248.))
                        .overflow_y_scrollbar()
                        .hugged()
                        .flex()
                        .flex_col()
                        .children(rows),
                )
        } else {
            zone.border_dashed()
                .border_color(p.control_border.opacity(0.55))
                .min_h(px(268.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .px(px(24.))
                .py(px(20.))
                .child(emblem("send-emblem").engaged(engaged).size(100.))
                .child(
                    div()
                        .mt(px(10.))
                        .flex()
                        .flex_wrap()
                        .justify_center()
                        .gap_x(px(9.))
                        .child(text::display("Drop your files").text_color(p.text))
                        .child(text::display("here").text_color(p.accent_text)),
                )
                .child(
                    text::body(if self.preparing_paths {
                        "Adding your selection…"
                    } else {
                        "They are encrypted on this device before they leave it."
                    })
                    .mt(px(6.))
                    .text_center()
                    .text_color(p.text_muted),
                )
                .child(
                    div()
                        .mt(px(16.))
                        .flex()
                        .flex_wrap()
                        .justify_center()
                        .gap(px(10.))
                        .child(
                            button::primary("choose-files")
                                .large()
                                .icon("folder")
                                .label("Choose files")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.choose_paths(false, window, cx)
                                })),
                        )
                        .child(
                            button::secondary("choose-folder")
                                .large()
                                .label("Choose a folder")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.choose_paths(true, window, cx)
                                })),
                        ),
                )
                .child(
                    text::caption("or drag and drop anywhere in this panel")
                        .mt(px(12.))
                        .text_color(p.text_subtle),
                )
        };

        let targets = (!devices.is_empty()).then(|| {
            let link_selected = self.selected_device.is_none();
            let chip = |id: SharedString, name: &'static str, label: String, selected: bool| {
                if selected {
                    button::tonal(id)
                } else {
                    button::secondary(id)
                }
                .small()
                .icon(name)
                .label(label)
            };
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(field::label(p, "Send to", false))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(8.))
                        .child(
                            chip(
                                "device-target-link".into(),
                                "link",
                                "Anyone with the link".into(),
                                link_selected,
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.select_device(None, cx))),
                        )
                        .children(devices.iter().map(|device| {
                            let id = device.id.clone();
                            let selected =
                                self.selected_device.as_deref() == Some(device.id.as_str());
                            chip(
                                SharedString::from(format!("device-target-{}", device.id)),
                                if device.platform.eq_ignore_ascii_case("android")
                                    || device.platform.eq_ignore_ascii_case("ios")
                                {
                                    "mobile"
                                } else {
                                    "laptop"
                                },
                                device.name.clone(),
                                selected,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.select_device(Some(id.clone()), cx),
                            ))
                        })),
                )
                .when(!link_selected, |this| {
                    this.child(field::hint(
                        p,
                        "Your local device service verifies the device before it connects.",
                        Tone::Neutral,
                    ))
                })
        });

        let note_bytes = self.attached_editor.read(cx).value().len();
        let attach = surface::card(p)
            .px(px(16.))
            .py(px(4.))
            .flex()
            .flex_col()
            .child(
                switch_row("attach-note", "Attach a note", self.attach_note)
                    .icon("note")
                    .description("Send context alongside your files.")
                    .on_toggle(cx.listener(|this, checked: &bool, _, cx| {
                        this.attach_note = *checked;
                        this.status = None;
                        cx.notify();
                    })),
            )
            .when(self.attach_note, |this| {
                let entrance = motion::entrance_with(
                    "attached-note",
                    0,
                    8.,
                    motion::enter_quick(),
                    window,
                    cx,
                );
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .pb(px(14.))
                        .entering(entrance)
                        .child(
                            div()
                                .flex()
                                .gap(px(10.))
                                .child(div().flex_1().min_w_0().child(
                                    field::input(p, &self.attached_title).id("attached-note-title"),
                                ))
                                .child(
                                    div().w(px(150.)).flex_none().child(
                                        field::select(p, &self.attached_language)
                                            .id("attached-note-language"),
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .h(px(180.))
                                .rounded(px(radius::CONTROL))
                                .border_1()
                                .border_color(p.border)
                                .overflow_hidden()
                                .child(
                                    Editor::new(&self.attached_editor)
                                        .aria_label("Attached note contents")
                                        .bordered(false)
                                        .h_full(),
                                ),
                        )
                        .child(field::hint(
                            p,
                            format!(
                                "{} of {} · uses this transfer's encryption and expiry",
                                format::bytes(note_bytes as u64),
                                format::bytes(ATTACHED_NOTE_BYTES as u64)
                            ),
                            if note_bytes > ATTACHED_NOTE_BYTES {
                                Tone::Danger
                            } else {
                                Tone::Neutral
                            },
                        )),
                )
            });

        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(zone)
            .children(targets)
            .child(attach)
            .into_any_element()
    }

    fn note_view(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let characters = self.note_editor.read(cx).value().chars().count();
        // The language select sits `NOTE_HEADER_GAP` inside the editor's top corner.
        surface::well(p)
            .rounded(px(radius::CARD))
            .h(px(388.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .h(px(NOTE_SELECT_HEIGHT + 2. * (NOTE_HEADER_GAP - 1.)))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(14.))
                    .pr(px(NOTE_HEADER_GAP - 1.))
                    .bg(p.surface)
                    .border_b_1()
                    .border_color(p.line_soft)
                    .child(
                        icon("note")
                            .size(px(16.))
                            .flex_none()
                            .text_color(p.text_muted),
                    )
                    .child(
                        div().flex_1().min_w_0().h(px(34.)).child(
                            Input::new(&self.note_title)
                                .id("note-title")
                                .appearance(false)
                                .bordered(false)
                                .h(px(34.)),
                        ),
                    )
                    .child(
                        div()
                            .w(px(140.))
                            .h(px(NOTE_SELECT_HEIGHT))
                            .flex_none()
                            .child(
                                Select::new(&self.language)
                                    .id("note-language")
                                    .with_size(Size::Medium)
                                    .h(px(NOTE_SELECT_HEIGHT))
                                    .rounded(px(radius::nested(radius::CARD, NOTE_HEADER_GAP)))
                                    .bg(p.sunken),
                            ),
                    ),
            )
            .child(
                div().flex_1().min_h_0().child(
                    Editor::new(&self.note_editor)
                        .aria_label("Note contents")
                        .bordered(false)
                        .h_full(),
                ),
            )
            .child(
                div()
                    .h(px(34.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .px(px(14.))
                    .border_t_1()
                    .border_color(p.line_soft)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(icon("lock").size(px(13.)).text_color(p.text_subtle))
                            .child(
                                text::caption("Encrypted on this device before sharing.")
                                    .text_color(p.text_muted),
                            ),
                    )
                    .child(
                        text::caption(format::plural(characters as u64, "character"))
                            .text_color(p.text_muted),
                    ),
            )
            .into_any_element()
    }

    fn options(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let entrance = motion::entrance("send-options", 2, window, cx);
        let files = self.mode == ComposerMode::Files;
        let preferences = self.active_preferences();
        let selected_lifetime = preferences.lifetime.read(cx).selected_value().copied();
        let lifetime_error = unavailable_lifetime_error(selected_lifetime, &self.lifetime_hours);
        let contacts: Vec<_> = if files {
            snapshot
                .contacts
                .as_ref()
                .map(|data| {
                    data.contacts
                        .iter()
                        .filter(|contact| contact.status == "accepted")
                        .map(|contact| (contact.username.clone(), contact.id))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let recipient = preferences.recipient.clone();

        surface::panel(p)
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .p(px(20.))
            .entering(entrance)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(text::eyebrow("For this transfer").text_color(p.text_subtle))
                    .child(text::subtitle("Sharing options").text_color(p.text)),
            )
            .child(
                field::optional_field(
                    p,
                    "Password",
                    field::secret(p, &preferences.password).id("transfer-password"),
                )
                .child(field::hint(
                    p,
                    "Share the password separately from the link.",
                    Tone::Neutral,
                )),
            )
            .child(
                field::field(
                    p,
                    "Kept for",
                    field::select(p, &preferences.lifetime).id("transfer-lifetime"),
                )
                .child(match lifetime_error {
                    Some(error) => field::hint(p, error, Tone::Danger),
                    None => field::hint(p, retention_label(&snapshot.policy), Tone::Neutral),
                }),
            )
            .child(surface::hairline(p))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        switch_row(
                            "include-key",
                            "Include key in link",
                            preferences.include_key,
                        )
                        .icon("key")
                        .description("Off sends the key separately.")
                        .on_toggle(cx.listener(
                            |this, checked: &bool, _, cx| {
                                this.active_preferences_mut().include_key = *checked;
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        switch_row("turbo", "Turbo Transfer", preferences.turbo && files)
                            .icon("bolt")
                            .description("Share while it uploads.")
                            .disabled(!files)
                            .on_toggle(cx.listener(|this, checked: &bool, _, cx| {
                                this.active_preferences_mut().turbo = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        if files {
                            switch_row(
                                "zip-or-burn",
                                "Combine into one ZIP",
                                preferences.zip_directories,
                            )
                            .icon("archive")
                            .description("One archive to download.")
                        } else {
                            switch_row(
                                "zip-or-burn",
                                "Burn after reading",
                                preferences.burn_on_read,
                            )
                            .icon("eye-off")
                            .description("Removed once it is opened.")
                        }
                        .on_toggle(cx.listener(
                            |this, checked: &bool, _, cx| {
                                if this.mode == ComposerMode::Files {
                                    this.files.zip_directories = *checked;
                                } else {
                                    this.notes.burn_on_read = *checked;
                                }
                                cx.notify();
                            },
                        )),
                    ),
            )
            .when(files, |this| {
                this.child(surface::hairline(p)).child(
                    field::optional_field(
                        p,
                        "Deliver to a username",
                        field::input(p, &recipient)
                            .id("transfer-recipient")
                            .prefix(div().child(text::body("@").text_color(p.text_subtle))),
                    )
                    .when(!contacts.is_empty(), |this| {
                        this.child(
                            div().flex().flex_wrap().gap(px(6.)).children(
                                contacts
                                    .into_iter()
                                    .enumerate()
                                    .map(|(index, (username, id))| {
                                        let input = recipient.clone();
                                        button::secondary(("friend-recipient", index))
                                            .small()
                                            .label(format!("@{username}"))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                input.update(cx, |input, cx| {
                                                    input.set_value(username.clone(), window, cx)
                                                });
                                                this.files.chosen_contact =
                                                    Some((username.clone(), id));
                                                cx.notify();
                                            }))
                                    }),
                            ),
                        )
                    })
                    .child(field::hint(
                        p,
                        "Goes to their inbox, protected by their account key.",
                        Tone::Neutral,
                    )),
                )
            })
    }

    fn share_view(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entrance = motion::entrance_with("share-view", 0, 12., motion::enter(), window, cx);
        let column = div()
            .w_full()
            .max_w(px(560.))
            .mx_auto()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(12.))
            .px(px(24.))
            .py(px(36.))
            .entering(entrance);
        let back = || {
            button::quiet("share-back")
                .label("Back to draft")
                .on_click(cx.listener(|this, _, _, cx| this.back_to_draft(cx)))
        };
        let again = || {
            button::tonal("share-new")
                .icon("plus")
                .label("New transfer")
                .on_click(cx.listener(|this, _, window, cx| this.new_transfer(window, cx)))
        };
        let heading = |title: &'static str, detail: String| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .child(text::title(title).text_center().text_color(p.text))
                .child(
                    text::body(detail)
                        .max_w(px(440.))
                        .text_center()
                        .text_color(p.text_muted),
                )
        };

        let job = match &self.share {
            Some(Share::Job(id)) => snapshot.jobs.iter().find(|job| &job.id == id).cloned(),
            _ => None,
        };
        match (&self.share, job) {
            (Some(Share::Rejected(reason)), _) => column
                .child(row::icon_tile(p, "alert", 56., false))
                .child(heading(
                    "That could not be shared",
                    "Nothing left this device. Your draft is unchanged.".into(),
                ))
                .child(
                    div()
                        .w_full()
                        .child(feedback::notice(p, reason.clone(), Tone::Danger)),
                )
                .child(div().mt(px(6.)).child(back()))
                .into_any_element(),
            (Some(Share::Device(name)), _) => column
                .child(row::icon_tile(p, "laptop", 56., true))
                .child(heading(
                    "Queued for your device",
                    format!(
                        "The local device service delivers it once {name} is reachable. \
                         Follow it in Transfers."
                    ),
                ))
                .child(
                    div()
                        .mt(px(6.))
                        .flex()
                        .gap(px(8.))
                        .child(again())
                        .child(back()),
                )
                .into_any_element(),
            (Some(Share::Job(_)), Some(job)) => self.job_view(&job, column, p, cx),
            _ => column
                .child(spinner(p.accent_text, 28.))
                .child(heading(
                    "Encrypting…",
                    "Preparing your transfer on this device.".into(),
                ))
                .into_any_element(),
        }
    }

    fn job_view(
        &mut self,
        job: &JobSnapshot,
        column: gpui::Div,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fraction = job
            .progress
            .total_bytes
            .filter(|total| *total > 0)
            .map(|total| (job.progress.completed_bytes as f32 / total as f32).clamp(0., 1.));
        let transferred = match job.progress.total_bytes {
            Some(total) => format!(
                "{} of {}",
                format::bytes(job.progress.completed_bytes),
                format::bytes(total)
            ),
            None => format::bytes(job.progress.completed_bytes),
        };
        let note = job.direction == TransferDirection::Note;
        let back = button::quiet("share-back")
            .label("Back to draft")
            .on_click(cx.listener(|this, _, _, cx| this.back_to_draft(cx)));
        let again = button::tonal("share-new")
            .icon("plus")
            .label("New transfer")
            .on_click(cx.listener(|this, _, window, cx| this.new_transfer(window, cx)));
        let heading = |title: &'static str, detail: &'static str| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .child(text::title(title).text_center().text_color(p.text))
                .child(
                    text::body(detail)
                        .max_w(px(440.))
                        .text_center()
                        .text_color(p.text_muted),
                )
        };

        if job.state == TransferState::Failed {
            let reason = job.error.as_ref().map_or_else(
                || "The transfer stopped before it finished.".to_owned(),
                |error| format::readable_error(&error.detail),
            );
            return column
                .child(row::icon_tile(p, "alert", 56., false))
                .child(heading(
                    "This share did not finish",
                    "Your draft is unchanged, so you can try again.",
                ))
                .child(
                    div()
                        .w_full()
                        .child(feedback::notice(p, reason, Tone::Danger)),
                )
                .child(div().mt(px(6.)).child(back))
                .into_any_element();
        }

        let Some(link) = job.share_url.clone() else {
            if job.state == TransferState::Complete {
                return column
                    .child(emblem("share-emblem").sealed(true).size(96.))
                    .child(heading(
                        "Delivered",
                        "It is waiting in the recipient's inbox, protected by their account key.",
                    ))
                    .child(div().mt(px(6.)).flex().gap(px(8.)).child(again).child(back))
                    .into_any_element();
            }
            let phase = if job.progress.phase.is_empty() {
                "Working".to_owned()
            } else {
                job.progress.phase.clone()
            };
            return column
                .child(emblem("share-emblem").engaged(true).size(96.))
                .child(heading(
                    if note {
                        "Encrypting your note"
                    } else {
                        "Encrypting and uploading"
                    },
                    "Keep Filebeam open. The link appears here when it is ready.",
                ))
                .child(
                    div()
                        .w_full()
                        .mt(px(6.))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(progress("share-progress", fraction).height(8.))
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .gap(px(12.))
                                .child(text::caption(phase).text_color(p.text_muted))
                                .child(
                                    text::caption(match fraction {
                                        Some(fraction) => {
                                            format!("{transferred} · {}", percent_label(fraction))
                                        }
                                        None => transferred.clone(),
                                    })
                                    .text_color(p.text_muted),
                                ),
                        ),
                )
                .into_any_element();
        };

        let split = job.separate_key.is_some();
        let live = job.capabilities.can_end_live;
        let uploading = job.state != TransferState::Complete && !live && fraction != Some(1.);
        let copy = |value: String, kind: Copied| {
            cx.listener(move |this, _, _, cx| this.copy(value.clone(), kind, cx))
        };
        column
            .child(emblem("share-emblem").sealed(true).size(96.))
            .child(heading(
                if note {
                    "Your encrypted note is ready"
                } else {
                    "Your encrypted link is ready"
                },
                if split {
                    "Send the link and the key through different channels. Both are needed."
                } else {
                    "The key stays in the link fragment and is never sent to the server."
                },
            ))
            .child(
                div()
                    .w_full()
                    .mt(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        copy_field("copy-share-link", "link", link.clone())
                            .action("Copy link")
                            .primary()
                            .copied(self.copied == Some(Copied::Link))
                            .on_copy(copy(link, Copied::Link)),
                    )
                    .when_some(job.separate_key.clone(), |this, key| {
                        this.child(
                            // The key is never drawn; only the clipboard receives it.
                            copy_field("copy-share-key", "key", "Decryption key · hidden")
                                .action("Copy key")
                                .copied(self.copied == Some(Copied::Key))
                                .on_copy(copy(key, Copied::Key)),
                        )
                    })
                    .when_some(job.cli_command.clone(), |this, command| {
                        this.child(
                            copy_field("copy-share-cli", "terminal", command.clone())
                                .action("Copy")
                                .copied(self.copied == Some(Copied::Command))
                                .on_copy(copy(command, Copied::Command)),
                        )
                    }),
            )
            .when(live && job.state != TransferState::Complete, |this| {
                this.child(div().w_full().child(feedback::notice(
                    p,
                    "Keep Filebeam open and online until the recipient has finished downloading.",
                    Tone::Info,
                )))
            })
            .when(uploading, |this| {
                this.child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(progress("share-progress", fraction))
                        .child(
                            text::caption(format!("Still uploading · {transferred}"))
                                .text_color(p.text_muted),
                        ),
                )
            })
            .child(div().mt(px(8.)).flex().gap(px(8.)).child(again).child(back))
            .into_any_element()
    }
}
