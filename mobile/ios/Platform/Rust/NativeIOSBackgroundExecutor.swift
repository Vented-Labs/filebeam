import Foundation

#if canImport(UIKit) && canImport(FilebeamCore)
import UIKit
import FilebeamCore

/// Thin iOS-only translation layer. No transfer policy or credentials live here.
public final class NativeIOSBackgroundExecutor: @unchecked Sendable, NativeInboxBackgroundExecutor {
    private let driver: BackgroundHTTPDriver

    public init(engine: BackgroundTransfer, stateDirectory: URL, onEvent: @escaping @Sendable (NativeBackgroundEvent) -> Void) {
        driver = BackgroundHTTPDriver(engine: engine, stateDirectory: stateDirectory) { event in
            switch event {
            case let .progress(checkpointID, done, total): onEvent(.progress(checkpointID: checkpointID, done: done, total: total))
            case let .readyToFinalize(checkpointID, direction): onEvent(.readyToFinalize(checkpointID: checkpointID, direction: Self.direction(direction)))
            case let .failed(checkpointID, message): onEvent(.failed(checkpointID: checkpointID, message: message))
            case let .paused(checkpointID, direction): onEvent(.paused(checkpointID: checkpointID, direction: Self.direction(direction)))
            }
        }
        BackgroundHTTPDriver.attach(driver)
    }

    public func enqueue(checkpointID: String, direction: NativeBackgroundDirection) async throws { try await driver.enqueue(checkpointID: checkpointID, direction: Self.direction(direction)) }
    public func pause(checkpointID: String) async throws { try await driver.pause(checkpointID: checkpointID) }
    public func resume(checkpointID: String, direction: NativeBackgroundDirection) async throws { try await driver.resume(checkpointID: checkpointID, direction: Self.direction(direction)) }
    public func reconcile() async { await driver.reconcile() }
    public func supplyInboxCookieContext(_ cookie: String, checkpointID: String) throws { try driver.supplyInboxCookieContext(cookie, checkpointID: checkpointID) }

    private static func direction(_ value: NativeBackgroundDirection) -> BackgroundDirection {
        switch value { case .upload: return .upload; case .download: return .download; case .inboxDownload: return .inboxDownload }
    }
    private static func direction(_ value: BackgroundDirection) -> NativeBackgroundDirection {
        switch value { case .upload: return .upload; case .download: return .download; case .inboxDownload: return .inboxDownload }
    }
}
#endif
