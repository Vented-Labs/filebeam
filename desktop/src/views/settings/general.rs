//! The instance, desktop behaviour and updates; transfer limits; and what this
//! build is.

use std::path::{Path, PathBuf};

use filebeam_client_config::{
    Config, MAX_CONCURRENCY, MAX_MEMORY_LIMIT_MIB, MIN_MEMORY_LIMIT_MIB, normalize_server_url,
};
use filebeam_client_updater::{Options, Product, Updater};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement, Styled, Window, div, prelude::FluentBuilder,
    px,
};

use super::{
    Area, Copied, SettingsEvent, SettingsPanel, UpdateCheck, config_home, parse_concurrency,
    parse_memory,
};
use crate::{
    VERSION,
    model::{ClientCommand, DesktopSnapshot, PolicyAvailability},
    theme::Palette,
    ui::{
        TextExt, Tone, button, feedback,
        field::{self, copy_field},
        format, row, surface, text,
    },
};

fn manual_update(home: PathBuf) -> anyhow::Result<String> {
    let config = Config::load(config_home(&home))?;
    let key = option_env!("FILEBEAM_RELEASE_PUBLIC_KEY")
        .filter(|key| !key.is_empty())
        .ok_or_else(|| anyhow::anyhow!("this build has no embedded release key"))?;
    Updater::new(Options::for_product(
        config.home,
        Product::Desktop,
        VERSION,
        key,
    )?)
    .run_manual()
}

/// What an update check found, in a sentence.
fn update_result(result: &str) -> String {
    match result {
        "already on the latest release" => "You are on the latest release.".into(),
        "update check is not due" => "Filebeam checked recently. Try again later.".into(),
        staged => format!("{staged} is downloaded and installs the next time Filebeam starts."),
    }
}

fn update_failure(error: &str) -> String {
    if error.contains("no embedded release key") {
        "This build was not made by a release, so it cannot update itself.".into()
    } else {
        format::readable_error(error)
    }
}

pub(super) fn config_path(home: &Path) -> String {
    config_home(home)
        .map(|path| path.join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("~/.filebeam/config.toml"))
        .display()
        .to_string()
}

fn data_path(home: &Path) -> String {
    config_home(home)
        .map(|path| path.join("transfers"))
        .unwrap_or_else(|| PathBuf::from("~/.filebeam/transfers"))
        .display()
        .to_string()
}

impl SettingsPanel {
    pub(super) fn save_instance(&mut self, cx: &mut Context<Self>) {
        if self.waiting("change instance") {
            return;
        }
        let draft = Self::text(&self.server, cx);
        if draft.is_empty() {
            self.say(
                Area::Instance,
                Tone::Danger,
                "Enter the instance's address.",
                cx,
            );
            return;
        }
        match normalize_server_url(&draft) {
            Ok(url) if url == self.snapshot(cx).instance.url => {
                self.say(
                    Area::Instance,
                    Tone::Neutral,
                    "Already using this instance.",
                    cx,
                );
            }
            Ok(url) => self.send(ClientCommand::ChangeInstance { url }, cx),
            Err(_) => self.say(
                Area::Instance,
                Tone::Danger,
                "Enter a full address, such as https://files.example.com.",
                cx,
            ),
        }
    }

    fn set_notifications(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.notifications = enabled;
        cx.emit(SettingsEvent::Notifications(enabled));
        self.persist(
            Area::Desktop,
            false,
            window.window_handle(),
            cx,
            move |config| {
                config.desktop.notifications = enabled;
            },
        );
    }

    fn set_background_transfers(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.background_transfers = enabled;
        cx.emit(SettingsEvent::BackgroundTransfers(enabled));
        self.persist(
            Area::Desktop,
            false,
            window.window_handle(),
            cx,
            move |config| {
                config.desktop.background_transfers = enabled;
            },
        );
    }

    fn set_auto_update(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.auto_update = enabled;
        self.persist(
            Area::Updates,
            false,
            window.window_handle(),
            cx,
            move |config| {
                config.updates.auto_update = enabled;
            },
        );
    }

