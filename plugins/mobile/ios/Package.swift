// swift-tools-version:5.9
// SPDX-License-Identifier: GPL-3.0-or-later
import PackageDescription

let package = Package(
    name: "tauri-plugin-vaultport-mobile",
    platforms: [.iOS(.v16)],
    products: [
        .library(name: "tauri-plugin-vaultport-mobile", type: .static, targets: ["VaultportMobile"])
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "VaultportMobile",
            dependencies: [.byName(name: "Tauri")],
            path: "Sources"
        )
    ]
)
