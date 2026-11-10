// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "FilebeamTheme",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [.library(name: "FilebeamTheme", targets: ["FilebeamTheme"])],
    targets: [
        .target(name: "FilebeamTheme"),
        .testTarget(name: "FilebeamThemeTests", dependencies: ["FilebeamTheme"], resources: [.copy("Fixtures")]),
    ]
)
