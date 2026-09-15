package io.filebeam.android

import android.app.Application
import io.filebeam.android.platform.TransferCoordinator
import io.filebeam.android.platform.SettingsStore
import io.filebeam.android.platform.services.AccountService
import io.filebeam.android.platform.services.InboxService
import io.filebeam.android.platform.services.NotesService
import io.filebeam.android.platform.services.TurboService
import io.filebeam.android.platform.services.NativeAccountService
import io.filebeam.android.platform.services.NativeInboxService
import io.filebeam.android.platform.services.NativeNotesService
import io.filebeam.android.platform.services.NativeTurboService
import io.filebeam.android.platform.services.AccountSessionRegistry

class FilebeamApplication : Application() {
    val settings by lazy { SettingsStore(this) }
    val accountSessions by lazy { AccountSessionRegistry(BuildConfig.DEBUG) }
    val transfers by lazy { TransferCoordinator(this, settings, accountSessions) }
    val accounts: AccountService by lazy { NativeAccountService(this, accountSessions) }
    val notes: NotesService by lazy { NativeNotesService(accountSessions) { instance, request -> transfers.startLiveNote(instance, request) } }
    val turbo: TurboService by lazy { NativeTurboService(accountSessions) }
    val inbox: InboxService by lazy { NativeInboxService(accountSessions, accounts, transfers) }
}
