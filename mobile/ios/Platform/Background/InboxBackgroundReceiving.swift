import BackgroundTasks
import FilebeamDomain
import Foundation

@MainActor
enum InboxBackgroundReceiving {
    static let identifier = "io.filebeam.inbox.refresh"

    static func schedule() {
        let request = BGAppRefreshTaskRequest(identifier: identifier)
        request.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
        try? BGTaskScheduler.shared.submit(request)
    }

    static func run() async {
        schedule()
        do {
            let paths = try AppPaths()
            let settings = try SettingsStore().load()
            guard let instance = FilebeamInstance(origin: settings.instanceOrigin) else { return }
            let secrets = IOSKeychainStore(namespace: Bundle.main.bundleIdentifier ?? "io.filebeam.ios", stateDirectory: paths.transfers)
            let service = try NativeFilebeamService(stateDirectory: paths.transfers, secrets: secrets, relayOnly: settings.relayOnly)
            await withTaskCancellationHandler {
                _ = try? await service.receiveAutomatically(instance: instance)
            } onCancel: {
                service.cancelAutomaticReceiving()
            }
        } catch {
            // Locked or unavailable protected state is retried on a later wake or foreground catch-up.
        }
    }
}
