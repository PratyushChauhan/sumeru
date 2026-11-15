// swift-tools-version:5.10
// Native macOS shell for Sumeru. Builds with the Command Line Tools (`swift build`); no Xcode
// project. `scripts/build-macos-app.sh` bundles it with the headless Rust engine into Sumeru.app.
import PackageDescription

let package = Package(
    name: "Sumeru",
    platforms: [.macOS(.v14)],
    targets: [
        // Engine client, JSON models, agent recipes, draft parsing: no UI, fully unit-tested.
        .target(name: "SumeruKit"),
        // SwiftUI views + app state.
        .target(name: "SumeruUI", dependencies: ["SumeruKit"]),
        .executableTarget(name: "Sumeru", dependencies: ["SumeruUI"]),
        .testTarget(name: "SumeruKitTests", dependencies: ["SumeruKit"]),
    ]
)
