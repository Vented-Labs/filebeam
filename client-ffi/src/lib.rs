//! Kotlin and Swift control-plane bindings. File contents stay in Rust.

mod client;
mod error;
mod job;
mod newlinkformatter;
mod notes;
mod runtime;
mod secret_store;
mod self_test;
mod services;
mod source;
mod types;

pub use client::TransferClient;
pub use error::{ClientError, Result, invalid, operation};
pub use job::TransferJob;
pub use newlinkformatter::format_download_with_cli;
pub use notes::{CreatedNote, NoteRequest, OpenedNote};
pub use runtime::NativeRuntime;
pub use secret_store::{SecretStoreCallback, checkpoint_self_test};
pub use self_test::{crypto_self_test, webrtc_self_test};
pub use services::NativeServices;
pub use source::SourceCallback;
pub use types::*;

uniffi::setup_scaffolding!();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_origins_do_not_accept_secret_or_path_components() {
        assert_eq!(
            client::origin("https://files.example/", false).unwrap(),
            "https://files.example"
        );
        for bad in [
            "http://files.example",
            "https://u:p@files.example",
            "https://files.example/path",
            "https://files.example/#key",
            "https://files.example/?key=1",
        ] {
            assert!(client::origin(bad, false).is_err());
        }
        assert!(client::origin("http://127.0.0.1:8000", true).is_ok());
    }

    #[test]
    fn native_crypto_authenticates_the_installed_library() {
        assert!(crypto_self_test().unwrap());
    }

    #[test]
    fn upload_options_preserve_password_retention_and_transport() {
        let options = client::upload_options(UploadOptions {
            transport: Transport::WebRtc,
            archive: false,
            turbo: false,
            password: true,
            retention_hours: Some(72),
            authentication: UploadAuthentication {
                bearer_token: None,
                session_cookie: None,
            },
            recipient: None,
        });
        let options = options.unwrap();
        assert!(matches!(
            options.transport,
            filebeam_client_core::protocol::Transport::WebRtc
        ));
        assert!(options.password);
        assert_eq!(options.retention_hours, Some(72));
    }

    #[test]
    fn upload_options_reject_ambiguous_authentication_and_invalid_inbox_combinations() {
        assert!(
            client::upload_options(UploadOptions {
                transport: Transport::Http,
                archive: false,
                turbo: false,
                password: false,
                retention_hours: None,
                authentication: UploadAuthentication {
                    bearer_token: Some("one".into()),
                    session_cookie: Some("two".into()),
                },
                recipient: None,
            })
            .is_err()
        );
        assert!(
            client::upload_options(UploadOptions {
                transport: Transport::WebRtc,
                archive: false,
                turbo: true,
                password: false,
                retention_hours: None,
                authentication: UploadAuthentication {
                    bearer_token: None,
                    session_cookie: None,
                },
                recipient: None,
            })
            .is_err()
        );
        assert!(
            client::upload_options(UploadOptions {
                transport: Transport::WebRtc,
                archive: false,
                turbo: false,
                password: false,
                retention_hours: None,
                authentication: UploadAuthentication {
                    bearer_token: None,
                    session_cookie: None,
                },
                recipient: Some(UploadRecipient {
                    username: "receiver".into(),
                    user_id: 1,
                    account_key_bundle_id: 2,
                    public_key: "key".into(),
                }),
            })
            .is_err()
        );
    }

    #[test]
    fn provider_source_requires_bounded_revisioned_identity() {
        assert!(
            client::source_spec(UploadSource {
                kind: SourceKind::Provider,
                name: "document.bin".into(),
                path_or_identity: "content://provider/document/1".into(),
                offset: u64::MAX,
                length: 1,
                mutation_token: Some("v1".into()),
            })
            .is_err()
        );
        assert!(
            client::source_spec(UploadSource {
                kind: SourceKind::Provider,
                name: "document.bin".into(),
                path_or_identity: "content://provider/document/1".into(),
                offset: 0,
                length: 1,
                mutation_token: None,
            })
            .is_err()
        );
    }
}
