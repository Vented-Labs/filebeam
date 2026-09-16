package io.filebeam.android.ui

enum class ReceiveKind { FILE, NOTE, TURBO, WEB_RTC, UNKNOWN }
data class ReceiveIngress(val raw: String, val kind: ReceiveKind, val isFullUrl: Boolean)

enum class ReceiptDisposition { PRIVATE_VERIFIED, EXPORTED, FAILED }
data class TransferReceipt(val id: String, val disposition: ReceiptDisposition, val result: String?, val message: String?)

data class JobCapabilities(
    val canPause: Boolean = false,
    val canResume: Boolean = false,
    val canRetryExport: Boolean = false,
    val canEndLive: Boolean = false,
    val canRevoke: Boolean = false,
    val canRemoveLocal: Boolean = false,
)

/** A catalog row is keyed by checkpoint ID so an active snapshot never renders as a duplicate saved row. */
fun <T> dedupeActiveCatalog(activeId: String?, catalog: List<T>, id: (T) -> String): List<T> =
    catalog.filter { id(it) != activeId }

sealed interface NotificationRoute {
    data class Transfer(val id: String) : NotificationRoute
    data class Prompt(val id: ULong) : NotificationRoute
}

/** Presentation-only ingress classification. The core remains the authority that validates the link. */
fun parseReceiveIngress(value: String): ReceiveIngress? {
    val trimmed = value.trim()
    if (trimmed.isBlank()) return null
    val uri = runCatching { android.net.Uri.parse(trimmed) }.getOrNull()
    val fullUrl = uri?.scheme in setOf("https", "http", "filebeam")
    val path = uri?.path.orEmpty().lowercase()
    val kind = when {
        path.contains("note") -> ReceiveKind.NOTE
        path.contains("turbo") -> ReceiveKind.TURBO
        path.contains("webrtc") || path.contains("live") -> ReceiveKind.WEB_RTC
        fullUrl || trimmed.matches(Regex("[A-Za-z0-9_-]{8,}")) -> ReceiveKind.FILE
        else -> ReceiveKind.UNKNOWN
    }
    return ReceiveIngress(trimmed, kind, fullUrl)
}
