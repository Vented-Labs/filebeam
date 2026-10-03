package io.filebeam.android.ui

import android.net.Uri
import io.filebeam.rust.Transport

data class SelectedSource(
    val uri: Uri,
    val identity: String = uri.toString(),
    val displayName: String = uri.lastPathSegment ?: "file",
    val sizeBytes: Long? = null,
    val relativePath: String? = null,
    val error: String? = null,
)

enum class RecipientStatus { NONE, UNVALIDATED, VALIDATING, VALIDATED, INVALID, CONFLICT }

/** An instance-scoped recipient key returned by account discovery, never free-form input. */
data class ValidatedRecipient(
    val username: String,
    val origin: String,
    val id: ULong,
    val accountKeyBundleId: ULong,
    val publicKey: String,
    val revision: Long,
)

data class RecipientDraft(
    val username: String = "",
    val status: RecipientStatus = RecipientStatus.NONE,
    val error: String? = null,
    val identity: ValidatedRecipient? = null,
)

data class SendDraft(
    val sources: List<SelectedSource> = emptyList(),
    val transport: Transport = Transport.HTTP,
    val archive: Boolean = false,
    val turbo: Boolean = false,
    val passwordProtected: Boolean = false,
    val retentionHours: String = "",
    val recipient: RecipientDraft = RecipientDraft(),
    val driver: String? = null,
    /** Link presentation preference for files; independent from note sharing. */
    val includeKeyInLink: Boolean = true,
    val attachedNote: NoteDraft? = null,
)

data class NoteDraft(
    val title: String = "",
    val body: String = "",
    /** Enables a password prompt at submission; the password itself is process-memory only. */
    val passwordEnabled: Boolean = false,
    val retentionHours: String = "",
    val language: String = "plain",
    val burnAfterRead: Boolean = false,
    val live: Boolean = false,
    val driver: String? = null,
    /** Link presentation preference for notes; independent from file sharing. */
    val includeKeyInLink: Boolean = true,
)

enum class SendContent { FILES, NOTES }

fun NoteDraft.attachmentError(): String? = when {
    body.isEmpty() || body.toByteArray(Charsets.UTF_8).size > 64 * 1024 -> "Attached note must contain 1 byte to 64 KiB of UTF-8"
    title.codePointCount(0, title.length) > 160 -> "Attached note title must contain at most 160 characters"
    language !in listOf("plain", "php", "dotenv", "javascript", "typescript", "json", "markdown", "css", "html") -> "Unsupported attached note language"
    else -> null
}

internal fun shouldRestoreDraft(launchRevision: Long, currentRevision: Long) = launchRevision == currentRevision

/** Strictly parses a user-entered retention. Empty means unspecified; zero and overflow are errors. */
fun retentionHours(value: String): Result<ULong?> = when {
    value.isBlank() -> Result.success(null)
    value.any { !it.isDigit() } -> Result.failure(IllegalArgumentException("Retention must be a positive whole number"))
    else -> value.toULongOrNull()?.takeIf { it > 0uL }?.let(Result.Companion::success)
        ?: Result.failure(IllegalArgumentException("Retention must be a positive value within the supported range"))
}

fun SendDraft.recipientConflict(): String? = recipient.username.takeIf(String::isNotBlank)?.let {
    when {
        transport != Transport.HTTP -> "Recipient delivery requires HTTP"
        turbo -> "Recipient delivery cannot be used with Turbo Transfer"
        passwordProtected -> "Recipient delivery cannot be used with password protection"
        else -> null
    }
}
