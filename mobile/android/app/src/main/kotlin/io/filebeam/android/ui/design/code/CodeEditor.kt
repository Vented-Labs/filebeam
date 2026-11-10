package io.filebeam.android.ui.design.code

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.color.EditorColors
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

private fun EditorColors.style(kind: TokenKind): SpanStyle = when (kind) {
    TokenKind.Keyword -> SpanStyle(color = keyword)
    TokenKind.String -> SpanStyle(color = string)
    TokenKind.Number -> SpanStyle(color = number)
    TokenKind.Constant -> SpanStyle(color = constant)
    TokenKind.Function -> SpanStyle(color = function)
    TokenKind.Property -> SpanStyle(color = property)
    TokenKind.Type -> SpanStyle(color = type)
    TokenKind.Tag -> SpanStyle(color = tag)
    TokenKind.Attribute -> SpanStyle(color = attribute)
    TokenKind.Comment -> SpanStyle(color = comment, fontStyle = FontStyle.Italic)
    TokenKind.Punctuation -> SpanStyle(color = punctuation)
    TokenKind.Operator -> SpanStyle(color = operator)
    TokenKind.Meta -> SpanStyle(color = meta)
    TokenKind.Escape -> SpanStyle(color = escape)
    TokenKind.Link -> SpanStyle(color = link)
    TokenKind.Heading -> SpanStyle(color = heading, fontWeight = FontWeight.Bold)
    TokenKind.Invalid -> SpanStyle(color = invalid)
}

private fun annotate(text: String, tokens: List<Token>, colors: EditorColors): AnnotatedString {
    val builder = AnnotatedString.Builder(text)
    for (token in tokens) {
        // Tokens may briefly trail the text while a new pass is running.
        if (token.end <= text.length) builder.addStyle(colors.style(token.kind), token.start, token.end)
    }
    return builder.toAnnotatedString()
}

/** Computes tokens off the main thread, debounced for large documents. */
@Composable
private fun rememberTokens(text: String, language: String): List<Token> {
    var tokens by remember(language) { mutableStateOf(if (text.length <= 20_000) Highlighter.highlight(text, language) else emptyList()) }
    LaunchedEffect(text, language) {
        if (text.length <= 20_000) {
            tokens = Highlighter.highlight(text, language)
        } else {
            delay(120)
            tokens = withContext(Dispatchers.Default) { Highlighter.highlight(text, language) }
        }
    }
    return tokens
}

private val GutterWidth = 44.dp

/** Draws line numbers at the top of each logical line, so wrapped lines keep their number. */
private fun DrawScope.drawGutter(layout: TextLayoutResult?, text: String, measurer: androidx.compose.ui.text.TextMeasurer, style: TextStyle, colors: EditorColors, activeLine: Int, topPadding: Float) {
    drawRect(colors.gutterBackground)
    drawLine(colors.border, Offset(size.width - 0.5f, 0f), Offset(size.width - 0.5f, size.height), 1f)
    val result = layout ?: return
    var logical = 1
    var offset = 0
    while (true) {
        val visual = result.getLineForOffset(offset.coerceAtMost(result.layoutInput.text.length))
        val top = result.getLineTop(visual) + topPadding
        val active = logical - 1 == activeLine
        val measured = measurer.measure(
            logical.toString(), style.copy(color = if (active) colors.gutterActiveText else colors.gutterText, textAlign = TextAlign.End),
            constraints = Constraints.fixedWidth((size.width - 12.dp.toPx()).toInt().coerceAtLeast(1)),
        )
        drawText(measured, topLeft = Offset(0f, top))
        val next = text.indexOf('\n', offset)
        if (next < 0) break
        offset = next + 1
        logical++
    }
}

/**
 * A code surface with a line gutter and syntax colouring.
 * With [onValueChange] it is an editor; without, a selectable read-only viewer.
 * [wrap] switches between soft wrapping and horizontal scrolling.
 */
@Composable
fun CodeEditor(
    value: String,
    language: String,
    modifier: Modifier = Modifier,
    onValueChange: ((String) -> Unit)? = null,
    wrap: Boolean = true,
    placeholder: String? = null,
    minHeight: Dp = 180.dp,
    maxHeight: Dp = 420.dp,
) {
    val colors = Fb.palette.editor
    val style = Fb.type.code.copy(color = colors.text)
    val tokens = rememberTokens(value, language)
    val measurer = rememberTextMeasurer()
    val density = LocalDensity.current
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    var field by remember { mutableStateOf(TextFieldValue(value)) }
    if (field.text != value) field = field.copy(text = value, selection = TextRange(field.selection.start.coerceAtMost(value.length), field.selection.end.coerceAtMost(value.length)))
    val activeLine = if (onValueChange == null) -1 else value.take(field.selection.start.coerceAtMost(value.length)).count { it == '\n' }
    val vertical = rememberScrollState()
    val horizontal = rememberScrollState()
    val padding = 12.dp
    val topPadding = with(density) { padding.toPx() }
    Box(
        modifier
            .fillMaxWidth()
            .heightIn(min = minHeight, max = maxHeight)
            .clip(Shapes.control)
            .background(colors.background)
            .border(1.dp, colors.border, Shapes.control),
    ) {
        Row(Modifier.fillMaxWidth().verticalScroll(vertical).heightIn(min = minHeight).height(IntrinsicSize.Min)) {
            Box(
                Modifier
                    .width(GutterWidth)
                    .fillMaxHeight()
                    .drawBehind { drawGutter(layout, value, measurer, style.copy(fontSize = style.fontSize * 0.86f), colors, activeLine, topPadding) },
            )
            val content = Modifier
                .weight(1f)
                .then(if (wrap) Modifier else Modifier.horizontalScroll(horizontal))
                .padding(padding)
            if (onValueChange != null) {
                val transformation = remember(tokens, colors) {
                    VisualTransformation { text -> TransformedText(annotate(text.text, tokens, colors), OffsetMapping.Identity) }
                }
                BasicTextField(
                    value = field,
                    onValueChange = {
                        field = it
                        if (it.text != value) onValueChange(it.text)
                    },
                    modifier = content.onPreviewKeyEvent { event ->
                        // A hardware Tab indents instead of moving focus out of the editor.
                        if (event.type == KeyEventType.KeyDown && event.key == Key.Tab) {
                            val selection = field.selection
                            val updated = field.text.replaceRange(selection.min, selection.max, "    ")
                            field = TextFieldValue(updated, TextRange(selection.min + 4))
                            onValueChange(updated)
                            true
                        } else false
                    },
                    textStyle = style,
                    cursorBrush = SolidColor(colors.caret),
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
                    visualTransformation = transformation,
                    onTextLayout = { layout = it },
                    decorationBox = { inner ->
                        Box {
                            if (value.isEmpty() && placeholder != null) Text(placeholder, style = style.copy(color = colors.comment))
                            inner()
                        }
                    },
                )
            } else {
                val annotated = remember(value, tokens, colors) { annotate(value, tokens, colors) }
                SelectionContainer(content) {
                    Text(annotated, style = style, softWrap = wrap, onTextLayout = { layout = it })
                }
            }
        }
    }
}
