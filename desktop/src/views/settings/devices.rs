//! Devices registered to the account, and receiving from them on this desktop.
//! A device is only trusted once its fingerprint has been compared in person.

use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, PathPromptOptions,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use super::{Area, Copied, SettingsPanel};
use crate::{
    assets::icon,
    model::{
        ClientCommand, DesktopSnapshot, DeviceDirectoryAvailability, DeviceSnapshot, DeviceTrust,
    },
    shell::OpenAccount,
    theme::{Palette, tokens::radius},
    ui::{
        Hugged, TextExt, Tone, button, dialog, feedback,
        field::{self, copy_field},
        row, surface, text,
    },
};

fn trust(trust: DeviceTrust) -> (&'static str, Tone) {
    match trust {
        DeviceTrust::Verified => ("Verified", Tone::Success),
        DeviceTrust::Registered => ("Not verified", Tone::Warning),
        DeviceTrust::Pending => ("Waiting for approval", Tone::Accent),
        DeviceTrust::Revoked => ("Revoked", Tone::Neutral),
    }
}

fn platform_icon(platform: &str) -> &'static str {
    match platform.to_ascii_lowercase().as_str() {
        "linux" => "os-linux",
        "macos" | "darwin" => "os-macos",
        "windows" => "os-windows",
        "android" | "ios" => "mobile",
        _ => "laptop",
    }
}

fn platform_name(platform: &str) -> String {
    match platform.to_ascii_lowercase().as_str() {
        "linux" => "Linux".into(),
        "macos" | "darwin" => "macOS".into(),
        "windows" => "Windows".into(),
        "android" => "Android".into(),
        "ios" => "iOS".into(),
        _ => platform.to_owned(),
    }
}

fn is_local(device: &DeviceSnapshot) -> bool {
    device.detail.as_deref() == Some("This device")
}

impl SettingsPanel {
    fn register_device(&mut self, cx: &mut Context<Self>) {
        if self.waiting("enroll device") {
            return;
        }
        let display_name = Self::text(&self.device_name, cx);
        if display_name.is_empty() {
            self.say(Area::Devices, Tone::Danger, "Give this desktop a name.", cx);
            return;
        }
        self.send(ClientCommand::EnrollDevice { display_name }, cx);
    }

    fn approve_device(&mut self, cx: &mut Context<Self>) {
        if self.waiting("approve device enrollment") {
            return;
        }
        let approval_payload = Self::text(&self.approval_payload, cx);
        let verified_fingerprint = Self::text(&self.approval_fingerprint, cx);
        if approval_payload.is_empty() || verified_fingerprint.is_empty() {
            self.say(
                Area::Devices,
                Tone::Danger,
                "Paste the request and enter the fingerprint the new device shows.",
                cx,
            );
            return;
        }
        self.send(
            ClientCommand::ApproveDeviceEnrollment {
                approval_payload,
                verified_fingerprint,
            },
            cx,
        );
    }

    fn rename_device(&mut self, id: String, cx: &mut Context<Self>) {
        let display_name = Self::text(&self.rename_name, cx);
        if display_name.is_empty() {
            self.say(Area::Devices, Tone::Danger, "Enter a new name.", cx);
            return;
        }
        if !self.waiting("rename device") {
            self.send(ClientCommand::RenameDevice { id, display_name }, cx);
        }
    }

    fn verify_device(&mut self, id: String, cx: &mut Context<Self>) {
        let verified_fingerprint = Self::text(&self.verify_fingerprint, cx);
        if verified_fingerprint.is_empty() {
            self.say(
                Area::Devices,
                Tone::Danger,
                "Enter the fingerprint that device shows.",
                cx,
            );
            return;
        }
        if !self.waiting("confirm device verification") {
            self.send(
                ClientCommand::ConfirmDevicePeer {
                    id,
                    verified_fingerprint,
                },
                cx,
            );
        }
    }

