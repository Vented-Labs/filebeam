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

class FilebeamApplication : Application() {
    val settings by lazy { SettingsStore(this) }
    val transfers by lazy { TransferCoordinator(this, settings) }
    val accounts: AccountService by lazy { NativeAccountService(BuildConfig.DEBUG) }
    val notes: NotesService by lazy { NativeNotesService(BuildConfig.DEBUG) }
    val turbo: TurboService by lazy { NativeTurboService(BuildConfig.DEBUG) }
    val inbox: InboxService by lazy { NativeInboxService(BuildConfig.DEBUG) }
}
