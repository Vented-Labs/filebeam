use gpui::{TestAppContext, WindowHandle, test::TestWindowExt as _};
use gpui_component::Root;

use super::*;
use crate::{
    model::{
        AccountSnapshot, ClientError, ClientErrorCode, ClientMessage, KeyCustody, OperationStatus,
    },
    testing::{self, Commands},
};

fn signed_in(key_custody: KeyCustody) -> DesktopSnapshot {
    DesktopSnapshot {
        account: AccountSnapshot {
            authenticated: true,
            email: Some("ada@example.test".into()),
            username: Some("ada".into()),
            profile_url: Some("https://files.example/@ada".into()),
            email_verified: true,
            inbox_enabled: true,
            key_custody,
            ..AccountSnapshot::default()
        },
        ..DesktopSnapshot::default()
    }
}

fn panel(
    cx: &mut TestAppContext,
    snapshot: DesktopSnapshot,
) -> (WindowHandle<Root>, Entity<AccountPanel>, Commands) {
    testing::rooted(cx, snapshot, AccountPanel::new)
}

fn set(
    cx: &mut TestAppContext,
    window: WindowHandle<Root>,
    input: &Entity<InputState>,
    value: &'static str,
) {
    testing::in_window(cx, window, |window, cx| {
        input.update(cx, |input, cx| input.set_value(value, window, cx));
    });
}

fn failed(id: u64, operation: &str, detail: &str) -> ClientMessage {
    ClientMessage {
        id,
        operation: operation.into(),
        error: ClientError {
            code: ClientErrorCode::Remote,
            detail: detail.into(),
        },
    }
}

fn completed(id: u64, operation: &str) -> OperationStatus {
    OperationStatus {
        id,
        operation: operation.into(),
    }
}

#[test]
fn forms_are_checked_before_anything_is_sent() {
    use super::auth::validate;
    assert_eq!(
        validate(AuthMode::SignIn, "", "", "x"),
        Err("Enter your email or username.")
    );
    assert_eq!(
        validate(AuthMode::SignIn, "", "ada", ""),
        Err("Enter your password.")
    );
    assert_eq!(validate(AuthMode::SignIn, "", "ada", "x"), Ok(()));
    assert_eq!(
        validate(AuthMode::Register, "", "a@b.c", "longenough"),
        Err("Choose a username.")
    );
    assert!(validate(AuthMode::Register, "Ada!", "a@b.c", "longenough").is_err());
    assert!(validate(AuthMode::Register, "ada", "not-an-email", "longenough").is_err());
    assert!(validate(AuthMode::Register, "ada", "a@b.c", "short").is_err());
    assert_eq!(
        validate(AuthMode::Register, "ada", "a@b.c", "longenough"),
        Ok(())
    );
    assert!(validate(AuthMode::Reset, "", "ada", "").is_err());
    assert_eq!(validate(AuthMode::Reset, "", "a@b.c", ""), Ok(()));
}

#[test]
fn failures_are_worded_for_what_was_attempted() {
    assert_eq!(
        failure("login", "native login returned 422 Unprocessable Entity"),
        "That email and password do not match an account."
    );
    assert_eq!(
        failure("login", "native login returned 429 Too Many Requests"),
        "Too many attempts. Wait a moment and try again."
    );
    assert!(failure("register", "native registration returned 422").contains("already be taken"));
    assert!(
        failure(
            "delete account",
            "native account deletion returned 422 Unprocessable Entity"
        )
        .contains("nothing was deleted")
    );
    assert_eq!(
        failure(
            "unlock receiving key",
            "operation failed; sensitive detail was withheld"
        ),
        "That was not accepted. Check the password and try again."
    );
    assert_eq!(area("login"), Area::Auth);
    assert_eq!(area("update contact"), Area::Contacts);
    assert_eq!(area("export receiving key"), Area::Key);
    assert_eq!(area("logout"), Area::Profile);
}

#[test]
fn a_report_accepts_an_id_or_the_link_it_came_from() {
    use super::profile::reported_transfer;
    let id = "01JD8Q4XK7M2N9P3R5T6V8W0YZ";
    assert_eq!(reported_transfer(id).as_deref(), Some(id));
    assert_eq!(
        reported_transfer(&format!(" https://files.example/{id}#k=v1.secret ")).as_deref(),
        Some(id)
    );
    assert_eq!(
        reported_transfer(&format!("https://files.example/n/{id}/?x=1")).as_deref(),
        Some(id)
    );
    assert_eq!(reported_transfer("not a transfer"), None);
    assert_eq!(reported_transfer(""), None);
}

