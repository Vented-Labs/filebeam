package io.filebeam.android.ui.feature.settings

import android.os.StatFs
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.platform.storage.StorageUsage
import io.filebeam.android.platform.storage.StorageUsageBucket
import io.filebeam.android.platform.storage.StorageUsageScanner
import io.filebeam.android.ui.common.formatBytes
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.Hairline
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Spinner
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.SubScreen
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

/** How much private space transfers occupy on this device, split by what the files are for. */
@Composable
fun StorageScreen() {
    val app = LocalAppModel.current
    val context = LocalContext.current
    val palette = Fb.palette
    var usage by remember { mutableStateOf<StorageUsage?>(null) }
    var free by remember { mutableStateOf(0L) }
    LaunchedEffect(Unit) {
        withContext(Dispatchers.IO) {
            usage = StorageUsageScanner.scan(app.app.transfers.storage.root, File(context.noBackupFilesDir, "transfers"))
            free = runCatching { StatFs(context.noBackupFilesDir.path).availableBytes }.getOrDefault(0L)
        }
    }
    SubScreen(stringResource(R.string.storage_title)) {
        val loaded = usage
        if (loaded == null) {
            item { Box(Modifier.fillMaxWidth().padding(Space.xxxl), contentAlignment = Alignment.Center) { Spinner() } }
            return@SubScreen
        }
        val buckets = listOf(
            Triple(R.string.storage_verified, loaded.verified, palette.progressFill),
            Triple(R.string.storage_snapshots, loaded.snapshots, palette.info.ink),
            Triple(R.string.storage_recovery, loaded.recovery, palette.warning.ink),
        )
        item {
            Panel(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.lg)) {
                    FbText(formatBytes(loaded.bytes), style = Fb.type.display)
                    FbText(stringResource(R.string.storage_free, formatBytes(free)), style = Fb.type.small, color = palette.textMuted)
                    Canvas(Modifier.fillMaxWidth().height(10.dp)) {
                        val radius = CornerRadius(size.height / 2)
                        drawRoundRect(palette.progressTrack, cornerRadius = radius)
                        val total = loaded.bytes.coerceAtLeast(1).toFloat()
                        var x = 0f
                        buckets.forEach { (_, bucket, color) ->
                            val width = size.width * (bucket.bytes / total)
                            if (width > 0f) drawRoundRect(color, Offset(x, 0f), Size(width, size.height), radius)
                            x += width
                        }
                    }
                }
            }
        }
        item {
            Panel(Modifier.fillMaxWidth()) {
                buckets.forEachIndexed { index, (label, bucket, color) ->
                    if (index > 0) Hairline()
                    BucketRow(stringResource(label), bucket, color)
                }
            }
        }
        item { FbText(stringResource(R.string.storage_hint), style = Fb.type.small, color = palette.textMuted) }
    }
}

@Composable
private fun BucketRow(label: String, bucket: StorageUsageBucket, color: Color) {
    Row(Modifier.fillMaxWidth().padding(Space.lg), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
        Canvas(Modifier.size(12.dp)) { drawCircle(color) }
        Column(Modifier.weight(1f)) {
            FbText(label, style = Fb.type.bodyStrong)
            FbText(pluralStringResource(R.plurals.storage_files, bucket.files, bucket.files), style = Fb.type.small, color = Fb.palette.textMuted)
        }
        FbText(formatBytes(bucket.bytes), style = Fb.type.numeric, color = Fb.palette.text)
    }
}
