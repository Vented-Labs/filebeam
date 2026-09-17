package io.filebeam.android.ui.send

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.ProductionGroupCard
import io.filebeam.android.R
import androidx.compose.ui.res.stringResource

private val noteLanguages = listOf("plain", "env", "json", "markdown")

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NoteEditor(title: String, body: String, language: String, enabled: Boolean, onTitleChange: (String) -> Unit, onBodyChange: (String) -> Unit, onLanguageChange: (String) -> Unit) {
    var languageMenu by remember { mutableStateOf(false) }
    Column(verticalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(FilebeamSpace.Small)) {
        OutlinedTextField(title, onTitleChange, enabled = enabled, label = { Text(stringResource(R.string.note_title)) }, modifier = Modifier.fillMaxWidth())
        ProductionGroupCard {
            Row(Modifier.fillMaxWidth().padding(horizontal = FilebeamSpace.Medium, vertical = FilebeamSpace.Small)) {
                Text(stringResource(R.string.note_editor), Modifier.weight(1f), style = androidx.compose.material3.MaterialTheme.typography.titleMedium)
                ExposedDropdownMenuBox(expanded = languageMenu, onExpandedChange = { languageMenu = it }) {
                    OutlinedTextField(language.uppercase(), {}, readOnly = true, enabled = enabled, label = { Text(stringResource(R.string.note_language)) }, trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(languageMenu) }, modifier = Modifier.menuAnchor())
                    androidx.compose.material3.DropdownMenu(expanded = languageMenu, onDismissRequest = { languageMenu = false }) {
                        noteLanguages.forEach { item -> androidx.compose.material3.DropdownMenuItem(text = { Text(item.uppercase()) }, onClick = { onLanguageChange(item); languageMenu = false }) }
                    }
                }
            }
            Row(Modifier.fillMaxWidth().heightIn(min = 180.dp)) {
                Text(lineNumbers(body), Modifier.padding(FilebeamSpace.Small), color = androidx.compose.material3.MaterialTheme.colorScheme.onSurfaceVariant)
                OutlinedTextField(body, onBodyChange, enabled = enabled, label = { Text(stringResource(R.string.note_editor)) }, modifier = Modifier.weight(1f), minLines = 7, visualTransformation = SafeHighlightTransformation(language, androidx.compose.material3.MaterialTheme.colorScheme.primary))
            }
        }
    }
}

private fun lineNumbers(text: String): String = (1..maxOf(1, text.count { it == '\n' } + 1)).joinToString("\n")

/** Presentation-only syntax tinting. It never parses or executes note content. */
private class SafeHighlightTransformation(private val language: String, private val tint: androidx.compose.ui.graphics.Color) : VisualTransformation {
    override fun filter(text: AnnotatedString): TransformedText {
        val styled = AnnotatedString.Builder(text)
        if (language != "plain") Regex("#.*$|//.*$|\\b(true|false|null)\\b", RegexOption.MULTILINE).findAll(text.text).forEach {
            styled.addStyle(SpanStyle(color = tint), it.range.first, it.range.last + 1)
        }
        return TransformedText(styled.toAnnotatedString(), OffsetMapping.Identity)
    }
}
