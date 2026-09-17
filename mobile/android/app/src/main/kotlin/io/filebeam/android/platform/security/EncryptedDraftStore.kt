package io.filebeam.android.platform.security

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import org.json.JSONObject
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

sealed interface DraftLoadResult {
    data object Missing : DraftLoadResult
    data class Restored(val value: JSONObject) : DraftLoadResult
    /** The old ciphertext cannot be recovered after keystore invalidation. It is not an empty draft. */
    data class Unavailable(val cause: Throwable) : DraftLoadResult
}

/** Process-restorable composer drafts. Note text is encrypted; passwords are RAM-only and never serialized. */
class EncryptedDraftStore(context: Context) {
    private val file = AtomicFile(File(context.noBackupFilesDir, "composer-drafts"))
    private val alias = "filebeam.composer-drafts.v1"

    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(alias, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build())
        }.generateKey()
    }

    @Synchronized fun save(value: JSONObject) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val output = file.startWrite()
        try {
            output.write(byteArrayOf(1) + cipher.iv + cipher.doFinal(value.toString().toByteArray(Charsets.UTF_8)))
            file.finishWrite(output)
        } catch (error: Exception) { file.failWrite(output); throw error }
    }

    @Synchronized fun loadResult(): DraftLoadResult {
        if (!file.baseFile.exists()) return DraftLoadResult.Missing
        return try {
            val bytes = file.readFully()
            require(bytes.size >= 29 && bytes[0] == 1.toByte()) { "Invalid encrypted draft" }
            val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply {
                init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes.copyOfRange(1, 13)))
            }
            DraftLoadResult.Restored(JSONObject(cipher.doFinal(bytes.copyOfRange(13, bytes.size)).toString(Charsets.UTF_8)))
        } catch (error: Exception) {
            DraftLoadResult.Unavailable(error)
        }
    }

    /** Compatibility for callers that only need a successfully restored value. */
    @Synchronized fun load(): JSONObject? = (loadResult() as? DraftLoadResult.Restored)?.value

    @Synchronized fun clear() = file.delete()
}