    /// Revoking cannot be undone, so it asks for the password in a prompt.
    fn confirm_revoke(
        &mut self,
        device: DeviceSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_notice(Area::Revoke, cx);
        let password = self.revoke_password.clone();
        let forgotten = password.clone();
        let (shown, submitted) = (cx.entity(), cx.entity());
        dialog::prompt(format!("Revoke {}?", device.name))
            .body(
                "It can no longer send to or receive from your devices. To use it again it has \
                 to be registered and approved from the start.",
            )
            .confirm("Revoke device")
            .destructive()
            .open_with(
                window,
                cx,
                move |p, _, cx| {
                    let problem = shown.read(cx).notice(Area::Revoke, p);
                    Some(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .child(field::field(
                                p,
                                "Current password",
                                field::secret(p, &password).id("revoke-password"),
                            ))
                            .children(problem)
                            .into_any_element(),
                    )
                },
                move |window, cx| {
                    let id = device.id.clone();
                    let sent = submitted.update(cx, |this, cx| {
                        let current_password = this.revoke_password.read(cx).value().to_string();
                        if current_password.is_empty() {
                            this.say(
                                Area::Revoke,
                                Tone::Danger,
                                "Enter your current password.",
                                cx,
                            );
                            return false;
                        }
                        this.clear_notice(Area::Revoke, cx);
                        this.send(
                            ClientCommand::RevokeDevice {
                                id,
                                current_password,
                            },
                            cx,
                        );
                        true
                    });
                    window.refresh();
                    sent
                },
                move |window, cx| {
                    forgotten.update(cx, |input, cx| input.set_value("", window, cx));
                },
            );
    }

