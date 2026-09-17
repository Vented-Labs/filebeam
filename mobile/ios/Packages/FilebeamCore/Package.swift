// swift-tools-version: 6.0
import PackageDescription
let package = Package(name: "FilebeamCore", platforms: [.iOS(.v17)], products: [.library(name: "FilebeamCore", targets: ["FilebeamCore"])], targets: [.binaryTarget(name: "FilebeamCoreFFI", path: "Artifacts/FilebeamCoreFFI.xcframework"), .target(name: "FilebeamCore", dependencies: ["FilebeamCoreFFI"], path: "Generated")])
