package io.filebeam.android.platform.background

import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.Context
import io.filebeam.android.FilebeamApplication
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/** OS-constrained discovery and ciphertext receiving; no private receiving keys are needed. */
class InboxDiscoveryService : JobService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    override fun onStartJob(params: JobParameters): Boolean {
        scope.launch {
            val app = application as FilebeamApplication
            val success = runCatching {
                val instance = app.settings.values.first().instance
                app.accounts.restoreReceivingSession(instance)
                app.accountSessions.service(instance).accountReceiveAutomatically()
            }.isSuccess
            jobFinished(params, !success)
        }
        return true
    }
    override fun onStopJob(params: JobParameters): Boolean {
        (application as FilebeamApplication).accounts.cancelAutomaticReceiving()
        return true
    }
    override fun onDestroy() { (application as FilebeamApplication).accounts.cancelAutomaticReceiving(); scope.cancel(); super.onDestroy() }
    companion object {
        private const val ID = 3301
        fun schedule(context: Context) {
            context.getSystemService(JobScheduler::class.java).schedule(JobInfo.Builder(ID, ComponentName(context, InboxDiscoveryService::class.java))
                .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY).setPeriodic(15 * 60 * 1000L).setPersisted(true).build())
        }
    }
}
