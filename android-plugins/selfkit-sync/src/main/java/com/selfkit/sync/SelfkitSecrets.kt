package com.selfkit.sync

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * 32-byte wrapping key for the Rust secret file (data key, server passwords).
 * The key itself is stored encrypted by a non-exportable Android Keystore AES key,
 * so copying the app's files elsewhere doesn't reveal the secrets.
 */
object SelfkitSecrets {
    private const val ALIAS = "selfkit_sync_wrap"
    private const val PREFS = "selfkit_sync"
    private const val PREF_KEY = "wrap_v1"

    private fun keystoreKey(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return gen.generateKey()
    }

    @Synchronized
    fun wrapKey(ctx: Context): ByteArray {
        val prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val k = keystoreKey()
        prefs.getString(PREF_KEY, null)?.let { stored ->
            val raw = Base64.decode(stored, Base64.NO_WRAP)
            val c = Cipher.getInstance("AES/GCM/NoPadding")
            c.init(Cipher.DECRYPT_MODE, k, GCMParameterSpec(128, raw.copyOfRange(0, 12)))
            return c.doFinal(raw.copyOfRange(12, raw.size))
        }
        val fresh = ByteArray(32).also { SecureRandom().nextBytes(it) }
        val c = Cipher.getInstance("AES/GCM/NoPadding")
        c.init(Cipher.ENCRYPT_MODE, k)
        val out = c.iv + c.doFinal(fresh)
        prefs.edit().putString(PREF_KEY, Base64.encodeToString(out, Base64.NO_WRAP)).apply()
        return fresh
    }
}