#[gpui::test]
fn signing_in_validates_submits_on_enter_and_explains_a_refusal(cx: &mut TestAppContext) {
    let mut snapshot = DesktopSnapshot::default();
    let (window, panel, commands) = panel(cx, snapshot.clone());
    testing::in_window(cx, window, |window, cx| {
        window.render_frame(cx);
        window.click("account-submit", cx);
    });
    assert!(commands.try_recv().is_err(), "an empty form is not sent");
    panel.read_with(cx, |panel, _| {
        assert_eq!(
            panel.notice.as_ref().map(|notice| notice.text.as_str()),
            Some("Enter your email or username.")
        );
    });

    let (email, password) =
        panel.read_with(cx, |panel, _| (panel.email.clone(), panel.password.clone()));
    set(cx, window, &email, " ada@example.test ");
    set(cx, window, &password, "hunter2 ");
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| {
            panel.submit_auth(window, cx);
            panel.submit_auth(window, cx);
        });
    });
    assert!(matches!(
        commands.try_recv(),
        Ok(ClientCommand::Login { email, password, remember: true })
            if email == "ada@example.test" && password == "hunter2 "
    ));
    assert!(
        commands.try_recv().is_err(),
        "a pending sign-in is not repeated"
    );

    snapshot
        .messages
        .push(failed(1, "login", "native login returned 422"));
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| {
            panel.on_snapshot(&snapshot, window, cx);
            assert!(!panel.waiting("login"));
            assert_eq!(
                panel.notice.as_ref().map(|notice| notice.text.as_str()),
                Some("That email and password do not match an account.")
            );
        });
        window.render_frame(cx);
    });
}

#[gpui::test]
fn signing_in_clears_secrets_and_opens_the_profile(cx: &mut TestAppContext) {
    let (window, panel, commands) = panel(cx, DesktopSnapshot::default());
    let password = panel.read_with(cx, |panel, _| panel.password.clone());
    set(cx, window, &password, "hunter2");
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| {
            panel.set_auth_mode(AuthMode::Register, cx);
            panel.on_snapshot(&signed_in(KeyCustody::Unknown), window, cx);
            assert_eq!(panel.tab, Tab::Profile);
            assert_eq!(panel.auth, AuthMode::SignIn);
            assert!(panel.password.read(cx).value().is_empty());
        });
    });
    assert!(commands.try_recv().is_err());
}

#[gpui::test]
fn a_change_shows_at_once_and_settles_with_the_service(cx: &mut TestAppContext) {
    let mut snapshot = signed_in(KeyCustody::Unknown);
    let (window, panel, commands) = panel(cx, snapshot.clone());
    panel.update(cx, |panel, cx| {
        panel.change("inbox", 0, ClientCommand::SetInboxEnabled(false), cx);
        assert_eq!(panel.shown("inbox", 1), 0);
        // A second change waits for the first to be answered.
        panel.change("inbox", 1, ClientCommand::SetInboxEnabled(true), cx);
        assert_eq!(panel.shown("inbox", 1), 0);
    });
    assert!(matches!(
        commands.try_recv(),
        Ok(ClientCommand::SetInboxEnabled(false))
    ));
    assert!(commands.try_recv().is_err());

    // A refusal returns the control to what the service still says.
    snapshot
        .messages
        .push(failed(1, "set inbox enabled", "request returned 503"));
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| {
            panel.on_snapshot(&snapshot, window, cx);
            assert_eq!(panel.shown("inbox", 1), 1);
            assert_eq!(
                panel.notice.as_ref().map(|notice| notice.area),
                Some(Area::Receiving)
            );
        });
    });
}

#[gpui::test]
fn deleting_needs_the_password_and_the_typed_word(cx: &mut TestAppContext) {
    let mut snapshot = signed_in(KeyCustody::Unknown);
    let (window, panel, commands) = panel(cx, snapshot.clone());
    let confirm = |cx: &mut TestAppContext| {
        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            window.click("dialog-confirm", cx);
        });
    };
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| panel.confirm_delete(window, cx));
    });
    confirm(cx);
    assert!(commands.try_recv().is_err());

    let (password, phrase) = panel.read_with(cx, |panel, _| {
        (panel.confirm_password.clone(), panel.confirm_phrase.clone())
    });
    set(cx, window, &password, "hunter2");
    set(cx, window, &phrase, "delete");
    confirm(cx);
    assert!(commands.try_recv().is_err(), "the word must match exactly");
    panel.read_with(cx, |panel, _| {
        assert_eq!(
            panel.notice.as_ref().map(|notice| notice.area),
            Some(Area::Delete)
        );
    });

    set(cx, window, &phrase, "DELETE");
    confirm(cx);
    assert!(matches!(
        commands.try_recv(),
        Ok(ClientCommand::DeleteAccount { current_password, confirmation })
            if current_password == "hunter2" && confirmation == "DELETE"
    ));

    // A refused password is reported on the page and never left in its field.
    snapshot.messages.push(failed(
        1,
        "delete account",
        "native account deletion returned 422",
    ));
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| {
            panel.on_snapshot(&snapshot, window, cx);
            assert_eq!(
                panel.notice.as_ref().map(|notice| notice.area),
                Some(Area::Profile)
            );
            assert!(panel.confirm_password.read(cx).value().is_empty());
        });
    });
}

