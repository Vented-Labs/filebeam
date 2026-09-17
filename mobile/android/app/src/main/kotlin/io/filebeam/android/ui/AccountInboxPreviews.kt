package io.filebeam.android.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import io.filebeam.android.R
import io.filebeam.android.platform.services.InboxItem
import io.filebeam.android.platform.services.InboxItemState

@Preview(showBackground = true, widthDp = 360)
@Composable
private fun AccountSecurityPreview() = FilebeamTheme {
    Column {
        Text(stringResource(R.string.account), style = MaterialTheme.typography.headlineSmall)
        Text(stringResource(R.string.key_export_review))
    }
}

@Preview(showBackground = true, widthDp = 360)
@Composable
private fun InboxLockedAndExpiredPreview() = FilebeamTheme {
    Column {
        InboxRow(InboxItem("opaque-1", "Encrypted delivery", "authenticated", 0, InboxItemState.Locked), false, {}, {})
        InboxRow(InboxItem("opaque-2", "Encrypted delivery", "authenticated", 0, InboxItemState.Expired), false, {}, {})
    }
}
