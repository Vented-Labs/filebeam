import Foundation
import FilebeamDomain

/// Sizes in binary units with one decimal where it helps, e.g. "2.0 GiB".
func formatBytes(_ bytes: UInt64) -> String {
    if bytes < 1024 { return "\(bytes) B" }
    let units = ["KiB", "MiB", "GiB", "TiB"]
    var value = Double(bytes) / 1024
    var unit = 0
    while value >= 1024 && unit < units.count - 1 { value /= 1024; unit += 1 }
    return value >= 100 ? String(format: "%.0f %@", value, units[unit]) : String(format: "%.1f %@", value, units[unit])
}

func formatBytes(_ bytes: Int64) -> String { formatBytes(UInt64(max(0, bytes))) }

private func plural(_ count: Int, _ singular: String, _ pluralForm: String? = nil) -> String {
    "\(count) " + (count == 1 ? singular : (pluralForm ?? singular + "s"))
}

/// "1 hour", "3 days", "1 year": retention options read as durations, never as raw hour counts.
func formatRetention(hours: UInt64) -> String {
    if hours > 0 && hours % 8760 == 0 { return plural(Int(hours / 8760), "year") }
    if hours > 0 && hours % 24 == 0 { return plural(Int(hours / 24), "day") }
    return plural(Int(hours), "hour")
}

private let absoluteFormatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.dateStyle = .medium
    formatter.timeStyle = .short
    return formatter
}()

func formatAbsolute(_ date: Date) -> String { absoluteFormatter.string(from: date) }

/// "in 3 hours", "2 days ago", "moments ago".
func formatRelative(_ date: Date, now: Date = Date()) -> String {
    let seconds = Int(date.timeIntervalSince(now).rounded())
    let future = seconds > 0
    let magnitude = abs(seconds)
    let amount: String
    switch magnitude {
    case ..<60: return future ? "in under a minute" : "moments ago"
    case ..<3600: amount = plural(magnitude / 60, "minute")
    case ..<86_400: amount = plural(magnitude / 3600, "hour")
    default: amount = plural(magnitude / 86_400, "day")
    }
    return future ? "in \(amount)" : "\(amount) ago"
}

/// Parses the server's ISO-8601 timestamps; nil when absent or malformed.
func parseInstant(_ value: String?) -> Date? {
    guard let value, !value.isEmpty else { return nil }
    let fractional = ISO8601DateFormatter()
    fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return fractional.date(from: value) ?? ISO8601DateFormatter().date(from: value)
}

/// Turns a failure from the transfer core into a sentence a person can act on. Core errors arrive
/// as `detail=…` strings, sometimes ending in an HTTP status; those never reach the screen raw.
func readableMessage(_ raw: String?) -> String {
    var text = (raw ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    if text.hasPrefix("detail=") { text = String(text.dropFirst(7)).trimmingCharacters(in: .whitespaces) }
    if text.isEmpty { return "Something went wrong. Try again." }
    let status = text.firstMatch(of: /returned (\d{3})/).flatMap { Int($0.1) }
    switch status {
    case 401, 419: return "Your session ended. Sign in again."
    case 403: return "This account is not allowed to do that."
    case 404: return "That could not be found. It may have expired or been removed."
    case 422: return "The server did not accept that. Check what you entered and try again."
    case 429: return "Too many attempts. Wait a moment and try again."
    case let code? where code >= 500: return "The server had a problem. Try again shortly."
    default: break
    }
    let lowered = text.lowercased()
    if lowered.contains("error sending request") || lowered.contains("dns") || lowered.contains("connect") {
        return "The instance could not be reached. Check your connection."
    }
    return text.prefix(1).uppercased() + text.dropFirst()
}

func readableMessage(_ error: Error) -> String {
    if let domain = error as? FilebeamDomainError {
        switch domain {
        case let .invalidInput(value), let .rejected(value), let .remote(value), let .unavailable(value), let .network(value), let .storage(value), let .cryptography(value): return readableMessage(value)
        case .cancelled: return "Cancelled."
        }
    }
    return readableMessage(error.localizedDescription)
}

/// 24 characters from a 64-symbol alphabet: 144 bits, the same recipe as the web.
func generatePassword() -> String {
    let alphabet = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_")
    return String((0..<24).map { _ in alphabet[Int.random(in: 0..<alphabet.count)] })
}

extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
    var isBlank: Bool { trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
}
