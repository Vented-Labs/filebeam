package io.filebeam.android

import android.app.Application
import android.app.Activity
import android.os.Bundle
import androidx.core.net.toUri
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
import io.filebeam.rust.SecretStoreCallback
import io.filebeam.rust.SourceCallback
import io.filebeam.android.platform.security.CheckpointSecretStore
import io.filebeam.android.platform.devices.DeviceRegistry
import io.filebeam.android.platform.devices.NativeDeviceRegistry
import io.filebeam.android.platform.DeviceJobLifecycle
import java.io.File

class FilebeamApplication : Application() {
    val settings by lazy { SettingsStore(this) }
    val nativeRuntime by lazy {
        NativeRuntime.newWithSecretStoreAndSource(
            ClientConfig(
                File(noBackupFilesDir, "transfers").absolutePath,
                128u,
                2u,
                false,
                BuildConfig.DEBUG,
            ),
            object : SecretStoreCallback {
                private val secrets = CheckpointSecretStore(this@FilebeamApplication, File(noBackupFilesDir, "transfers"))
                override fun loadOrCreate(scope: String): ByteArray = secrets.loadOrCreate(scope)
                override fun remove(scope: String) = secrets.remove(scope)
            },
            object : SourceCallback {
                override fun open(identity: String): Long = contentResolver.openFileDescriptor(identity.toUri(), "r")?.use { it.detachFd().toLong() }
                    ?: error("The selected document is no longer available.")
                override fun mutationToken(identity: String): String = identity
            },
        )
    }
    val accountSessions by lazy { AccountSessionRegistry(BuildConfig.DEBUG, nativeRuntime) }
    val accounts: AccountService by lazy { NativeAccountService(this, accountSessions) }
    val transfers by lazy { TransferCoordinator(this, settings, accountSessions, nativeRuntime, accounts) }
    val notes: NotesService by lazy { NativeNotesService(accountSessions, { instance, request -> transfers.startLiveNote(instance, request) }, transfers::endLiveNote, transfers::createHttpNote, transfers::openNote) }
    val turbo: TurboService by lazy { NativeTurboService(accountSessions) }
    val inbox: InboxService by lazy { NativeInboxService(accountSessions, accounts, transfers) }
    val deviceJobs by lazy { DeviceJobLifecycle(nativeRuntime) }
    /** Registry and authenticated peer jobs share this application-owned coordinator. */
    val devices: DeviceRegistry by lazy { NativeDeviceRegistry(this, accountSessions, nativeRuntime, deviceJobs) }

    private var startedActivities = 0

    override fun onCreate() {
        super.onCreate()
        registerActivityLifecycleCallbacks(object : ActivityLifecycleCallbacks {
            override fun onActivityStarted(activity: Activity) {
                startedActivities++
                if (startedActivities == 1) deviceJobs.onForeground()
            }
            override fun onActivityStopped(activity: Activity) {
                startedActivities = (startedActivities - 1).coerceAtLeast(0)
                if (startedActivities == 0) deviceJobs.onBackground()
            }
            override fun onActivityCreated(activity: Activity, state: Bundle?) = Unit
            override fun onActivityResumed(activity: Activity) = Unit
            override fun onActivityPaused(activity: Activity) = Unit
            override fun onActivitySaveInstanceState(activity: Activity, state: Bundle) = Unit
            override fun onActivityDestroyed(activity: Activity) = Unit
        })
    }

    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        if (level >= TRIM_MEMORY_UI_HIDDEN) deviceJobs.onBackground()
    }
}
