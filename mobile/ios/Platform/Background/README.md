# Background HTTP

`BackgroundHTTPDriver` runs only descriptors returned by `FilebeamCore.BackgroundTransfer`. Use one stable identifier, `BackgroundHTTPDriver.stableSessionIdentifier`, rather than creating a session per origin.

App startup integration:

```swift
let background = try await service.backgroundEngine()
let driver = BackgroundHTTPDriver(
    engine: background,
    stateDirectory: paths.transfers,
    sessionIdentifier: BackgroundHTTPDriver.stableSessionIdentifier,
    onEvent: { event in bridge.handleBackgroundHTTPEvent(event) }
)
BackgroundHTTPDriver.attach(driver)
await driver.reconcile()
```

For an inbox relaunch, load the account cookie from `IOSKeychainStore` only after unlock, call `try driver.supplyInboxCookieContext(cookie, checkpointID: id)`, then `try await driver.resume(checkpointID: id, direction: .inboxDownload)`. The cookie is never written by this component. A locked state directory or unavailable cookie is retained and reported as recoverable; no checkpoint or journal is deleted.

Attach the delegate to SwiftUI without changing session identifiers:

```swift
@main struct FilebeamApp: App {
    @UIApplicationDelegateAdaptor(FilebeamAppDelegate.self) var appDelegate
    // ...
}
```

The bridge handles `.readyToFinalize` by calling the appropriate Rust `finalizeUpload`, `finalizeDownload`, or `finalizeInboxDownload`; verification and password prompts are foreground work, not network progress.

The driver emits `.readyToFinalize` only after Rust has durably acknowledged every staged operation and a fresh `pending*Work` query is empty. Paused or failed journal entries therefore cannot finalize a partial transfer. It can safely be constructed while protected storage is unavailable; call `await driver.reconcile()` again after unlock to reopen the journal, bind interrupted suspended tasks from their versioned opaque task descriptions, replay staged completions, and resume only non-paused OS work.

## Download Item Selection

After `prepareDownload` completes, present its checkpoint's authenticated local manifest before requesting any work:

```swift
let items: [DownloadItem] = try background.downloadItems(checkpointId: checkpointID)
try background.selectDownloadItems(checkpointId: checkpointID, itemIds: [items[0].id])
let work = try background.pendingDownloadWork(transferId: checkpointID)
```

`DownloadItem` has exactly `id: String`, `name: String`, and `size: UInt64`; it exposes no key, digest, or ciphertext location. `downloadItems` is checkpoint I/O only and never makes a network request or claims a peer. Selection must be non-empty, contain unique authenticated manifest IDs, and is durable across relaunch. The first `pendingDownloadWork` call freezes selection (legacy callers that skip selection get all items). Only selected ciphertext is described, accepted, authenticated, verified, and published; finalization returns selected paths only.

Portable journal checks run without iOS SDKs:

```sh
docker run --rm -v "$PWD/mobile/ios/Platform/Background:/src" -w /src swift:6.2-noble swift test
```

## Redirect limitation

iOS background URL sessions can follow HTTP redirects before app code can reliably reject each hop. This driver validates the original Rust descriptor but cannot safely strip signed headers on a redirected request. Rust/native server routes used here must therefore be direct HTTPS signed/ciphertext routes with no redirects (including canonical-host redirects). Do not use this executor for arbitrary third-party URLs or routes that redirect across origins.