    fn set_relay_only(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.relay_only = enabled;
        self.persist(
            Area::Network,
            true,
            window.window_handle(),
            cx,
            move |config| {
                config.webrtc.relay_only = enabled;
            },
        );
    }

    pub(super) fn save_limits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.waiting("reload settings") {
            return;
        }
        let memory = parse_memory(self.memory_limit.read(cx).value().as_ref());
        let concurrency = parse_concurrency(self.concurrency.read(cx).value().as_ref());
        match (memory, concurrency) {
            (Ok(memory), Ok(concurrency)) => {
                self.persist(
                    Area::Network,
                    true,
                    window.window_handle(),
                    cx,
                    move |config| {
                        config.transfers.memory_limit_mib = memory;
                        config.transfers.max_concurrency = concurrency;
                    },
                );
            }
            (Err(problem), _) | (_, Err(problem)) => {
                self.say(Area::Network, Tone::Danger, problem, cx);
            }
        }
    }

    fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if self.update == UpdateCheck::Checking {
            return;
        }
        self.update = UpdateCheck::Checking;
        self.clear_notice(Area::Updates, cx);
        let home = self.home.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor.spawn(async move { manual_update(home) }).await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(found) => this.update = UpdateCheck::Done(update_result(&found)),
                    Err(error) => {
                        this.update = UpdateCheck::Idle;
                        this.say(
                            Area::Updates,
                            Tone::Danger,
                            update_failure(&error.to_string()),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn general(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let changing = self.waiting("change instance");
        let (status, tone) = match (snapshot.instance.connected, snapshot.policy.availability) {
            (true, PolicyAvailability::Available) => ("Connected", Tone::Success),
            (_, PolicyAvailability::Cached) => ("Using saved details", Tone::Warning),
            (true, _) => ("Connected", Tone::Success),
            _ => ("Not reachable", Tone::Danger),
        };
        let tray = self
            .presence
            .as_ref()
            .is_some_and(|presence| presence.available);
        let checking = self.update == UpdateCheck::Checking;
        let (notify, background, updates) = (
            cx.entity().downgrade(),
            cx.entity().downgrade(),
            cx.entity().downgrade(),
        );

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.))
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
                            .child(text::eyebrow("Instance").text_color(p.text_subtle))
                            .child(feedback::pill(p, status, tone)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div().flex_1().min_w_0().child(
                                    field::input(p, &self.server).id("settings-server").prefix(
                                        crate::assets::icon("globe")
                                            .size(px(15.))
                                            .text_color(p.text_subtle),
                                    ),
                                ),
                            )
                            .child(
                                button::primary("save-server")
                                    .label("Use this instance")
                                    .loading(changing)
                                    .on_click(cx.listener(|this, _, _, cx| this.save_instance(cx))),
                            ),
                    )
                    .children(self.notice(Area::Instance, p))
                    .when_some(
                        snapshot
                            .instance
                            .detail
                            .clone()
                            .filter(|_| !snapshot.instance.connected),
                        |this, detail| {
                            this.child(
                                feedback::notice(p, format::readable_error(&detail), Tone::Danger)
                                    .items_center()
                                    .child(
                                        button::quiet("check-connection")
                                            .small()
                                            .label("Try again")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                let _ =
                                                    this.client.dispatch(ClientCommand::Refresh);
                                                cx.notify();
                                            })),
                                    ),
                            )
                        },
                    )
                    .child(field::hint(
                        p,
                        "New transfers use this instance, and each instance has its own account. \
                         Transfers already running keep the one they started on.",
                        Tone::Neutral,
                    )),
            )
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(text::eyebrow("On this desktop").text_color(p.text_subtle))
                    .children(self.notice(Area::Desktop, p))
                    .child(
                        row::switch_row(
                            "notifications",
                            "Notify me when a transfer finishes",
                            self.notifications && self.notifications_available,
                        )
                        .icon("bell")
                        .description(if self.notifications_available {
                            "A system notification without file names or links.".to_owned()
                        } else {
                            "System notifications are not available here.".to_owned()
                        })
                        .disabled(!self.notifications_available)
                        .on_toggle(move |on, window, cx| {
                            let on = *on;
                            let _ = notify.update(cx, |this, cx| {
                                this.set_notifications(on, window, cx);
                            });
                        }),
                    )
                    .child(surface::hairline(p))
                    .child(
                        row::switch_row(
                            "background-transfers",
                            "Keep transferring when the window closes",
                            self.background_transfers && tray,
                        )
                        .icon("transfers")
                        .description(if tray {
                            "Filebeam stays in the system tray until transfers finish or you quit."
                                .to_owned()
                        } else {
                            "Needs a system tray, which this desktop does not provide.".to_owned()
                        })
                        .disabled(!tray)
                        .on_toggle(move |on, window, cx| {
                            let on = *on;
                            let _ = background.update(cx, |this, cx| {
                                this.set_background_transfers(on, window, cx);
                            });
                        }),
                    ),
            )
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(text::eyebrow("Updates").text_color(p.text_subtle))
                    .child(
                        row::switch_row(
                            "auto-update",
                            "Install updates automatically",
                            self.auto_update,
                        )
                        .icon("updates")
                        .description(
                            "Signed releases are downloaded and applied at the next start.",
                        )
                        .on_toggle(move |on, window, cx| {
                            let on = *on;
                            let _ = updates.update(cx, |this, cx| {
                                this.set_auto_update(on, window, cx);
                            });
                        }),
                    )
                    .child(surface::hairline(p))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .py(px(10.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        text::body_strong(format!("Filebeam {VERSION}"))
                                            .text_color(p.text),
                                    )
                                    .child(
                                        text::small(match &self.update {
                                            UpdateCheck::Done(result) => result.clone(),
                                            UpdateCheck::Checking => {
                                                "Checking for a newer release…".to_owned()
                                            }
                                            UpdateCheck::Idle => {
                                                "Check whether a newer release is available."
                                                    .to_owned()
                                            }
                                        })
                                        .text_color(p.text_muted),
                                    ),
                            )
                            .child(
                                button::secondary("manual-update")
                                    .small()
                                    .label("Check now")
                                    .loading(checking)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.check_for_updates(cx)),
                                    ),
                            ),
                    )
                    .children(self.notice(Area::Updates, p)),
            )
            .into_any_element()
    }

    pub(super) fn network(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let saving = self.waiting("reload settings");
        let relay = cx.entity().downgrade();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(text::eyebrow("Privacy").text_color(p.text_subtle))
                    .child(
                        row::switch_row(
                            "relay-only",
                            "Always relay P2P transfers",
                            self.relay_only,
                        )
                        .icon("shield")
                        .description(
                            "A direct connection shows your network address to the other \
                                 person. Relaying hides it and can be slower.",
                        )
                        .on_toggle(move |on, window, cx| {
                            let on = *on;
                            let _ = relay.update(cx, |this, cx| {
                                this.set_relay_only(on, window, cx);
                            });
                        }),
                    ),
            )
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(text::eyebrow("Limits").text_color(p.text_subtle))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(16.))
                            .child(
                                field::field(
                                    p,
                                    "Memory for transfers",
                                    field::input(p, &self.memory_limit)
                                        .id("settings-memory")
                                        .suffix(text::small("MiB").text_color(p.text_subtle)),
                                )
                                .flex_1()
                                .child(field::hint(
                                    p,
                                    format!(
                                        "{MIN_MEMORY_LIMIT_MIB} to {MAX_MEMORY_LIMIT_MIB}. More \
                                         moves large files faster."
                                    ),
                                    Tone::Neutral,
                                )),
                            )
                            .child(
                                field::field(
                                    p,
                                    "Transfers at once",
                                    field::input(p, &self.concurrency).id("settings-concurrency"),
                                )
                                .flex_1()
                                .child(field::hint(
                                    p,
                                    format!(
                                        "1 to {MAX_CONCURRENCY}. Leave empty to let Filebeam \
                                         decide."
                                    ),
                                    Tone::Neutral,
                                )),
                            ),
                    )
                    .children(self.notice(Area::Network, p))
                    .when(
                        snapshot.settings.restart_required
                            && self.notice(Area::Network, p).is_none(),
                        |this| {
                            this.child(feedback::notice(
                                p,
                                "Restart Filebeam for the saved limits to take effect.",
                                Tone::Warning,
                            ))
                        },
                    )
                    .child(
                        div().flex().justify_end().child(
                            button::primary("save-transfers")
                                .label("Save limits")
                                .loading(saving)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.save_limits(window, cx)),
                                ),
                        ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn about(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let modifier = crate::shell::modifier_label();
        let config = config_path(&self.home);
        let data = data_path(&self.home);
        let shortcut = |keys: String, action: &'static str| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.))
                .py(px(5.))
                .child(text::small(action).text_color(p.text_muted))
                .child(
                    div()
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(6.))
                        .bg(p.sunken)
                        .border_1()
                        .border_color(p.border)
                        .child(text::code(keys).text_color(p.text)),
                )
        };
        let (copy_config, copy_data) = (config.clone(), data.clone());
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .child(crate::ui::brand::mark(p, 40.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(text::title("Filebeam").text_color(p.text))
                            .child(
                                text::small(format!(
                                    "Version {VERSION} · end-to-end encrypted file and note \
                                     transfer"
                                ))
                                .single_line()
                                .text_color(p.text_muted),
                            ),
                    ),
            )
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(text::eyebrow("Files on this device").text_color(p.text_subtle))
                    .child(field::field(
                        p,
                        "Settings file",
                        copy_field("about-copy-config", "file", config)
                            .copied(self.copied == Some(Copied::ConfigPath))
                            .on_copy(cx.listener(move |this, _, _, cx| {
                                this.copy(copy_config.clone(), Copied::ConfigPath, cx)
                            })),
                    ))
                    .child(field::field(
                        p,
                        "Transfer data",
                        copy_field("about-copy-data", "storage", data)
                            .copied(self.copied == Some(Copied::DataPath))
                            .on_copy(cx.listener(move |this, _, _, cx| {
                                this.copy(copy_data.clone(), Copied::DataPath, cx)
                            })),
                    ))
                    .child(field::hint(
                        p,
                        "Settings are shared with the beam command-line client. The look of \
                         this app is not.",
                        Tone::Neutral,
                    )),
            )
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(text::eyebrow("Keyboard").text_color(p.text_subtle))
                    .child(shortcut(format!("{modifier} K"), "Go to a page"))
                    .child(shortcut(
                        format!("{modifier} 1 – 5"),
                        "Send, Receive, Transfers, Inbox, History",
                    ))
                    .child(shortcut(format!("{modifier} ,"), "Settings"))
                    .child(shortcut(
                        format!("{modifier} Shift P"),
                        "Pause the selected transfer",
                    ))
                    .child(shortcut(
                        format!("{modifier} Shift R"),
                        "Resume the selected transfer",
                    ))
                    .child(shortcut(format!("{modifier} Q"), "Quit Filebeam")),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_results_and_paths_read_plainly() {
        assert_eq!(
            update_result("already on the latest release"),
            "You are on the latest release."
        );
        assert!(update_result("v1.4.0").starts_with("v1.4.0 is downloaded"));
        assert!(update_failure("this build has no embedded release key").contains("release"));
        assert_eq!(config_path(&PathBuf::new()), "~/.filebeam/config.toml");
        assert_eq!(
            config_path(Path::new("/home/ada/.filebeam")),
            "/home/ada/.filebeam/config.toml"
        );
        assert_eq!(data_path(Path::new("/x")), "/x/transfers");
    }
}
