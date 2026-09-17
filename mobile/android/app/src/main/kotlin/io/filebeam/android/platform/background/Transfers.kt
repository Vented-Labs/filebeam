package io.filebeam.android.platform.background

import android.app.Notification
import android.Manifest
import android.content.pm.PackageManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.ConnectivityManager
import android.net.Network
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.annotation.RequiresApi
import androidx.core.content.ContextCompat
import io.filebeam.android.FilebeamApplication
import io.filebeam.android.MainActivity
import io.filebeam.android.R
import io.filebeam.android.platform.TransferUiState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch

object TransferScheduler {
    const val JOB_ID = 4100
    const val NOTIFICATION_ID = 4101
    private const val CHANNEL = "transfers"
    const val EXTRA_ROUTE = "io.filebeam.android.transfer_route"

    fun start(context: Context, liveSender: Boolean) {
        if (Build.VERSION.SDK_INT >= 34 && !liveSender) {
            val job = JobInfo.Builder(JOB_ID, ComponentName(context, TransferJobService::class.java))
                .setUserInitiated(true)
                .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                .build()
            check(context.getSystemService(JobScheduler::class.java).schedule(job) == JobScheduler.RESULT_SUCCESS) {
                context.getString(R.string.background_unavailable)
            }
        } else {
            ContextCompat.startForegroundService(context, Intent(context, TransferService::class.java))
        }
    }

    fun notification(context: Context, state: TransferUiState): Notification {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(CHANNEL, context.getString(R.string.notification_channel), NotificationManager.IMPORTANCE_LOW))
        val route = notificationRoute(state)
        val open = PendingIntent.getActivity(context, route?.hashCode() ?: 0,
            Intent(context, MainActivity::class.java).putExtra(EXTRA_ROUTE, route),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val pause = PendingIntent.getBroadcast(context, 0, Intent(context, TransferActionReceiver::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val snapshot = state.snapshot
        val builder = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_filebeam)
            .setContentTitle(context.getString(R.string.app_name))
            .setContentText(if (snapshot?.prompt != null) context.getString(R.string.notification_input) else state.phase)
            .setContentIntent(open).setOnlyAlertOnce(true).setOngoing(state.busy)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
        if (state.busy) builder.addAction(0, context.getString(R.string.pause), pause)
        val total = snapshot?.total
        if (total != null && total > 0u) {
            builder.setProgress(1000, ((snapshot.done.toDouble() / total.toDouble()) * 1000).toInt().coerceIn(0, 1000), false)
        } else builder.setProgress(0, 0, state.busy)
        return builder.build()
    }

    fun canPostNotifications(context: Context): Boolean =
        Build.VERSION.SDK_INT < 33 || notificationPermissionGranted(context)

    @RequiresApi(33)
    private fun notificationPermissionGranted(context: Context): Boolean =
        ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    internal fun notificationsAllowed(sdkInt: Int, permissionGranted: Boolean): Boolean =
        sdkInt < 33 || permissionGranted

    fun postCompletion(context: Context, state: TransferUiState) {
        if (canPostNotifications(context)) {
            context.getSystemService(NotificationManager::class.java)
                .notify(NOTIFICATION_ID + 1, notification(context, state.copy(busy = false)))
        }
    }

    /** Only opaque identifiers cross the notification boundary; link fragments and prompts stay in encrypted state. */
    internal fun notificationRoute(state: TransferUiState): String? = state.snapshot?.prompt?.id?.let { "prompt:$it" }
        ?: state.current?.id?.let { "transfer:$it" }
        ?: state.receipt?.id?.let { "transfer:$it" }
}

class TransferActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        (context.applicationContext as FilebeamApplication).transfers.pause()
    }
}

class TransferService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var task: Job? = null
    private var networkCallback: ConnectivityManager.NetworkCallback? = null
    private val coordinator get() = (application as FilebeamApplication).transfers

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(TransferScheduler.NOTIFICATION_ID, TransferScheduler.notification(this, coordinator.state.value))
        observeNetwork()
        if (task?.isActive != true) task = scope.launch {
            val updates = launch {
                coordinator.state.collectLatest {
                    if (TransferScheduler.canPostNotifications(this@TransferService)) {
                        getSystemService(NotificationManager::class.java).notify(TransferScheduler.NOTIFICATION_ID, TransferScheduler.notification(this@TransferService, it))
                    }
                }
            }
            try {
                coordinator.execute()
                if (coordinator.state.value.snapshot?.state == io.filebeam.rust.JobState.COMPLETE || coordinator.state.value.phase == "complete") {
                    TransferScheduler.postCompletion(this@TransferService, coordinator.state.value)
                }
            }
            finally { updates.cancel(); stopForeground(STOP_FOREGROUND_REMOVE); stopSelf() }
        }
        return START_NOT_STICKY
    }
    override fun onTimeout(startId: Int, fgsType: Int) { coordinator.systemStop(); stopSelf() }
    // Cancelling this service task pauses its own execution. Do not pause a queued replacement.
    override fun onDestroy() { unregisterNetwork(); scope.cancel(); super.onDestroy() }
    override fun onBind(intent: Intent?): IBinder? = null

    private fun observeNetwork() {
        if (networkCallback != null) return
        val manager = getSystemService(ConnectivityManager::class.java)
        networkCallback = object : ConnectivityManager.NetworkCallback() {
            override fun onLost(network: Network) { coordinator.networkChanged(false) }
            override fun onAvailable(network: Network) { coordinator.networkChanged(true) }
        }.also(manager::registerDefaultNetworkCallback)
    }

    private fun unregisterNetwork() {
        networkCallback?.let { getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(it) }
        networkCallback = null
    }
}

@RequiresApi(34)
class TransferJobService : JobService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var task: Job? = null
    private val coordinator get() = (application as FilebeamApplication).transfers

    override fun onStartJob(params: JobParameters): Boolean {
        if (task?.isActive == true) return true
        setNotification(params, TransferScheduler.NOTIFICATION_ID,
            TransferScheduler.notification(this, coordinator.state.value), JOB_END_NOTIFICATION_POLICY_REMOVE)
        task = scope.launch {
            val updates = launch {
                coordinator.state.collectLatest {
                    setNotification(params, TransferScheduler.NOTIFICATION_ID,
                        TransferScheduler.notification(this@TransferJobService, it), JOB_END_NOTIFICATION_POLICY_REMOVE)
                }
            }
            try {
                coordinator.execute()
                if (coordinator.state.value.snapshot?.state == io.filebeam.rust.JobState.COMPLETE || coordinator.state.value.phase == "complete") {
                    TransferScheduler.postCompletion(this@TransferJobService, coordinator.state.value)
                }
                jobFinished(params, false)
            }
            finally { updates.cancel() }
        }
        return true
    }

    override fun onStopJob(params: JobParameters): Boolean {
        coordinator.systemStop()
        task?.cancel()
        return params.stopReason != JobParameters.STOP_REASON_USER
    }
    override fun onDestroy() { scope.cancel(); super.onDestroy() }
}
