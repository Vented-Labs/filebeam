package io.filebeam.android.ui.feature.send

import android.net.Uri
import androidx.annotation.DrawableRes
import androidx.compose.runtime.Immutable
import io.filebeam.android.R
import io.filebeam.android.platform.ValidatedRecipient

enum class SendMode { Files, Notes }

@Immutable
data class SelectedSource(
    val uri: Uri,
    val name: String,
    val sizeBytes: Long?,
    val mimeType: String?,
    val relativePath: String? = null,
    val error: String? = null,
) {
    val identity: String get() = uri.toString()
}

/** The icon family the web queue uses, chosen from the MIME type and extension. */
@DrawableRes
fun fileIcon(name: String, mimeType: String?): Int {
    val type = mimeType.orEmpty().lowercase()
    val extension = name.substringAfterLast('.', "").lowercase()
    return when {
        type.startsWith("image/") -> R.drawable.ic_approved_image
        type.startsWith("video/") -> R.drawable.ic_approved_video
        type.startsWith("audio/") -> R.drawable.ic_approved_music
        extension in setOf("zip", "tar", "gz", "tgz", "7z", "rar", "bz2", "xz") || "zip" in type || "compressed" in type -> R.drawable.ic_approved_archive
        extension in setOf("js", "ts", "tsx", "jsx", "json", "php", "py", "rs", "kt", "java", "c", "cpp", "h", "go", "rb", "sh", "css", "html", "xml", "yml", "yaml", "toml", "md", "sql", "env") ||
            type.startsWith("text/x-") || type == "application/json" -> R.drawable.ic_approved_code
        else -> R.drawable.ic_approved_file
    }
}

/** Languages a note can be labelled with; keys match the web and the wire format. */
enum class NoteLanguage(val key: String, val label: String, val extension: String) {
    Plain("plain", "Plain text", "txt"),
    Php("php", "PHP", "php"),
    Dotenv("dotenv", ".ENV", "env"),
    JavaScript("javascript", "JavaScript", "js"),
    TypeScript("typescript", "TypeScript", "ts"),
    Json("json", "JSON", "json"),
    Markdown("markdown", "Markdown", "md"),
    Css("css", "CSS", "css"),
    Html("html", "HTML", "html");

    companion object {
        fun fromKey(key: String?) = entries.firstOrNull { it.key == key } ?: Plain
    }
}

@Immutable
data class NoteDraft(
    val title: String = "",
    val body: String = "",
    val language: NoteLanguage = NoteLanguage.Plain,
    val wrap: Boolean = true,
    /** Kept in memory only; never written to the draft store. */
    val password: String? = null,
    val retentionHours: Long? = null,
    val includeKey: Boolean = true,
    val burnOnRead: Boolean = false,
    val driver: String? = null,
) {
    val bytes: Int get() = body.toByteArray(Charsets.UTF_8).size
    val isEmpty: Boolean get() = title.isEmpty() && body.isEmpty()
}

@Immutable
data class FileDraft(
    val sources: List<SelectedSource> = emptyList(),
    val driver: String? = null,
    val archive: Boolean = false,
    /** Kept in memory only; never written to the draft store. */
    val password: String? = null,
    val retentionHours: Long? = null,
    val includeKey: Boolean = true,
    val recipient: ValidatedRecipient? = null,
    val attachedNote: NoteDraft? = null,
) {
    val knownBytes: Long get() = sources.sumOf { it.sizeBytes ?: 0L }
    val sizeKnown: Boolean get() = sources.all { it.sizeBytes != null }
}

const val ATTACHED_NOTE_MAX_BYTES = 64 * 1024
const val NOTE_TITLE_MAX = 160
const val PASSWORD_MIN = 8
const val PASSWORD_MAX = 128

/** Why sending is currently blocked, in the order a person should fix things. */
sealed interface SendBlocker {
    data object LoadingPolicy : SendBlocker
    data class PolicyUnavailable(val detail: String?) : SendBlocker
    data object SignInRequired : SendBlocker
    data object TransportDisabled : SendBlocker
    data object NothingSelected : SendBlocker
    data class TooManyFiles(val maximum: Long) : SendBlocker
    data class TooLarge(val limitBytes: Long, val webrtcFits: Boolean) : SendBlocker
    data class NoteTooLarge(val limitBytes: Long) : SendBlocker
    data object AttachedNoteInvalid : SendBlocker
    data object PasswordTooShort : SendBlocker
    data object SourceError : SendBlocker
}

/** What the Send tab is showing. */
sealed interface SendStage {
    data object Composing : SendStage

    /** A transfer started from this tab is running or finished; the share view is shown. */
    data class Sharing(val kind: ShareKind, val turbo: Boolean, val startedAtMillis: Long) : SendStage
}

enum class ShareKind { Files, LiveFiles, Note, LiveNote, Inbox }
