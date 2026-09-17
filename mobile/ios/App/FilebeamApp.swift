import SwiftUI
import FilebeamDomain

private final class SealedNativeRecordStorage: NativeRecordStorage, @unchecked Sendable {
    private let directory: URL
    private let keychain: IOSKeychainStore

    init(directory: URL, keychain: IOSKeychainStore) {
        self.directory = directory
        self.keychain = keychain
    }

    func load(name: String) throws -> Data? {
        try SealedRecordStore(directory: directory, name: name, context: "native.\(name)", keychain: keychain).load()
    }

    func save(name: String, data: Data) throws {
        try SealedRecordStore(directory: directory, name: name, context: "native.\(name)", keychain: keychain).save(data)
    }
}

@main
struct FilebeamApp: App {
    @UIApplicationDelegateAdaptor(FilebeamAppDelegate.self) private var appDelegate
    var body: some Scene { WindowGroup { FilebeamAppContainer() } }
}

private struct FilebeamAppContainer: View {
    @State private var model: AppModel?
    @State private var importSources: (([URL]) async -> [FileSource])?
    @State private var startupError: String?

    var body: some View {
        Group {
            if let model, let importSources { FilebeamRootView(model: model, importSources: importSources) }
            else if let startupError { ContentUnavailableView("Filebeam could not start", systemImage: "exclamationmark.triangle", description: Text(startupError)) }
            else { ProgressView("Starting Filebeam") }
        }
        .task {
            guard model == nil, startupError == nil else { return }
            do {
                let paths = try AppPaths()
                let settingsStore = SettingsStore()
                let settings = try settingsStore.load()
                guard let instance = FilebeamInstance(origin: settings.instanceOrigin) else { throw FilebeamDomainError.invalidInput("The saved Filebeam instance is invalid.") }
                let secrets = IOSKeychainStore(namespace: Bundle.main.bundleIdentifier ?? "io.filebeam.ios", stateDirectory: paths.transfers)
                let records = SealedNativeRecordStorage(directory: paths.transfers, keychain: secrets)
                let service = try NativeFilebeamService(stateDirectory: paths.transfers, secrets: secrets, relayOnly: settings.relayOnly, recordStorage: records)
                let executor = NativeIOSBackgroundExecutor(engine: try await service.backgroundEngine(), stateDirectory: paths.transfers) { event in
                    service.handleBackgroundEvent(event)
                }
                service.configureBackgroundExecutor(executor)
                let documents = DocumentStore(root: paths.sources)
                let draftStore = DraftStore(directory: paths.drafts, keychain: secrets)
                model = AppModel(service: service, instance: instance, outputDirectory: documents.outputDirectory, loadDrafts: draftStore.load, saveDrafts: draftStore.save, settings: settings, saveSettings: { try settingsStore.save($0) })
                importSources = { urls in await documents.importSources(urls) }
            } catch { startupError = error.localizedDescription }
        }
    }
}