#[gpui::test]
fn a_report_is_checked_then_sent_with_its_category(cx: &mut TestAppContext) {
    let mut snapshot = DesktopSnapshot::default();
    let (window, panel, commands) = panel(cx, snapshot.clone());
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| panel.open_report(window, cx));
    });
    let confirm = |cx: &mut TestAppContext| {
        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            window.click("dialog-confirm", cx);
        });
    };
    confirm(cx);
    assert!(commands.try_recv().is_err());

    let id = panel.read_with(cx, |panel, _| panel.report_id.clone());
    set(
        cx,
        window,
        &id,
        "https://files.example/01JD8Q4XK7M2N9P3R5T6V8W0YZ#k=v1.x",
    );
    confirm(cx);
    assert!(commands.try_recv().is_err(), "a description is required");
    testing::in_window(cx, window, |window, cx| {
        let description = panel.read(cx).report_description.clone();
        description.update(cx, |input, cx| {
            input.set_value("This link serves malware.", window, cx)
        });
    });
    confirm(cx);
    let Ok(ClientCommand::Report {
        transfer_id,
        category,
        description,
        email,
    }) = commands.try_recv()
    else {
        panic!("the report is sent")
    };
    assert_eq!(transfer_id, "01JD8Q4XK7M2N9P3R5T6V8W0YZ");
    assert_eq!(category, "spam");
    assert_eq!(description, "This link serves malware.");
    assert_eq!(email, None);

    snapshot.operations.push(completed(1, "report transfer"));
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| {
            panel.on_snapshot(&snapshot, window, cx);
            let notice = panel.notice.clone().unwrap();
            assert_eq!((notice.area, notice.tone), (Area::Auth, Tone::Success));
            assert!(panel.report_id.read(cx).value().is_empty());
        });
    });
}

#[gpui::test]
fn key_actions_follow_the_key_state(cx: &mut TestAppContext) {
    // No key: creating one with the account password needs that password.
    let (window, panel, commands) = panel(cx, signed_in(KeyCustody::Unknown));
    panel.update(cx, |panel, cx| {
        panel.show_key(cx);
        panel.submit_key_password(cx);
        assert_eq!(
            panel.notice.as_ref().map(|notice| notice.area),
            Some(Area::Key)
        );
    });
    assert!(commands.try_recv().is_err());
    let key_password = panel.read_with(cx, |panel, _| panel.key_password.clone());
    set(cx, window, &key_password, "hunter2");
    testing::in_window(cx, window, |window, cx| {
        window.render_frame(cx);
        window.click("key-generate-password", cx);
    });
    assert!(matches!(
        commands.try_recv(),
        Ok(ClientCommand::GenerateReceivingKey {
            password: Some(password),
            replace: false,
            acknowledge_old_key_loss: false,
        }) if password == "hunter2"
    ));

    // Locked: Enter in the password field unlocks.
    let locked = KeyCustody::Locked {
        version: 2,
        fingerprint: "ABCD 1234".into(),
        custody_mode: "password".into(),
    };
    let (window, panel, commands) = self::panel(cx, signed_in(locked));
    let key_password = panel.read_with(cx, |panel, _| panel.key_password.clone());
    set(cx, window, &key_password, "hunter2");
    panel.update(cx, |panel, cx| {
        panel.show_key(cx);
        panel.submit_key_password(cx);
    });
    assert!(matches!(
        commands.try_recv(),
        Ok(ClientCommand::UnlockReceivingKey { password }) if password == "hunter2"
    ));

    // Ready: replacing asks for the password and states what is lost.
    let ready = KeyCustody::Configured {
        version: 2,
        fingerprint: "ABCD 1234".into(),
        custody_mode: "password".into(),
    };
    let (window, panel, commands) = self::panel(cx, signed_in(ready));
    panel.update(cx, |panel, cx| panel.show_key(cx));
    testing::in_window(cx, window, |window, cx| {
        window.render_frame(cx);
        window.click("key-replace", cx);
    });
    testing::in_window(cx, window, |window, cx| {
        window.render_frame(cx);
        window.click("dialog-confirm", cx);
    });
    assert!(commands.try_recv().is_err(), "replacing needs the password");
    let confirm_password = panel.read_with(cx, |panel, _| panel.confirm_password.clone());
    set(cx, window, &confirm_password, "hunter2");
    testing::in_window(cx, window, |window, cx| {
        window.render_frame(cx);
        window.click("dialog-confirm", cx);
    });
    assert!(matches!(
        commands.try_recv(),
        Ok(ClientCommand::GenerateReceivingKey {
            password: Some(_),
            replace: true,
            acknowledge_old_key_loss: true,
        })
    ));
}

