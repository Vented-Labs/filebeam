package io.filebeam.android.ui.design

import androidx.annotation.DrawableRes
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import io.filebeam.android.R

/** Approved Iconsax and custom Filebeam artwork. Vectors retain two-tone alpha and semantic tint. */
enum class ApprovedIcon(@DrawableRes val resource: Int) {
    Upload(R.drawable.ic_approved_upload), Archive(R.drawable.ic_approved_archive), Folder(R.drawable.ic_approved_folder), File(R.drawable.ic_approved_file),
    Download(R.drawable.ic_approved_download), Transfers(R.drawable.ic_approved_transfers), Shield(R.drawable.ic_approved_shield),
    Settings(R.drawable.ic_approved_settings), Storage(R.drawable.ic_approved_storage), Add(R.drawable.ic_approved_add),
    HttpServer(R.drawable.ic_custom_http_server), WebRtcP2p(R.drawable.ic_custom_webrtc_p2p),
}

@Composable
fun FilebeamMark(contentDescription: String?, modifier: Modifier = Modifier, tint: Color = androidx.compose.material3.LocalContentColor.current) =
    Icon(painterResource(R.drawable.ic_filebeam_mark), contentDescription, modifier, tint)

@Composable
fun ApprovedIcon(
    icon: ApprovedIcon,
    contentDescription: String?,
    modifier: Modifier = Modifier,
    tint: Color = androidx.compose.material3.LocalContentColor.current,
) = Icon(painterResource(icon.resource), contentDescription, modifier, tint)
