// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "EspressoMacSDK",
    platforms: [.macOS(.v12)],
    products: [
        .library(name: "EspressoMacSDK", targets: ["EspressoMacSDK"])
    ],
    targets: [
        .target(
            name: "EspressoMacSDK",
            swiftSettings: [
                .enableExperimentalFeature("StrictConcurrency")
            ]
        ),
        .testTarget(
            name: "EspressoMacSDKTests",
            dependencies: ["EspressoMacSDK"]
        )
    ]
)
