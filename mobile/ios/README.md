# Filebeam iOS

`Filebeam` targets iOS 17+ on iPhone and iPad. Screens are SwiftUI with native system
integrations for Files, sharing, previews and Keychain. The four destinations in the floating
navigation bar are Send, Receive, Transfers and Inbox; the account, settings, appearance and
about screens stack above them. `Platform/Rust` adapts the shared UniFFI engine;
`Platform/Background` executes Rust-prepared ciphertext requests through background URLSession.
`ShareExtension` is a distinct target.

## Design system

`DesignSystem/` holds the theme, type scale, motion and haptic tokens and every reusable
component. Colours come from `Packages/FilebeamTheme`, a platform-independent palette engine
that reproduces the web theme generator (`backend/app/Support/Theming`): the Filebeam presets,
the connected instance's brand colour and any custom colour derive the same OKLCH roles as the
web, and `swift test --package-path Packages/FilebeamTheme` checks them against the shared
golden fixture. The same package carries the note syntax highlighter used by the editor.

Text uses the system font through Dynamic Type text styles. Icons are SF Symbols, with the two
transport glyphs kept as original Filebeam artwork in `DesignSystem/CustomIcons.generated.swift`.

## Features

- **Send**: files, folders and shared content over stored HTTP or live WebRTC, with Turbo
  Transfer, optional password, retention, a separate decryption key, ZIP packaging, an
  attached note, and delivery to a friend's encrypted inbox.
- **Notes**: a syntax-highlighting editor for nine languages, with password, retention, burn
  on read, and stored or live delivery.
- **Receive**: unlock a link with its key and password, list the files before downloading,
  download one file or all, read notes, and report a transfer.
- **Transfers**: running and saved transfers with pause, resume and removal, account history
  with extend and delete, and storage usage.
- **Account**: sign in, registration, recovery, email verification and invitations through
  universal links; secure inbox setup with either key custody mode; friends and receiving
  preferences; registered devices; account deletion.
- **Appearance**: system, light and dark themes with the Filebeam presets, the instance colour
  or a custom colour. Changes spread from the touch point in a circular reveal.
- **About**: a features tour, the instance's changelog rendered natively, CLI install commands
  and open-source licences.

Password-custody inbox keys are unlocked once at sign-in and kept in the device Keychain, so
deliveries open without re-entering the account password. Screens that show keys, passwords or
decrypted notes are marked privacy sensitive.

## Build

Use macOS with Xcode 26.3. Select it with `xcode-select`, or set `IOS_XCODE_VERSION` and `IOS_DEVELOPER_DIR` together. `bootstrap.sh` installs Rust 1.98.0 and XcodeGen 2.41.0.

Run from the repository root:

```sh
bash scripts/ios/bootstrap.sh
bash scripts/ios/build-rust.sh release
bash scripts/ios/generate-project.sh
bash scripts/ios/build.sh Debug simulator
bash scripts/ios/build.sh Release device
bash scripts/ios/test.sh
```

The Rust build packages device and simulator libraries into an XCFramework and generates Swift bindings from the same source. Generated files are ignored; rebuild after changing Rust code. Set `IOS_INCLUDE_X86_64_SIMULATOR=0` to omit Intel simulator support. Release builds retain Rust debug information for dSYMs.

Set `IOS_TEST_OS=17` to select the minimum supported simulator runtime, or use `IOS_TEST_DESTINATION` for an explicit simulator/device. CI runners need both the iOS 17 and current-iOS runtimes installed.

## Signing and distribution

PR builds are unsigned. For archive/export, keep signing data external:

```sh
IOS_SIGNING_XCCONFIG=/secure/FilebeamSigning.xcconfig bash scripts/ios/archive.sh
IOS_EXPORT_OPTIONS_PLIST=/secure/ExportOptions.plist bash scripts/ios/export.sh
```

The protected distribution workflow accepts only a validated release tag after merge to `master`, imports those materials from secrets, and uploads the IPA to App Store Connect for TestFlight or App Store processing.

## Acceptance

`bash scripts/ios/acceptance.sh` starts a disposable Laravel container and runs normal, password-protected, ZIP, and Turbo HTTP transfers with byte/hash verification. It requires Docker, backend Composer dependencies, and the `filebeam-sail-php85/app:latest` image. The container and SQLite state are removed on exit; results are written to `test-results/ios/acceptance/`.

On macOS, set `IOS_ACCEPTANCE_HTTPS_LINK_FIXTURE` to a complete HTTPS link for a non-sensitive test transfer to include the XCUITest receive flow. Without it, the script runs only the native HTTP suite. The app requires an HTTPS backend exposing the `/api/native/v1` policy and account APIs.

## Universal links

The app uses `applinks:filebeam.io`. Configure the backend's `FILEBEAM_IOS_APP_IDS` with the signed Apple app identifiers, either comma-separated or as a JSON array. Each entry has the form `TEAM.bundle`; debug and release identifiers are separate entries.

The backend serves the association at `/.well-known/apple-app-site-association` and returns `404` when no identifiers are configured. Transfer, receiving-profile, invitation, verification, and password-reset links are supported.

## App icon

Regenerate the 1024px icon from the approved SVG after installing the root npm dependencies:

```sh
node scripts/ios/generate-app-icon.mjs \
  mobile/ios/Resources/Assets.xcassets/AppIcon.appiconset/AppIcon.svg \
  mobile/ios/Resources/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png
```
