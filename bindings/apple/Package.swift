// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "LightMem",
    platforms: [
        .macOS(.v12),
        .iOS(.v15),
    ],
    products: [
        .library(
            name: "LightMem",
            targets: ["LightMem"]
        ),
    ],
    targets: [
        .target(
            name: "LightMemFFI",
            path: "Sources/LightMemFFI",
            publicHeadersPath: "include"
        ),
        .target(
            name: "LightMem",
            dependencies: ["LightMemFFI"],
            path: "Sources/LightMem",
            linkerSettings: [
                .linkedLibrary("lightmem_ffi"),
                .unsafeFlags(["-L", "Frameworks"])
            ]
        ),
        .testTarget(
            name: "LightMemTests",
            dependencies: ["LightMem"],
            path: "Tests/LightMemTests"
        ),
    ]
)
