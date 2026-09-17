// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "FilebeamDomain",
    platforms: [.iOS(.v17)],
    products: [.library(name: "FilebeamDomain", targets: ["FilebeamDomain"])],
    targets: [
        .target(name: "FilebeamDomain"),
        .testTarget(name: "FilebeamDomainTests", dependencies: ["FilebeamDomain"])
    ]
)
