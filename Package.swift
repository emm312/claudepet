// swift-tools-version:6.2
import PackageDescription

let package = Package(
    name: "ClaudePet",
    platforms: [
        .macOS(.v13)
    ],
    targets: [
        .executableTarget(
            name: "ClaudePet",
            path: "Sources/ClaudePet",
            swiftSettings: [.defaultIsolation(MainActor.self)]
        ),
        .testTarget(
            name: "ClaudePetTests",
            dependencies: ["ClaudePet"],
            path: "Tests/ClaudePetTests",
            swiftSettings: [
                .defaultIsolation(MainActor.self),
                // Command Line Tools (no full Xcode) ships swift-testing as a
                // standalone framework outside the default search path.
                .unsafeFlags([
                    "-F", "/Library/Developer/CommandLineTools/Library/Developer/Frameworks"
                ])
            ],
            linkerSettings: [
                .unsafeFlags([
                    "-F", "/Library/Developer/CommandLineTools/Library/Developer/Frameworks",
                    "-Xlinker", "-rpath",
                    "-Xlinker", "/Library/Developer/CommandLineTools/Library/Developer/Frameworks",
                    "-Xlinker", "-rpath",
                    "-Xlinker", "/Library/Developer/CommandLineTools/Library/Developer/usr/lib"
                ])
            ]
        )
    ]
)
