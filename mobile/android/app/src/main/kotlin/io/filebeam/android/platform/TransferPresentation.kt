package io.filebeam.android.platform

import io.filebeam.rust.SavedTransferDetails
import io.filebeam.rust.Transport

enum class TransferKind { UPLOAD, DOWNLOAD, INBOX_DOWNLOAD, NOTE_LIVE, EXPORT, CONTROL }
enum class ReceiptState { VERIFIED_PRIVATE, APP_PRIVATE_PUBLISHED, SAF_EXPORTED, FAILED }

data class TransferActions(val pause: Boolean = false, val resume: Boolean = false, val retrySafExport: Boolean = false, val endLive: Boolean = false, val revokeRemote: Boolean = false, val removeLocal: Boolean = false)
data class CurrentTransfer(val id: String, val kind: TransferKind, val transport: Transport?, val actions: TransferActions)
data class SavedTransferPresentation(val details: SavedTransferDetails, val actions: TransferActions, val receiptState: ReceiptState?)

internal fun SavedTransferDetails.presentation() = SavedTransferPresentation(this,
    TransferActions(resume = canResume, retrySafExport = canRetrySave, endLive = canEndLive, revokeRemote = canRevokeRemote, removeLocal = canRemoveLocal),
    // This is a native/app-private checkpoint publication, not a SAF destination write.
    when { verifiedPrivately -> ReceiptState.VERIFIED_PRIVATE; exported -> ReceiptState.APP_PRIVATE_PUBLISHED; else -> null })
