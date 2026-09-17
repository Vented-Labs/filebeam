package io.filebeam.android.ui.design

import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import io.filebeam.android.ui.FilebeamTheme

@Preview(showBackground = true)
@Composable
private fun DesignFoundationPreview() = FilebeamTheme {
    ProductionGroupCard {
        OptionRow("Transfer options", "48 hours - Password off", ApprovedIcon.Settings, onClick = {})
        HorizontalDivider()
        OptionRow("Choose files", "Encrypted on this device", ApprovedIcon.Folder, selected = true, onClick = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun EmptyStatePreview() = FilebeamTheme {
    EmptyErrorState("Choose files to send", "Original quality. Encrypted on this device.", "Choose files", false, onAction = {})
}
