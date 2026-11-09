package io.filebeam.android.platform

import io.filebeam.rust.NativeDeviceCoordinator
import io.filebeam.rust.NativeRuntime
import io.filebeam.rust.NativeServices
import io.filebeam.rust.DeviceCoordinatorConfiguration
import io.filebeam.rust.NativeDeviceSigner
import io.filebeam.rust.DeviceSendRequest
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** Application-owned finite lifecycle for authenticated native device jobs. */
class DeviceJobLifecycle(runtime: NativeRuntime) {
    private val coordinator = NativeDeviceCoordinator(runtime)
    private val executor = Executors.newSingleThreadScheduledExecutor()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var receiverLoop: Job? = null
    @Volatile private var configured = false
    @Volatile private var foreground = false

    fun snapshots() = coordinator.snapshots()
    fun pause(id: ULong) = coordinator.pause(id)
    fun cancel(id: ULong) = coordinator.cancel(id)

    /** This application owner survives screen recreation and owns only one scoped identity. */
    fun configure(service: NativeServices, deploymentId: String, deviceId: String, publicKey: ByteArray, signer: NativeDeviceSigner) {
        coordinator.configure(service, DeviceCoordinatorConfiguration(deploymentId, deviceId, publicKey, "incoming"), signer)
        configured = true
        if (foreground) onForeground()
    }

    fun pollReceivers() = coordinator.pollReceivers()
    fun startSender(request: DeviceSendRequest) = coordinator.startSender(request)
    fun registeredTrustSnapshot() = coordinator.registeredTrustSnapshot()

    /** A single app-scoped receiver owner survives screen recreation. */
    fun onForeground() {
        foreground = true
        if (!configured || receiverLoop?.isActive == true) return
        receiverLoop = scope.launch {
            while (isActive) {
                // Readiness follows a successful authorized poll, never a UI refresh.
                runCatching {
                    coordinator.pollReceivers()
                    coordinator.advertisePresence(true)
                }
                delay(30_000)
            }
        }
    }

    /** Stop advertising before the finite expiry callback checkpoints jobs. */
    fun onBackground() {
        foreground = false
        receiverLoop?.cancel()
        receiverLoop = null
        if (configured) executor.execute { runCatching { coordinator.advertisePresence(false) } }
        onExecutionExpired()
    }

    /** Backgrounding never substitutes HTTP work for a peer job. */
    fun onExecutionExpired(timeoutMillis: Long = 4_000) {
        executor.execute { coordinator.expire(timeoutMillis.toULong()) }
    }

    fun close() {
        onBackground()
        scope.cancel()
        executor.shutdown()
        executor.awaitTermination(5, TimeUnit.SECONDS)
    }
}
