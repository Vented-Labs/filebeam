package io.filebeam.android.ui

import androidx.annotation.StringRes
import io.filebeam.android.R
import io.filebeam.android.platform.AppSettings

sealed interface InstanceTransactionStatus {
    data object Idle : InstanceTransactionStatus
    data object Checking : InstanceTransactionStatus
    data object ReadyToCommit : InstanceTransactionStatus
    data object ConfirmActiveTransfer : InstanceTransactionStatus
    data class Error(@StringRes val message: Int) : InstanceTransactionStatus
}

/** UI-owned input is separate from the last committed instance configuration. */
data class InstanceSettingsTransaction(
    val committed: AppSettings = AppSettings(),
    val draftInstance: String = committed.instance,
    val draftRelayOnly: Boolean = committed.relayOnly,
    val status: InstanceTransactionStatus = InstanceTransactionStatus.Idle,
) {
    val dirty get() = draftInstance.trim().trimEnd('/') != committed.instance || draftRelayOnly != committed.relayOnly
}

fun InstanceSettingsTransaction.withInput(instance: String = draftInstance, relayOnly: Boolean = draftRelayOnly) =
    copy(draftInstance = instance, draftRelayOnly = relayOnly, status = InstanceTransactionStatus.Idle)

internal fun instanceErrorFor(error: Throwable) = when {
    error.message?.contains("active transfer", ignoreCase = true) == true -> R.string.instance_error_active_transfer
    error.message?.contains("valid Filebeam instance", ignoreCase = true) == true -> R.string.instance_error_invalid
    else -> R.string.instance_error_check_failed
}
