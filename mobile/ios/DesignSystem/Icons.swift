import SwiftUI

/// The app's icon vocabulary, mapped to SF Symbols so every screen names the same idea the same way.
enum Icon: String {
    case send = "arrow.up.circle"
    case receive = "arrow.down.circle"
    case transfers = "arrow.left.arrow.right"
    case inbox = "tray"
    case folder = "folder"
    case note = "note.text"
    case key = "key"
    case clock = "clock"
    case users = "person.2"
    case user = "person"
    case link = "link"
    case archive = "archivebox"
    case lock = "lock.fill"
    case unlock = "lock.open"
    case bolt = "bolt.fill"
    case shield = "checkmark.shield"
    case plus = "plus"
    case close = "xmark"
    case chevronRight = "chevron.right"
    case chevronLeft = "chevron.left"
    case chevronDown = "chevron.down"
    case chevronUp = "chevron.up"
    case arrowUp = "arrow.up"
    case arrowUpRight = "arrow.up.right"
    case arrowRight = "arrow.right"
    case arrowLeft = "arrow.left"
    case copy = "doc.on.doc"
    case terminal = "terminal"
    case code = "chevron.left.forwardslash.chevron.right"
    case trash = "trash"
    case alert = "exclamationmark.triangle"
    case check = "checkmark"
    case info = "info.circle"
    case settings = "gearshape"
    case brush = "paintbrush"
    case storage = "internaldrive"
    case search = "magnifyingglass"
    case slash = "nosign"
    case bell = "bell"
    case notification = "app.badge"
    case mobile = "iphone"
    case laptop = "laptopcomputer"
    case monitor = "display"
    case sun = "sun.max"
    case moon = "moon"
    case globe = "globe"
    case book = "book"
    case heart = "heart"
    case help = "questionmark.circle"
    case updates = "sparkles"
    case image = "photo"
    case video = "film"
    case music = "music.note"
    case file = "doc"
    case download = "arrow.down.to.line"
    case upload = "arrow.up.to.line"
    case login = "arrow.right.square"
    case logout = "rectangle.portrait.and.arrow.right"
    case refresh = "arrow.clockwise"
    case restore = "arrow.counterclockwise"
    case color = "paintpalette"
    case eye = "eye"
    case eyeOff = "eye.slash"
    case paste = "doc.on.clipboard"
    case share = "square.and.arrow.up"
    case fingerprint = "touchid"
    case wave = "dot.radiowaves.left.and.right"
    case hourglass = "hourglass"
}

/// A symbol from the approved set, tinted with a semantic colour.
struct FbIcon: View {
    @Environment(\.theme) private var theme
    let icon: Icon
    var size: CGFloat = 20
    var tint: Color?
    var weight: Font.Weight = .medium

    init(_ icon: Icon, size: CGFloat = 20, tint: Color? = nil, weight: Font.Weight = .medium) {
        self.icon = icon; self.size = size; self.tint = tint; self.weight = weight
    }

    var body: some View {
        Image(systemName: icon.rawValue)
            .font(.system(size: size * 0.82, weight: weight))
            .foregroundStyle(tint ?? theme.textMuted)
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

/// The icon family the web queue uses, chosen from the file extension.
func fileIcon(for name: String) -> Icon {
    let ext = (name as NSString).pathExtension.lowercased()
    let images: Set<String> = ["png", "jpg", "jpeg", "gif", "webp", "heic", "heif", "svg", "bmp", "tiff", "tif", "avif"]
    let videos: Set<String> = ["mp4", "mov", "m4v", "mkv", "webm", "avi", "hevc"]
    let audio: Set<String> = ["mp3", "m4a", "aac", "wav", "flac", "ogg", "aiff", "opus"]
    let archives: Set<String> = ["zip", "tar", "gz", "tgz", "7z", "rar", "bz2", "xz"]
    let code: Set<String> = ["js", "ts", "tsx", "jsx", "json", "php", "py", "rs", "kt", "java", "c", "cpp", "h", "go", "rb", "sh", "css", "html", "xml", "yml", "yaml", "toml", "md", "sql", "env", "swift"]
    if images.contains(ext) { return .image }
    if videos.contains(ext) { return .video }
    if audio.contains(ext) { return .music }
    if archives.contains(ext) { return .archive }
    if code.contains(ext) { return .code }
    return .file
}