    fn choose_dropzone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let alias = Self::text(&self.dropzone_alias, cx);
        if alias.is_empty() {
            self.say(Area::Devices, Tone::Danger, "Name the folder first.", cx);
            return;
        }
        let auto_accept = self.dropzone_auto;
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Receive here".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.send(
                    ClientCommand::AddDeviceDropzone {
                        alias,
                        path,
                        auto_accept,
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    pub(super) fn devices(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !snapshot.account.authenticated {
            return surface::panel(p)
                .p(px(28.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.))
                .child(icon("laptop").size(px(28.)).text_color(p.text_subtle))
                .child(text::subtitle("Sign in to use your devices").text_color(p.text))
                .child(
                    text::small(
                        "Registered devices can send straight to each other, without a link.",
                    )
                    .text_center()
                    .text_color(p.text_muted),
                )
                .child(
                    div().mt(px(6.)).child(
                        button::primary("devices-sign-in")
                            .label("Sign in")
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(OpenAccount), cx)
                            }),
                    ),
                )
                .into_any_element();
        }

        let directory = &snapshot.device_directory;
        let local = directory
            .devices
            .iter()
            .find(|device| is_local(device))
            .cloned();
        let pending = directory.enrollment_payload.clone();
        let refreshing = self.waiting("refresh devices");

        let this_desktop: AnyElement = if let Some(payload) = pending {
            let fingerprint = directory.pending_fingerprint.clone().unwrap_or_default();
            let (copy_payload, copy_fingerprint) = (payload.clone(), fingerprint.clone());
            let finishing = self.waiting("complete device enrollment");
            div()
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(text::body_strong("Finish registering this desktop").text_color(p.text))
                .child(
                    text::small(
                        "On a device you already use, open Settings, then Devices, and approve \
                         this request. Check that it shows the same fingerprint.",
                    )
                    .text_color(p.text_muted),
                )
                .child(field::field(
                    p,
                    "Request",
                    copy_field("device-copy-enrollment", "laptop", payload)
                        .action("Copy request")
                        .primary()
                        .copied(self.copied == Some(Copied::Enrollment))
                        .on_copy(cx.listener(move |this, _, _, cx| {
                            this.copy(copy_payload.clone(), Copied::Enrollment, cx)
                        })),
                ))
                .child(field::field(
                    p,
                    "Fingerprint",
                    copy_field("device-copy-fingerprint", "shield", fingerprint)
                        .copied(self.copied == Some(Copied::Fingerprint))
                        .on_copy(cx.listener(move |this, _, _, cx| {
                            this.copy(copy_fingerprint.clone(), Copied::Fingerprint, cx)
                        })),
                ))
                .child(
                    div().flex().justify_end().child(
                        button::primary("complete-device-enrollment")
                            .label("It is approved")
                            .loading(finishing)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if !this.waiting("complete device enrollment") {
                                    this.send(ClientCommand::CompleteDeviceEnrollment, cx);
                                }
                            })),
                    ),
                )
                .into_any_element()
        } else if let Some(device) = &local {
            let (label, tone) = trust(device.trust);
            div()
                .flex()
                .items_center()
                .gap(px(14.))
                .child(row::icon_tile(
                    p,
                    platform_icon(&device.platform),
                    44.,
                    true,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            text::body_strong(device.name.clone())
                                .single_line()
                                .text_color(p.text),
                        )
                        .child(
                            text::caption(format!(
                                "{} · {}",
                                platform_name(&device.platform),
                                device.fingerprint
                            ))
                            .single_line()
                            .text_color(p.text_muted),
                        ),
                )
                .child(feedback::pill(p, label, tone))
                .into_any_element()
        } else {
            let registering = self.waiting("enroll device");
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(text::body_strong("Register this desktop").text_color(p.text))
                .child(
                    text::small(
                        "Your other devices can then send straight to it. The first device is \
                         active at once; later ones are approved from one you already use.",
                    )
                    .text_color(p.text_muted),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(field::input(p, &self.device_name).id("device-name")),
                        )
                        .child(
                            button::primary("enroll-device")
                                .label("Register")
                                .loading(registering)
                                .on_click(cx.listener(|this, _, _, cx| this.register_device(cx))),
                        ),
                )
                .into_any_element()
        };

        let others: Vec<DeviceSnapshot> = directory
            .devices
            .iter()
            .filter(|device| !is_local(device))
            .cloned()
            .collect();
        let can_approve = local
            .as_ref()
            .is_some_and(|device| device.trust != DeviceTrust::Pending);
        let approving = self.waiting("approve device enrollment");
        let daemon = &directory.daemon;
        let receiving = daemon.receive_enabled;
        let (toggle_receive, toggle_auto) = (cx.entity().downgrade(), cx.entity().downgrade());

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .children(self.notice(Area::Devices, p))
            .when(
                directory.availability != DeviceDirectoryAvailability::Available,
                |this| {
                    this.children(
                        directory
                            .detail
                            .clone()
                            .map(|detail| feedback::notice(p, detail, Tone::Warning)),
                    )
                },
            )
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(text::eyebrow("This desktop").text_color(p.text_subtle))
                            .child(
                                button::ghost("refresh-devices")
                                    .small()
                                    .icon("updates")
                                    .label("Refresh")
                                    .loading(refreshing)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if !this.waiting("refresh devices") {
                                            this.send(ClientCommand::RefreshDevices, cx);
                                        }
                                    })),
                            ),
                    )
                    .child(this_desktop),
            )
            .when(!others.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(
                            div().px(px(4.)).child(
                                text::eyebrow("Your other devices").text_color(p.text_subtle),
                            ),
                        )
                        .child(
                            surface::panel(p).hugged().flex().flex_col().children(
                                others
                                    .into_iter()
                                    .map(|device| self.device_row(device, p, cx)),
                            ),
                        ),
                )
            })
            .when(can_approve, |this| {
                this.child(
                    surface::panel(p)
                        .p(px(22.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .child(text::eyebrow("Approve a new device").text_color(p.text_subtle))
                        .child(
                            text::small(
                                "Only approve a device you are holding. Compare its fingerprint \
                                 by eye before entering it here.",
                            )
                            .text_color(p.text_muted),
                        )
                        .child(field::field(
                            p,
                            "Request from the new device",
                            field::input(p, &self.approval_payload).id("approval-payload"),
                        ))
                        .child(field::field(
                            p,
                            "Its fingerprint",
                            field::input(p, &self.approval_fingerprint).id("approval-fingerprint"),
                        ))
                        .child(
                            div().flex().justify_end().child(
                                button::primary("approve-device")
                                    .label("Approve device")
                                    .loading(approving)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.approve_device(cx)),
                                    ),
                            ),
                        ),
                )
            })
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                text::eyebrow("Receiving on this desktop")
                                    .text_color(p.text_subtle),
                            )
                            .child(if daemon.connected {
                                feedback::pill(p, "Service running", Tone::Success)
                            } else {
                                feedback::pill(p, "Service not running", Tone::Neutral)
                            }),
                    )
                    .when(!daemon.connected, |this| {
                        let installing = self.waiting("install device service");
                        this.child(
                            feedback::notice(
                                p,
                                daemon.detail.clone().unwrap_or_else(|| {
                                    "A small background service receives from your devices while \
                                     Filebeam is closed."
                                        .to_owned()
                                }),
                                Tone::Neutral,
                            )
                            .items_center()
                            .child(
                                button::secondary("install-device-service")
                                    .small()
                                    .label("Set it up")
                                    .loading(installing)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if !this.waiting("install device service") {
                                            this.send(ClientCommand::InstallDeviceService, cx);
                                        }
                                    })),
                            ),
                        )
                    })
                    .child(
                        row::switch_row(
                            "toggle-device-receiving",
                            "Receive from my devices",
                            receiving,
                        )
                        .description("Verified devices can send to the folders below.")
                        .disabled(!daemon.connected)
                        .on_toggle(move |on, _, cx| {
                            let on = *on;
                            let _ = toggle_receive.update(cx, |this, cx| {
                                if !this.waiting("set device receiving") {
                                    this.send(ClientCommand::SetDeviceReceiving(on), cx);
                                }
                            });
                        }),
                    )
                    .when(!daemon.destinations.is_empty(), |this| {
                        this.child(surface::hairline(p))
                            .children(daemon.destinations.iter().map(|destination| {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .py(px(4.))
                                    .child(icon("folder").size(px(18.)).text_color(p.text_muted))
                                    .child(
                                        text::body_strong(destination.alias.clone())
                                            .flex_1()
                                            .single_line()
                                            .text_color(p.text),
                                    )
                                    .child(feedback::pill(
                                        p,
                                        if !destination.enabled {
                                            "Off"
                                        } else if destination.approval.eq_ignore_ascii_case("auto")
                                        {
                                            "Saves without asking"
                                        } else {
                                            "Asks first"
                                        },
                                        if destination.enabled {
                                            Tone::Accent
                                        } else {
                                            Tone::Neutral
                                        },
                                    ))
                            }))
                    })
                    .child(surface::hairline(p))
                    .child(field::label(p, "Add a folder", false))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div().flex_1().min_w_0().child(
                                    field::input(p, &self.dropzone_alias).id("dropzone-alias"),
                                ),
                            )
                            .child(
                                button::secondary("choose-device-dropzone")
                                    .icon("folder")
                                    .label("Choose folder…")
                                    .disabled(!daemon.connected)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.choose_dropzone(window, cx)
                                    })),
                            ),
                    )
                    .child(
                        row::switch_row(
                            "dropzone-auto-confirmed",
                            "Save to it without asking",
                            self.dropzone_auto,
                        )
                        .description("Otherwise each transfer waits for you to accept it.")
                        .on_toggle(move |on, _, cx| {
                            let on = *on;
                            let _ = toggle_auto.update(cx, |this, cx| {
                                this.dropzone_auto = on;
                                cx.notify();
                            });
                        }),
                    ),
            )
            .into_any_element()
    }

    /// Another device, and when opened the things that can be done to it.
    fn device_row(&self, device: DeviceSnapshot, p: Palette, cx: &mut Context<Self>) -> gpui::Div {
        let open = self.expanded.as_deref() == Some(device.id.as_str());
        let (label, tone) = trust(device.trust);
        let toggle = device.id.clone();
        let header = row::hugging(p, row::Fit::PANEL, false)
            .id(SharedString::from(format!("device-{}", device.id)))
            .child(row::lead_tile(
                p,
                row::Fit::PANEL,
                platform_icon(&device.platform),
                false,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        text::body_strong(device.name.clone())
                            .single_line()
                            .text_color(p.text),
                    )
                    .child(
                        text::caption(format!(
                            "{} · {}",
                            platform_name(&device.platform),
                            device.fingerprint
                        ))
                        .single_line()
                        .text_color(p.text_muted),
                    ),
            )
            .child(feedback::pill(p, label, tone))
            .child(
                icon(if open { "chevron-up" } else { "chevron-down" })
                    .size(px(16.))
                    .flex_none()
                    .text_color(p.text_subtle),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                let opening = this.expanded.as_deref() != Some(toggle.as_str());
                this.expanded = opening.then(|| toggle.clone());
                for input in [&this.rename_name, &this.verify_fingerprint] {
                    Self::clear(&input.clone(), window, cx);
                }
                cx.notify();
            }));

        let (rename_id, verify_id, revoked) =
            (device.id.clone(), device.id.clone(), device.clone());
        let renaming = self.waiting("rename device");
        let verifying = self.waiting("confirm device verification");
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(row::slot().child(header))
            .when(open, |this| {
                this.child(
                    row::slot().child(
                        surface::hugging_well(p, radius::PANEL)
                            .p(px(14.))
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .when(device.trust == DeviceTrust::Registered, |this| {
                                this.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(8.))
                                        .child(field::label(p, "Verify this device", false))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.))
                                                .child(
                                                    div().flex_1().min_w_0().child(
                                                        field::input(p, &self.verify_fingerprint)
                                                            .id("verify-fingerprint"),
                                                    ),
                                                )
                                                .child(
                                                    button::primary("confirm-device-peer")
                                                        .small()
                                                        .label("Verify")
                                                        .loading(verifying)
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                this.verify_device(
                                                                    verify_id.clone(),
                                                                    cx,
                                                                )
                                                            },
                                                        )),
                                                ),
                                        )
                                        .child(field::hint(
                                            p,
                                            "Read the fingerprint from that device's own screen. \
                                         Until it matches, nothing is sent to it.",
                                            Tone::Neutral,
                                        )),
                                )
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.))
                                    .child(field::label(p, "Rename", false))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(
                                                div().flex_1().min_w_0().child(
                                                    field::input(p, &self.rename_name)
                                                        .id("rename-device-name"),
                                                ),
                                            )
                                            .child(
                                                button::secondary("rename-device")
                                                    .small()
                                                    .label("Rename")
                                                    .loading(renaming)
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            this.rename_device(
                                                                rename_id.clone(),
                                                                cx,
                                                            )
                                                        },
                                                    )),
                                            ),
                                    ),
                            )
                            .when(device.trust != DeviceTrust::Revoked, |this| {
                                this.child(surface::hairline(p)).child(
                                    div().flex().child(
                                        button::danger("revoke-device")
                                            .small()
                                            .icon("slash")
                                            .label("Revoke this device…")
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.confirm_revoke(revoked.clone(), window, cx)
                                            })),
                                    ),
                                )
                            }),
                    ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platforms_and_trust_are_named_for_people() {
        assert_eq!(platform_name("macos"), "macOS");
        assert_eq!(platform_name("Linux"), "Linux");
        assert_eq!(platform_name("haiku"), "haiku");
        assert_eq!(platform_icon("android"), "mobile");
        assert_eq!(platform_icon("windows"), "os-windows");
        assert_eq!(platform_icon("something"), "laptop");
        assert_eq!(trust(DeviceTrust::Registered).0, "Not verified");
        assert_eq!(trust(DeviceTrust::Verified), ("Verified", Tone::Success));
    }
}
