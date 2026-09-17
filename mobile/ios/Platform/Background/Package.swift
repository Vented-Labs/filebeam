// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "FilebeamBackgroundPortable",
    products: [.library(name: "FilebeamBackgroundPortable", targets: ["FilebeamBackgroundPortable"])],
    targets: [
        .target(name: "FilebeamBackgroundPortable", path: ".", exclude: ["README.md", "PortableTests"], sources: ["BackgroundTypes.swift", "BackgroundJournal.swift", "BackgroundHTTPDriver.swift"]),
        .testTarget(name: "FilebeamBackgroundPortableTests", dependencies: ["FilebeamBackgroundPortable"], path: "PortableTests")
    ]
)
