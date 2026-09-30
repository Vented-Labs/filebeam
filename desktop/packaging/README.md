# Desktop release packaging

The signed `desktop` catalog is independent of `cli`: `/desktop/index.json` is a schema-v1 Ed25519 envelope and immutable update payloads live under `/desktop/versions/vX.Y.Z`. Every updater payload is a platform archive containing exactly `filebeam/filebeam` (or `filebeam/filebeam.exe`), which is the layout enforced by `filebeam-client-updater`.

Consumer installers are distinct from those updater payloads: Linux AppImage integration, signed/notarized macOS DMG, and signed per-user Windows NSIS setup must be built by their native release runners. Do not describe an unsigned artifact as signed. Required signing inputs are `APPLE_SIGNING_IDENTITY`, `APPLE_NOTARY_PROFILE`, `WINDOWS_SIGNING_CERTIFICATE`, and `WINDOWS_SIGNING_PASSWORD`; catalog publication additionally requires the existing `RELEASE_SIGNING_KEY` and R2 credentials.
