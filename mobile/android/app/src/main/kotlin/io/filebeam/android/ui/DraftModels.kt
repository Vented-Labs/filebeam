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

data class RecipientDraft(val username: String = "", val status: RecipientStatus = RecipientStatus.NONE, val error: String? = null)

data class SendDraft(
    val sources: List<SelectedSource> = emptyList(),
    val transport: Transport = Transport.HTTP,
    val archive: Boolean = false,
    val turbo: Boolean = false,
    val passwordProtected: Boolean = false,
    val retentionHours: String = "",
    val recipient: RecipientDraft = RecipientDraft(),
)

data class NoteDraft(
    val title: String = "",
    val body: String = "",
    val password: String = "",
    val retentionHours: String = "",
    val language: String = "plain",
    val burnAfterRead: Boolean = false,
    val live: Boolean = false,
)

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
