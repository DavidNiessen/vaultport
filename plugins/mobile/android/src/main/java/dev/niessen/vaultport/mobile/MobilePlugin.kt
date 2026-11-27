// SPDX-License-Identifier: GPL-3.0-or-later
package dev.niessen.sshclient.mobile

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import android.view.WindowManager
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.Signature
import java.security.spec.ECGenParameterSpec
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

@InvokeArg
class KeyArgs {
    lateinit var key: String
    var value: String? = null
}

@InvokeArg
class KeepAliveArgs {
    var sessions: Int = 0
}

@InvokeArg
class SecureArgs {
    var secure: Boolean = true
}

@InvokeArg
class LabelArgs {
    lateinit var label: String
}

@InvokeArg
class SignArgs {
    lateinit var alias: String
    lateinit var data: String
    var reason: String = "Use your SSH key"
}

/**
 * Native services used by the ssh-client Rust core. Nothing here is callable from the web UI:
 * the plugin registers no Tauri commands, only mobile plugin entry points used from Rust.
 */
@TauriPlugin
class MobilePlugin(private val activity: Activity) : Plugin(activity) {
    private val keystore: KeyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
    private val prefs = activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    // ----- secure store (AES-256-GCM key in the Android Keystore) -----------------------------

    private fun wrappingKey(): SecretKey {
        (keystore.getEntry(WRAP_ALIAS, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
        val spec = KeyGenParameterSpec.Builder(
            WRAP_ALIAS,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setRandomizedEncryptionRequired(true)
            .apply { if (Build.VERSION.SDK_INT >= 28) setIsStrongBoxBacked(hasStrongBox()) }
            .build()
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        gen.init(spec)
        return gen.generateKey()
    }

    private fun hasStrongBox(): Boolean =
        Build.VERSION.SDK_INT >= 28 &&
            activity.packageManager.hasSystemFeature("android.hardware.strongbox_keystore")

    @Command
    fun secureSet(invoke: Invoke) {
        val args = invoke.parseArgs(KeyArgs::class.java)
        val value = args.value ?: return invoke.reject("value missing")
        try {
            val cipher = Cipher.getInstance(AES_GCM)
            cipher.init(Cipher.ENCRYPT_MODE, wrappingKey())
            cipher.updateAAD(args.key.toByteArray())
            val ct = cipher.doFinal(value.toByteArray(Charsets.UTF_8))
            val blob = cipher.iv + ct
            prefs.edit().putString(args.key, Base64.encodeToString(blob, Base64.NO_WRAP)).apply()
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("secure store failed: ${e.message}")
        }
    }

    @Command
    fun secureGet(invoke: Invoke) {
        val args = invoke.parseArgs(KeyArgs::class.java)
        val stored = prefs.getString(args.key, null)
        val ret = JSObject()
        if (stored == null) {
            ret.put("value", null)
            return invoke.resolve(ret)
        }
        try {
            val blob = Base64.decode(stored, Base64.NO_WRAP)
            val cipher = Cipher.getInstance(AES_GCM)
            cipher.init(Cipher.DECRYPT_MODE, wrappingKey(), GCMParameterSpec(128, blob, 0, 12))
            cipher.updateAAD(args.key.toByteArray())
            val plain = cipher.doFinal(blob, 12, blob.size - 12)
            ret.put("value", String(plain, Charsets.UTF_8))
            plain.fill(0)
            invoke.resolve(ret)
        } catch (e: Exception) {
            invoke.reject("secure store failed: ${e.message}")
        }
    }

    @Command
    fun secureDelete(invoke: Invoke) {
        val args = invoke.parseArgs(KeyArgs::class.java)
        prefs.edit().remove(args.key).apply()
        invoke.resolve()
    }

    // ----- keep-alive foreground service ------------------------------------------------------

    @Command
    fun keepAlive(invoke: Invoke) {
        val args = invoke.parseArgs(KeepAliveArgs::class.java)
        val intent = Intent(activity, KeepAliveService::class.java)
        if (args.sessions > 0) {
            intent.putExtra(KeepAliveService.EXTRA_SESSIONS, args.sessions)
            ContextCompat.startForegroundService(activity, intent)
        } else {
            activity.stopService(intent)
        }
        invoke.resolve()
    }

    // ----- screen privacy ---------------------------------------------------------------------

    @Command
    fun setSecureScreen(invoke: Invoke) {
        val args = invoke.parseArgs(SecureArgs::class.java)
        activity.runOnUiThread {
            if (args.secure) {
                activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
            } else {
                activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
            }
            invoke.resolve()
        }
    }

    // ----- hardware-backed SSH keys -----------------------------------------------------------

    @Command
    fun hardwareKeyCreate(invoke: Invoke) {
        val args = invoke.parseArgs(LabelArgs::class.java)
        val alias = "ssh-key-" + java.util.UUID.randomUUID().toString()
        try {
            val builder = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_SIGN)
                .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                .setDigests(KeyProperties.DIGEST_SHA256)
                .setUserAuthenticationRequired(true)
                .setInvalidatedByBiometricEnrollment(true)
            if (Build.VERSION.SDK_INT >= 30) {
                builder.setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
            }
            if (Build.VERSION.SDK_INT >= 28) builder.setIsStrongBoxBacked(hasStrongBox())
            val gen = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, ANDROID_KEYSTORE)
            gen.initialize(builder.build())
            val pair = gen.generateKeyPair()
            val ret = JSObject()
            ret.put("alias", alias)
            ret.put("publicKey", Base64.encodeToString(pair.public.encoded, Base64.NO_WRAP))
            prefs.edit().putString("label:$alias", args.label).apply()
            invoke.resolve(ret)
        } catch (e: Exception) {
            invoke.reject("could not create a hardware key: ${e.message}")
        }
    }

    @Command
    fun hardwareKeySign(invoke: Invoke) {
        val args = invoke.parseArgs(SignArgs::class.java)
        val fragment = activity as? FragmentActivity
            ?: return invoke.reject("biometric prompt unavailable")
        val can = BiometricManager.from(activity)
            .canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG)
        if (can != BiometricManager.BIOMETRIC_SUCCESS) {
            return invoke.reject("strong biometric authentication is not available")
        }
        try {
            val entry = keystore.getEntry(args.alias, null) as? KeyStore.PrivateKeyEntry
                ?: return invoke.reject("key not found")
            val signature = Signature.getInstance("SHA256withECDSA")
            signature.initSign(entry.privateKey)
            val data = Base64.decode(args.data, Base64.NO_WRAP)
            activity.runOnUiThread {
                val prompt = BiometricPrompt(
                    fragment,
                    ContextCompat.getMainExecutor(activity),
                    object : BiometricPrompt.AuthenticationCallback() {
                        override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                            try {
                                val sig = result.cryptoObject?.signature
                                    ?: return invoke.reject("no signature object")
                                sig.update(data)
                                val ret = JSObject()
                                ret.put("signature", Base64.encodeToString(sig.sign(), Base64.NO_WRAP))
                                invoke.resolve(ret)
                            } catch (e: Exception) {
                                invoke.reject("signing failed: ${e.message}")
                            }
                        }

                        override fun onAuthenticationError(code: Int, msg: CharSequence) {
                            invoke.reject("authentication cancelled: $msg")
                        }
                    },
                )
                val info = BiometricPrompt.PromptInfo.Builder()
                    .setTitle("ssh-client")
                    .setSubtitle(args.reason)
                    .setNegativeButtonText("Cancel")
                    .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG)
                    .build()
                prompt.authenticate(info, BiometricPrompt.CryptoObject(signature))
            }
        } catch (e: Exception) {
            invoke.reject("signing failed: ${e.message}")
        }
    }

    @Command
    fun hardwareKeyDelete(invoke: Invoke) {
        val args = invoke.parseArgs(LabelArgs::class.java)
        try {
            keystore.deleteEntry(args.label)
            prefs.edit().remove("label:${args.label}").apply()
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("could not delete the key: ${e.message}")
        }
    }

    companion object {
        private const val ANDROID_KEYSTORE = "AndroidKeyStore"
        private const val WRAP_ALIAS = "ssh-client-secure-store"
        private const val PREFS = "ssh-client-secure-store"
        private const val AES_GCM = "AES/GCM/NoPadding"
    }
}