#[gpui::test]
fn a_contact_request_is_normalised_and_confirmed(cx: &mut TestAppContext) {
    let mut snapshot = signed_in(KeyCustody::Unknown);
    let (window, panel, commands) = panel(cx, snapshot.clone());
    panel.update(cx, |panel, cx| panel.select_tab(Tab::Contacts, cx));
    assert!(matches!(
        commands.try_recv(),
        Ok(ClientCommand::RefreshContacts)
    ));

    let contact = panel.read_with(cx, |panel, _| panel.contact.clone());
    set(cx, window, &contact, "al");
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| panel.request_contact(window, cx));
    });
    assert!(commands.try_recv().is_err());
    set(cx, window, &contact, "@Grace_H");
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| panel.request_contact(window, cx));
    });
    assert!(matches!(
        commands.try_recv(),
        Ok(ClientCommand::ContactAction { username, action, .. })
            if username == "grace_h" && action == "request"
    ));

    snapshot.operations.push(completed(1, "update contact"));
    testing::in_window(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| {
            panel.on_snapshot(&snapshot, window, cx);
            assert_eq!(
                panel.notice.as_ref().map(|notice| notice.text.as_str()),
                Some("Request sent to @grace_h.")
            );
            assert!(panel.contact.read(cx).value().is_empty());
        });
    });
}

#[gpui::test]
fn every_section_renders_for_every_key_state(cx: &mut TestAppContext) {
    use filebeam_client_core::services::{
        Contact, ContactIdentity, Contacts, ReceivingDefaults, contacts::EffectiveReceiving,
    };
    let contact = |username: &str, status: &str| Contact {
        id: 1,
        username: username.into(),
        name: username.to_uppercase(),
        status: status.into(),
        can_send: None,
        auto_download: Some(true),
        effective: EffectiveReceiving {
            can_send: true,
            auto_download: true,
        },
    };
    for custody in [
        KeyCustody::Unknown,
        KeyCustody::PendingBackup {
            generation_id: "g".into(),
            fingerprint: "ABCD 1234".into(),
        },
        KeyCustody::Locked {
            version: 1,
            fingerprint: "ABCD 1234".into(),
            custody_mode: "self".into(),
        },
        KeyCustody::Configured {
            version: 1,
            fingerprint: "ABCD 1234".into(),
            custody_mode: "password".into(),
        },
    ] {
        let mut snapshot = signed_in(custody);
        snapshot.account.email_verified = false;
        snapshot.contacts = Some(Contacts {
            settings: ReceivingDefaults {
                receiving_policy: "friends".into(),
                auto_download_friends: true,
                revision: 1,
            },
            contacts: vec![
                contact("grace", "accepted"),
                contact("alan", "incoming"),
                contact("edsger", "outgoing"),
            ],
            blocked: vec![ContactIdentity {
                id: 9,
                username: "mallory".into(),
                name: "Mallory".into(),
            }],
        });
        let (window, panel, _commands) = panel(cx, snapshot);
        for tab in Tab::ALL {
            panel.update(cx, |panel, cx| {
                panel.tab = tab;
                panel.expanded = Some("grace".into());
                cx.notify();
            });
            testing::in_window(cx, window, |window, cx| window.render_frame(cx));
        }
    }
    for mode in [AuthMode::SignIn, AuthMode::Register, AuthMode::Reset] {
        let (window, panel, _commands) = panel(cx, DesktopSnapshot::default());
        panel.update(cx, |panel, cx| panel.set_auth_mode(mode, cx));
        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("account-submit").is_some());
        });
    }
}
