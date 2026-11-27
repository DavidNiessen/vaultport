// SPDX-License-Identifier: GPL-3.0-or-later
import Foundation
import LocalAuthentication
import Security
import Tauri
import UIKit

class KeyArgs: Decodable {
    let key: String
    let value: String?
}

class KeepAliveArgs: Decodable {
    let sessions: Int
}

class SecureArgs: Decodable {
    let secure: Bool
}

class LabelArgs: Decodable {
    let label: String
}

class SignArgs: Decodable {
    let alias: String
    let data: String
    let reason: String
}

/// Native services for the ssh-client Rust core (not exposed to the web UI).
class MobilePlugin: Plugin {
    private let service = "dev.niessen.sshclient.secure-store"
    private var backgroundTask: UIBackgroundTaskIdentifier = .invalid
    private var privacyView: UIView?

    // MARK: Secure store (Keychain, this device only, available when unlocked)

    @objc public func secureSet(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(KeyArgs.self)
        guard let value = args.value?.data(using: .utf8) else {
            invoke.reject("value missing")
            return
        }
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: args.key,
        ]
        SecItemDelete(query as CFDictionary)
        var add = query
        add[kSecValueData as String] = value
        add[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        if status == errSecSuccess {
            invoke.resolve()
        } else {
            invoke.reject("keychain error \(status)")
        }
    }

    @objc public func secureGet(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(KeyArgs.self)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: args.key,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound {
            invoke.resolve(["value": NSNull()])
        } else if status == errSecSuccess, let data = item as? Data,
            let value = String(data: data, encoding: .utf8)
        {
            invoke.resolve(["value": value])
        } else {
            invoke.reject("keychain error \(status)")
        }
    }

    @objc public func secureDelete(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(KeyArgs.self)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: args.key,
        ]
        SecItemDelete(query as CFDictionary)
        invoke.resolve()
    }

    // MARK: Keep-alive (iOS grants a short background grace period only)

    @objc public func keepAlive(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(KeepAliveArgs.self)
        DispatchQueue.main.async {
            if args.sessions > 0 && self.backgroundTask == .invalid {
                self.backgroundTask = UIApplication.shared.beginBackgroundTask(
                    withName: "ssh-client-sessions"
                ) { [weak self] in
                    guard let self else { return }
                    UIApplication.shared.endBackgroundTask(self.backgroundTask)
                    self.backgroundTask = .invalid
                }
            } else if args.sessions == 0 && self.backgroundTask != .invalid {
                UIApplication.shared.endBackgroundTask(self.backgroundTask)
                self.backgroundTask = .invalid
            }
            invoke.resolve()
        }
    }

    // MARK: Screen privacy (cover the window when the app resigns active)

    @objc public func setSecureScreen(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(SecureArgs.self)
        DispatchQueue.main.async {
            let center = NotificationCenter.default
            center.removeObserver(self, name: UIApplication.willResignActiveNotification, object: nil)
            center.removeObserver(self, name: UIApplication.didBecomeActiveNotification, object: nil)
            if args.secure {
                center.addObserver(
                    self, selector: #selector(self.cover),
                    name: UIApplication.willResignActiveNotification, object: nil)
                center.addObserver(
                    self, selector: #selector(self.uncover),
                    name: UIApplication.didBecomeActiveNotification, object: nil)
            }
            invoke.resolve()
        }
    }

    @objc private func cover() {
        guard privacyView == nil,
            let window = UIApplication.shared.connectedScenes
                .compactMap({ ($0 as? UIWindowScene)?.keyWindow }).first
        else { return }
        let blur = UIVisualEffectView(effect: UIBlurEffect(style: .systemThickMaterial))
        blur.frame = window.bounds
        blur.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        window.addSubview(blur)
        privacyView = blur
    }

    @objc private func uncover() {
        privacyView?.removeFromSuperview()
        privacyView = nil
    }

    // MARK: Hardware keys (handled by the Secure Enclave backend in Rust on iOS)

    @objc public func hardwareKeyCreate(_ invoke: Invoke) throws {
        invoke.reject("Secure Enclave keys are created by the core on iOS")
    }

    @objc public func hardwareKeySign(_ invoke: Invoke) throws {
        invoke.reject("Secure Enclave keys are used by the core on iOS")
    }

    @objc public func hardwareKeyDelete(_ invoke: Invoke) throws {
        invoke.reject("Secure Enclave keys are managed by the core on iOS")
    }
}

@_cdecl("init_plugin_ssh_client_mobile")
func initPlugin() -> Plugin {
    return MobilePlugin()
}
