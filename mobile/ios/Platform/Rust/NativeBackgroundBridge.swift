import Foundation

public enum NativeBackgroundDirection: String, Codable, Sendable {
    case upload
    case download
    case inboxDownload
}

public enum NativeBackgroundEvent: Sendable, Equatable {
    case progress(checkpointID: String, done: UInt64, total: UInt64?)
    case readyToFinalize(checkpointID: String, direction: NativeBackgroundDirection)
    case failed(checkpointID: String, message: String)
    case paused(checkpointID: String, direction: NativeBackgroundDirection)
}

/// The native service owns transfer state; an executor owns only OS HTTP tasks.
public protocol NativeBackgroundExecutor: Sendable {
    func enqueue(checkpointID: String, direction: NativeBackgroundDirection) async throws
    func pause(checkpointID: String) async throws
    func resume(checkpointID: String, direction: NativeBackgroundDirection) async throws
    func reconcile() async
}

/// Optional capability for an app-owned inbox credential bridge. The executor never persists it.
public protocol NativeInboxBackgroundExecutor: NativeBackgroundExecutor {
    func supplyInboxCookieContext(_ cookie: String, checkpointID: String) throws
}
