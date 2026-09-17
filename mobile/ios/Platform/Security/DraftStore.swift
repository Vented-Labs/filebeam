import CryptoKit
import FilebeamDomain
import Foundation

final class DraftStore: Sendable {
    private let records: SealedRecordStore
    init(directory: URL, keychain: IOSKeychainStore) { records = SealedRecordStore(directory: directory, name: "composer-drafts-v1.bin", context: "composer-drafts-v1", keychain: keychain) }

    func load() async throws -> ComposerDrafts? {
        guard let data = try records.load() else { return nil }
        return try JSONDecoder().decode(ComposerDrafts.self, from: data)
    }
    func save(_ drafts: ComposerDrafts) async throws {
        try records.save(JSONEncoder().encode(drafts)) // ComposerDrafts intentionally has no password field.
    }
}
