package io.filebeam.android.ui.feature.send

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import io.filebeam.android.R
import io.filebeam.android.ui.design.Emblem
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbSwitch
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.Segment
import io.filebeam.android.ui.design.SegmentedRail
import io.filebeam.android.ui.design.SmoothProgress
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.shell.Staggered
import io.filebeam.android.ui.shell.TabScreen

@Composable
fun SendScreen() {
    var mode by remember { mutableStateOf("files") }
    var transport by remember { mutableStateOf("http") }
    var on by remember { mutableStateOf(false) }
    TabScreen {
        item {
            Staggered(0) {
                SegmentedRail(listOf(Segment("files", "Files", R.drawable.ic_approved_folder), Segment("notes", "Notes", R.drawable.ic_approved_note)), mode, { mode = it })
            }
        }
        item {
            Staggered(1) {
                Panel(Modifier.fillMaxWidth()) {
                    Column(Modifier.fillMaxWidth().padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.lg), horizontalAlignment = Alignment.CenterHorizontally) {
                        SegmentedRail(listOf(Segment("http", "HTTP", R.drawable.ic_custom_http_server, "Download later"), Segment("webrtc", "WebRTC", R.drawable.ic_custom_webrtc_p2p, "Send live")), transport, { transport = it })
                        Emblem(engaged = on)
                        FbText("Send files privately", style = Fb.type.display)
                        FbText("They are encrypted on this device before they leave it.", style = Fb.type.body, color = Fb.palette.textMuted)
                        FbButton("Choose files", { on = !on }, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_folder, trailingIcon = R.drawable.ic_approved_arrow_up)
                        FbSwitch(on, { on = it })
                        SmoothProgress(if (on) 0.7f else 0.1f)
                    }
                }
            }
        }
    }
}
