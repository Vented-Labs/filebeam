# Desktop release packaging

The signed `desktop` catalog is independent of `cli`: `/desktop/index.json` is a
schema-v1 Ed25519 envelope. Immutable update payloads live under
`/desktop/versions/vX.Y.Z`. Linux archives contain the AppImage at
`filebeam/filebeam`, Windows archives contain `filebeam/filebeam.exe`, and macOS
archives contain `Filebeam.app`.

Native runners also produce AppImage, DMG, and per-user NSIS installers. Windows
installers have no Authenticode signature. macOS bundles use an ad-hoc signature,
without Developer ID signing or notarization. No platform-signing credentials are
required; catalog publication uses the existing Ed25519 release key and R2
credentials.
