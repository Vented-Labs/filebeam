import FilebeamDomain
import Foundation

/// App-private, seekable snapshots. Provider URLs are only used while their security scope is valid.
final class DocumentStore: Sendable {
    private let root: URL
    init(root: URL) { self.root = root }

    func importSources(_ urls: [URL], progress: (@Sendable (Int64) -> Void)? = nil) async -> [FileSource] {
        await Task.detached(priority: .userInitiated) { [root] in
            urls.flatMap { Self.importSource($0, root: root, progress: progress) }
        }.value
    }

    func outputDirectory() throws -> URL {
        let output = root.appendingPathComponent("outputs", isDirectory: true)
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
        return output
    }

    private static func importSource(_ url: URL, root: URL, progress: (@Sendable (Int64) -> Void)?) -> [FileSource] {
        let accessing = url.startAccessingSecurityScopedResource()
        defer { if accessing { url.stopAccessingSecurityScopedResource() } }
        var coordinatorError: NSError?
        var result: [FileSource] = []
        NSFileCoordinator().coordinate(readingItemAt: url, options: [], error: &coordinatorError) { coordinated in
            do {
                let values = try coordinated.resourceValues(forKeys: [.isDirectoryKey])
                if values.isDirectory == true {
                    let enumerator = FileManager.default.enumerator(at: coordinated, includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey, .fileSizeKey], options: [])
                    var found = false
                    while let child = enumerator?.nextObject() as? URL {
                        let childValues = try child.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
                        if childValues.isSymbolicLink == true { continue }
                        guard childValues.isDirectory != true else { continue }
                        found = true; result.append(copy(child, relativeTo: coordinated, root: root, progress: progress))
                    }
                    if !found { result.append(failed(coordinated, "The selected folder contains no readable files.")) }
                } else { result.append(copy(coordinated, relativeTo: coordinated.deletingLastPathComponent(), root: root, progress: progress)) }
            } catch { result.append(failed(coordinated, error.localizedDescription)) }
        }
        if let coordinatorError { return [failed(url, coordinatorError.localizedDescription)] }
        return result
    }

    private static func copy(_ source: URL, relativeTo base: URL, root: URL, progress: (@Sendable (Int64) -> Void)?) -> FileSource {
        guard let relative = relativePath(source, relativeTo: base) else {
            return failed(source, "The provider returned an unsafe relative path.")
        }
        let target = uniqueTarget(root.appendingPathComponent(relative, isDirectory: false))
        do {
            try FileManager.default.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
            guard FileManager.default.createFile(atPath: target.path, contents: nil), let input = InputStream(url: source), let output = OutputStream(url: target, append: false) else { throw CocoaError(.fileWriteUnknown) }
            input.open(); output.open(); defer { input.close(); output.close() }
            let buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: 64 * 1024); defer { buffer.deallocate() }
            var copied: Int64 = 0
            while true {
                let count = input.read(buffer, maxLength: 64 * 1024)
                guard count >= 0 else { throw input.streamError ?? CocoaError(.fileReadUnknown) }
                if count == 0 { break }
                var written = 0
                while written < count { let n = output.write(buffer + written, maxLength: count - written); guard n > 0 else { throw output.streamError ?? CocoaError(.fileWriteUnknown) }; written += n }
                copied += Int64(count); progress?(copied)
            }
            let size = (try? target.resourceValues(forKeys: [.fileSizeKey]).fileSize).map(UInt64.init)
            return FileSource(name: relative, location: target.path, sizeBytes: size)
        } catch {
            try? FileManager.default.removeItem(at: target)
            return failed(source, error.localizedDescription)
        }
    }

    private static func failed(_ url: URL, _ message: String) -> FileSource {
        FileSource(name: displayName(url), location: url.absoluteString, state: .importFailed("Source could not be copied. \(message) Re-select it to retry; provider access expires if the app closes."))
    }
    private static func uniqueTarget(_ requested: URL) -> URL {
        var candidate = requested; var number = 2
        while FileManager.default.fileExists(atPath: candidate.path) { candidate = requested.deletingPathExtension().appendingPathExtension("\(number).\(requested.pathExtension)"); number += 1 }
        return candidate
    }
    private static func relativePath(_ source: URL, relativeTo base: URL) -> String? {
        let sourcePath = source.standardizedFileURL.path
        let basePath = base.standardizedFileURL.path
        guard sourcePath.hasPrefix(basePath + "/") else { return nil }
        let relative = String(sourcePath.dropFirst(basePath.count + 1))
        let components = relative.split(separator: "/", omittingEmptySubsequences: false)
        guard !components.isEmpty, components.allSatisfy({ !$0.isEmpty && $0 != "." && $0 != ".." }) else { return nil }
        return relative
    }

    private static func displayName(_ url: URL) -> String {
        let name = url.lastPathComponent
        return name.isEmpty || name == "." || name == ".." ? "source" : name
    }
}
