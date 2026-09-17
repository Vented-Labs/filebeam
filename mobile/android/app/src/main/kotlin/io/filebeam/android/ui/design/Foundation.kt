package io.filebeam.android.ui.design

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription

@Composable
fun ProductionGroupCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Card(modifier, colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow), shape = MaterialTheme.shapes.large) {
        Column(content = content)
    }
}

@Composable
fun OptionRow(
    title: String,
    supporting: String? = null,
    icon: ApprovedIcon? = null,
    selected: Boolean = false,
    enabled: Boolean = true,
    onClick: () -> Unit,
    trailing: @Composable (() -> Unit)? = null,
) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = FilebeamSpace.MinimumTouchTarget).clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) { if (selected) stateDescription = "Selected" }
            .padding(horizontal = FilebeamSpace.Medium, vertical = FilebeamSpace.Small),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(FilebeamSpace.Small),
    ) {
        icon?.let { ApprovedIcon(it, null) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            supporting?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
        trailing?.invoke()
    }
}

@Composable
fun ActionDock(label: String, enabled: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier, icon: ApprovedIcon = ApprovedIcon.Upload) {
    Button(onClick = onClick, enabled = enabled, modifier = modifier.fillMaxWidth().navigationBarsPadding().imePadding().heightIn(min = FilebeamSpace.PrimaryActionHeight)) {
        ApprovedIcon(icon, null)
        Text(label, Modifier.padding(start = FilebeamSpace.Small))
    }
}

@Composable
fun EmptyErrorState(title: String, detail: String, actionLabel: String, isError: Boolean, onAction: () -> Unit, modifier: Modifier = Modifier) {
    val container = if (isError) MaterialTheme.colorScheme.errorContainer else MaterialTheme.colorScheme.surfaceContainer
    val content = if (isError) MaterialTheme.colorScheme.onErrorContainer else MaterialTheme.colorScheme.onSurface
    Card(modifier.fillMaxWidth(), colors = CardDefaults.cardColors(containerColor = container)) {
        Column(Modifier.padding(FilebeamSpace.Large), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Small)) {
            ApprovedIcon(if (isError) ApprovedIcon.Shield else ApprovedIcon.Folder, null, tint = content)
            Text(title, style = MaterialTheme.typography.titleLarge, color = content)
            Text(detail, style = MaterialTheme.typography.bodyMedium, color = content)
            OutlinedButton(onClick = onAction) { Text(actionLabel) }
        }
    }
}
