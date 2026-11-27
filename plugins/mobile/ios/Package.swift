// swift-tools-version:5.9
// SPDX-License-Identifier: GPL-3.0-or-later
import PackageDescription

let package = Package(
    name: "tauri-plugin-ssh-client-mobile",
    platforms: [.iOS(.v16)],
    products: [
        .library(name: "tauri-plugin-ssh-client-mobile", type: .static, targets: ["SshClientMobile"])
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "SshClientMobile",
            dependencies: [.byName(name: "Tauri")],
            path: "Sources"
        )
    ]
)
