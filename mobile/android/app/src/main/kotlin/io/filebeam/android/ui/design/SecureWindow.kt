package io.filebeam.android.ui.design

import android.app.Activity
import android.util.Log
import io.filebeam.android.BuildConfig
import android.view.WindowManager
import androidx.activity.compose.LocalActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import java.util.WeakHashMap

/** Compose disposal can overlap during dialog animation, so secure ownership is counted per window. */
@Composable
fun SecureWindowEffect() {
    val activity = LocalActivity.current
    // Debug builds can opt out for evidence capture: adb shell setprop log.tag.FilebeamCapture DEBUG
    if (BuildConfig.DEBUG && Log.isLoggable("FilebeamCapture", Log.DEBUG)) return
    DisposableEffect(activity) {
        activity?.let(SecureWindows::acquire)
        onDispose { activity?.let(SecureWindows::release) }
    }
}

private object SecureWindows {
    private val owners = WeakHashMap<Activity, Int>()

    @Synchronized fun acquire(activity: Activity) {
        val count = owners[activity] ?: 0
        if (count == 0) activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        owners[activity] = count + 1
    }

    @Synchronized fun release(activity: Activity) {
        val count = owners[activity] ?: return
        if (count <= 1) {
            owners.remove(activity)
            activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        } else owners[activity] = count - 1
    }
}
