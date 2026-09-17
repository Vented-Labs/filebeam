package io.filebeam.android.platform.security

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import java.io.File
import java.security.KeyStore
import java.security.MessageDigest
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Per-state-root Keystore wrapper for Rust's checkpoint catalog key. The wrapped
 * key lives in no-backup storage, but is useless after backup restore or Keystore
 * invalidation; those cases deliberately surface an error instead of rekeying.
 */
class CheckpointSecretStore(context: Context, stateRoot: File) {
    private val id = MessageDigest.getInstance("SHA-256")
        .digest(stateRoot.canonicalPath.toByteArray(Charsets.UTF_8)).take(12)
        .joinToString("") { "%02x".format(it) }
    private val file = AtomicFile(File(context.noBackupFilesDir, "checkpoint-key-$id"))
    private val alias = "filebeam.checkpoint.$id.v1"

    @Synchronized fun loadOrCreate(scope: String): ByteArray {
        require(scope == "checkpoint-catalog-v1") { "Unknown checkpoint secret scope" }
        if (file.baseFile.exists()) return unwrap(file.readFully())
        val key = ByteArray(32).also { java.security.SecureRandom().nextBytes(it) }
        val wrapped = wrap(key, create = true)
        val output = file.startWrite()
        try { output.write(wrapped); file.finishWrite(output) }
        catch (error: Exception) { file.failWrite(output); throw error }
        return key
    }

    @Synchronized fun remove(scope: String) {
        require(scope == "checkpoint-catalog-v1") { "Unknown checkpoint secret scope" }
        file.delete()
    }

    private fun secretKey(create: Boolean): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(alias, null) as? SecretKey)?.let { return it }
        check(create) { "Checkpoint key is unavailable or invalidated" }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .build())
        }.generateKey()
    }

    private fun wrap(key: ByteArray, create: Boolean): ByteArray {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, secretKey(create)) }
        return byteArrayOf(1) + cipher.iv + cipher.doFinal(key)
    }

    private fun unwrap(record: ByteArray): ByteArray {
        require(record.size == 61 && record[0] == 1.toByte()) { "Invalid checkpoint key record" }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply {
            init(Cipher.DECRYPT_MODE, secretKey(create = false), GCMParameterSpec(128, record.copyOfRange(1, 13)))
        }
        return cipher.doFinal(record.copyOfRange(13, record.size)).also {
            require(it.size == 32) { "Invalid checkpoint key" }
        }
    }
}
