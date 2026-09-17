import SwiftUI
import UIKit
import UniformTypeIdentifiers

final class ShareExtensionViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        let host = UIHostingController(rootView: ShareExtensionView(stage: { [weak self] in self?.stageAndComplete() }, cancel: { [weak self] in self?.extensionContext?.cancelRequest(withError: CocoaError(.userCancelled)) }))
        addChild(host); host.view.frame = view.bounds; host.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]; view.addSubview(host.view); host.didMove(toParent: self)
    }
    private func stageAndComplete() {
        Task { @MainActor in
            do {
                guard let groupID = Bundle.main.object(forInfoDictionaryKey: "AppGroupIdentifier") as? String, let group = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: groupID) else { throw CocoaError(.fileNoSuchFile) }
                let providers = extensionContext?.inputItems.compactMap { $0 as? NSExtensionItem }.flatMap { $0.attachments ?? [] } ?? []
                let providerStaging = FileManager.default.temporaryDirectory.appendingPathComponent("filebeam-share-\(UUID().uuidString)", isDirectory: true)
                try FileManager.default.createDirectory(at: providerStaging, withIntermediateDirectories: true)
                defer { try? FileManager.default.removeItem(at: providerStaging) }
                var files: [URL] = []; var text: String?; var links: [String] = []; var unsupported = false
                for provider in providers {
                    do {
                        if let typeIdentifier = provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) ? (provider.registeredTypeIdentifiers.first(where: { $0 != UTType.fileURL.identifier && $0 != UTType.url.identifier }) ?? UTType.fileURL.identifier) : nil {
                            files.append(try await copyFileRepresentation(from: provider, typeIdentifier: typeIdentifier, into: providerStaging, fallbackName: "item-\(files.count + 1)"))
                        }
                        else if provider.hasItemConformingToTypeIdentifier(UTType.url.identifier), let url = try await loadItem(from: provider, typeIdentifier: UTType.url.identifier, as: URL.self) { links.append(url.absoluteString) }
                        else if provider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier), let value = try await loadItem(from: provider, typeIdentifier: UTType.plainText.identifier, as: String.self) { text = String(value.prefix(1_000_000)) }
                        else { unsupported = true }
                    } catch { unsupported = true }
                }
                _ = try SharedDraftInbox(groupURL: group).stage(files: files, text: text, links: links, failureHint: unsupported ? "One or more shared items are unsupported" : nil)
                extensionContext?.completeRequest(returningItems: nil)
            } catch { extensionContext?.cancelRequest(withError: error) }
        }
    }

    private func loadItem<Value: Sendable>(from provider: NSItemProvider, typeIdentifier: String, as type: Value.Type) async throws -> Value? {
        try await withCheckedThrowingContinuation { continuation in
            provider.loadItem(forTypeIdentifier: typeIdentifier, options: nil) { item, error in
                if let error { continuation.resume(throwing: error) }
                else { continuation.resume(returning: item as? Value) }
            }
        }
    }

    /// NSItemProvider invalidates this URL as soon as its completion returns. Copy it
    /// before resuming the awaiting task so the encrypted inbox only sees owned files.
    private func copyFileRepresentation(from provider: NSItemProvider, typeIdentifier: String, into directory: URL, fallbackName: String) async throws -> URL {
        let suggestedName = provider.suggestedName
        return try await withCheckedThrowingContinuation { continuation in
            provider.loadFileRepresentation(forTypeIdentifier: typeIdentifier) { url, error in
                guard let url else { continuation.resume(throwing: error ?? CocoaError(.fileReadUnknown)); return }
                let name = suggestedName?.isEmpty == false ? suggestedName! : (url.lastPathComponent.isEmpty ? fallbackName : url.lastPathComponent)
                let destination = directory.appendingPathComponent(name.replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "\\", with: "_"))
                do {
                    guard FileManager.default.createFile(atPath: destination.path, contents: nil), let input = InputStream(url: url), let output = OutputStream(url: destination, append: false) else { throw CocoaError(.fileWriteUnknown) }
                    input.open(); output.open(); defer { input.close(); output.close() }
                    var buffer = [UInt8](repeating: 0, count: 64 * 1024)
                    while true {
                        let count = input.read(&buffer, maxLength: buffer.count)
                        guard count >= 0 else { throw input.streamError ?? CocoaError(.fileReadUnknown) }
                        if count == 0 { break }
                        var offset = 0
                        while offset < count {
                            let written = buffer.withUnsafeBufferPointer { output.write($0.baseAddress!.advanced(by: offset), maxLength: count - offset) }
                            guard written > 0 else { throw output.streamError ?? CocoaError(.fileWriteUnknown) }
                            offset += written
                        }
                    }
                    continuation.resume(returning: destination)
                } catch { try? FileManager.default.removeItem(at: destination); continuation.resume(throwing: error) }
            }
        }
    }
}
private struct ShareExtensionView: View {
    let stage: () -> Void; let cancel: () -> Void
    var body: some View { VStack(spacing: 16) { Text("Add to Filebeam").font(.headline); Text("Items are staged locally and sent only after you choose Send in Filebeam.").multilineTextAlignment(.center); HStack { Button("Cancel", action: cancel); Button("Add", action: stage).buttonStyle(.borderedProminent) } }.padding() }
}
