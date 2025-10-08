// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "ParakeetSidecar",
    platforms: [
        .macOS(.v13)  // FluidAudio requires macOS 13.0+
    ],
    dependencies: [
        .package(url: "https://github.com/FluidInference/FluidAudio.git", from: "0.6.1")
    ],
    targets: [
        .executableTarget(
            name: "ParakeetSidecar",
            dependencies: [
                .product(name: "FluidAudio", package: "FluidAudio")
            ]
        ),
    ]
)
