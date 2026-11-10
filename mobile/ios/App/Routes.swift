import Foundation

/// The four primary destinations shown in the navigation bar. They live inside the root screen.
enum Tab: String, CaseIterable, Identifiable, Hashable {
    case send, receive, transfers, inbox
    var id: String { rawValue }
    var title: String {
        switch self { case .send: "Send"; case .receive: "Receive"; case .transfers: "Transfers"; case .inbox: "Inbox" }
    }
    var icon: Icon {
        switch self { case .send: .send; case .receive: .receive; case .transfers: .transfers; case .inbox: .inbox }
    }
}

enum AuthMode: String, Hashable {
    case login, register, forgot, reset, verify, invitation
}

/// Every screen the app can push. Routes carry only opaque identifiers; links, keys and
/// draft text never enter navigation state.
enum Route: Hashable {
    case account
    case auth(AuthMode)
    case secureInbox
    case contacts
    case devices
    case history
    case appearance
    case settings
    case storage
    case about
    case tour
    case changelog
    case release(String)
    case recipient(String)
}
