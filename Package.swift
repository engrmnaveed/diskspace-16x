// swift-tools-version: 6.0
import PackageDescription

var products: [Product] = [
    .library(name: "SpaceCore", targets: ["SpaceCore"]),
]

var targets: [Target] = [
    .target(name: "SpaceCore"),
    .testTarget(name: "SpaceCoreTests", dependencies: ["SpaceCore"]),
]

#if os(macOS)
products.append(.executable(name: "Space16XApp", targets: ["SpaceApp"]))
targets.append(.executableTarget(name: "SpaceApp", dependencies: ["SpaceCore"]))
#endif

let package = Package(
    name: "Space16X",
    platforms: [.macOS(.v14)],
    products: products,
    targets: targets
)
