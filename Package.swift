// swift-tools-version: 6.0
import PackageDescription

var products: [Product] = [
    .library(name: "SpaceCore", targets: ["SpaceCore"]),
    .library(name: "SpacePro", targets: ["SpacePro"]),
]

var targets: [Target] = [
    .target(name: "SpaceCore"),
    // Pro analysis, shown in the app when Settings → Pro features is on.
    .target(name: "SpacePro", dependencies: ["SpaceCore"]),
    .testTarget(name: "SpaceCoreTests", dependencies: ["SpaceCore"]),
    .testTarget(name: "SpaceProTests", dependencies: ["SpacePro", "SpaceCore"]),
]

#if os(macOS)
products.append(.executable(name: "Space16XApp", targets: ["SpaceApp"]))
targets.append(.executableTarget(name: "SpaceApp", dependencies: ["SpaceCore", "SpacePro"]))
#endif

let package = Package(
    name: "Space16X",
    platforms: [.macOS(.v14)],
    products: products,
    targets: targets
)
