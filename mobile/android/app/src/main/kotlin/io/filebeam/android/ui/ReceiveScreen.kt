package io.filebeam.android.ui

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.filebeam.android.R

@Composable
fun ReceiveScreen(model: FilebeamViewModel, busy: Boolean, receive: () -> Unit) {
    Text(stringResource(R.string.receive_title), style = MaterialTheme.typography.headlineSmall)
    Text(stringResource(R.string.receive_description))
    OutlinedTextField(model.link, { model.link = it }, enabled = !busy, label = { Text(stringResource(R.string.transfer_link)) }, modifier = Modifier.fillMaxWidth())
    Button(enabled = !busy && model.link.isNotBlank(), onClick = receive) { Text(stringResource(R.string.start_download)) }
}
