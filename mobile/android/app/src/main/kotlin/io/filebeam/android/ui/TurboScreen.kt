package io.filebeam.android.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.filebeam.android.R

@Composable
fun TurboScreen(model: FilebeamViewModel, instance: String) {
    var link by remember { mutableStateOf("") }
    Column {
        Text(stringResource(R.string.turbo), style = MaterialTheme.typography.headlineSmall)
        Text(stringResource(R.string.turbo_description))
        OutlinedTextField(link, { link = it }, label = { Text(stringResource(R.string.turbo_descriptor)) }, modifier = Modifier.fillMaxWidth())
        Button(enabled = link.isNotBlank(), onClick = { model.receiveLink(link) }) { Text(stringResource(R.string.open_turbo_download)) }
    }
}
