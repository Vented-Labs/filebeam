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
import io.filebeam.rust.ClientConfig
import io.filebeam.rust.NativeRuntime
import java.io.File

class FilebeamApplication : Application() {
    val settings by lazy { SettingsStore(this) }
    val nativeRuntime by lazy {
        NativeRuntime(
            ClientConfig(
                File(noBackupFilesDir, "transfers").absolutePath,
                128u,
                2u,
                false,
                BuildConfig.DEBUG,
            ),
        )
    }
    val accountSessions by lazy { AccountSessionRegistry(BuildConfig.DEBUG, nativeRuntime) }
    val accounts: AccountService by lazy { NativeAccountService(this, accountSessions) }
    val transfers by lazy { TransferCoordinator(this, settings, accountSessions, nativeRuntime, accounts) }
    val notes: NotesService by lazy { NativeNotesService(accountSessions) { instance, request -> transfers.startLiveNote(instance, request) } }
    val turbo: TurboService by lazy { NativeTurboService(accountSessions) }
    val inbox: InboxService by lazy { NativeInboxService(accountSessions, accounts, transfers) }
}
