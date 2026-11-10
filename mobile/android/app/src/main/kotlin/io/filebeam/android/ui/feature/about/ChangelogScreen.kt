package io.filebeam.android.ui.feature.about

import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import io.filebeam.android.R
import io.filebeam.android.ui.common.formatAbsolute
import io.filebeam.android.ui.common.parseInstant
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.EmptyState
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.FbTextField
import io.filebeam.android.ui.design.FilterChip
import io.filebeam.android.ui.design.Hairline
import io.filebeam.android.ui.design.IconAction
import io.filebeam.android.ui.design.JetBrainsMono
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Spinner
import io.filebeam.android.ui.design.StatusPill
import io.filebeam.android.ui.design.SwitchRow
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.feature.send.copySensitive
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.Route
import io.filebeam.android.ui.shell.SubScreen
import io.filebeam.android.ui.shell.collectAsStateCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

private data class ReleaseSummary(val tag: String, val title: String, val description: String, val publishedAt: String?, val prerelease: Boolean, val latest: Boolean, val featured: Boolean)

private fun JSONObject.summary() = ReleaseSummary(
    getString("tag"), optString("title").ifBlank { getString("tag") }, optString("description"),
    optString("published_at").takeIf(String::isNotBlank), optBoolean("prerelease"), optBoolean("latest"), optBoolean("featured"),
)

/** Release notes mirrored by the instance: searchable, paged, with pre-releases on request. */
@Composable
fun ChangelogScreen() {
    val app = LocalAppModel.current
    val navigator = LocalNavigator.current
    val settings by app.settings.collectAsStateCompat()
    var query by rememberSaveable { mutableStateOf("") }
    var prereleases by rememberSaveable { mutableStateOf(false) }
    var page by rememberSaveable { mutableIntStateOf(1) }
    var lastPage by remember { mutableIntStateOf(1) }
    var releases by remember { mutableStateOf<List<ReleaseSummary>?>(null) }
    var versions by remember { mutableStateOf<List<Pair<String, Boolean>>>(emptyList()) }
    var hasPrereleases by remember { mutableStateOf(false) }
    var archive by remember { mutableStateOf("ready") }
    var stale by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val generic = stringResource(R.string.changelog_unavailable_body)

    LaunchedEffect(query, prereleases, page, settings.instance) {
        // Typing settles before a search goes out, as on the web.
        if (query.isNotEmpty()) delay(250)
        error = null
        runCatching {
            withContext(Dispatchers.IO) { JSONObject(app.app.accountSessions.service(settings.instance).changelogJson(page.toUInt(), query.takeIf(String::isNotBlank), prereleases)) }
        }.onSuccess { data ->
            val listing = data.getJSONObject("releases")
            val rows = listing.getJSONArray("data")
            releases = (0 until rows.length()).map { rows.getJSONObject(it).summary() }
            lastPage = listing.optInt("last_page", 1)
            versions = data.optJSONArray("versions")?.let { array -> (0 until array.length()).map { array.getJSONObject(it).let { v -> v.getString("tag") to v.optBoolean("prerelease") } } } ?: emptyList()
            hasPrereleases = data.optBoolean("has_prereleases")
            archive = data.optJSONObject("archive")?.optString("state") ?: "ready"
            stale = data.optJSONObject("archive")?.optBoolean("stale") ?: false
        }.onFailure {
            releases = emptyList()
            error = it.message ?: generic
        }
    }

    SubScreen(stringResource(R.string.changelog_title)) {
        item {
            FbTextField(query, {
                query = it.take(120)
                page = 1
            }, placeholder = stringResource(R.string.changelog_search), leadingIcon = R.drawable.ic_approved_search)
        }
        if (hasPrereleases) {
            item {
                Panel(Modifier.fillMaxWidth()) {
                    SwitchRow(stringResource(R.string.changelog_prereleases), prereleases, {
                        prereleases = it
                        page = 1
                    }, Modifier.padding(horizontal = Space.lg))
                }
            }
        }
        if (versions.size > 1 && query.isBlank()) {
            item {
                Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                    FbText(stringResource(R.string.changelog_jump).uppercase(), style = Fb.type.eyebrow, color = Fb.palette.textMuted)
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                        versions.take(40).forEach { (tag, _) -> FilterChip(tag, false, { navigator.push(Route.Release(tag)) }) }
                    }
                }
            }
        }
        if (stale) item { Notice(stringResource(R.string.changelog_stale), tone = Tone.Warning) }
        val loaded = releases
        when {
            loaded == null -> item { Box(Modifier.fillMaxWidth().padding(Space.xxxl), contentAlignment = Alignment.Center) { Spinner() } }
            error != null || archive == "unavailable" -> item { EmptyState(R.drawable.ic_approved_alert, stringResource(R.string.changelog_unavailable_title), body = error ?: generic, tone = Tone.Warning) }
            loaded.isEmpty() -> item { EmptyState(R.drawable.ic_approved_search, stringResource(if (query.isBlank()) R.string.changelog_empty else R.string.changelog_no_match), tone = Tone.Neutral) }
            else -> {
                items(loaded, key = { it.tag }) { release ->
                    Panel(
                        Modifier.fillMaxWidth().animateItem(fadeInSpec = Motion.enter(), placementSpec = Motion.spatial()).clip(Shapes.panel).clickable(role = Role.Button) { navigator.push(Route.Release(release.tag)) },
                        border = if (release.featured) Fb.palette.choiceBorder else Fb.palette.border,
                    ) {
                        Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                                FbText(release.tag, style = Fb.type.codeSmall, color = Fb.palette.accentText)
                                if (release.latest) StatusPill(stringResource(R.string.changelog_latest), tone = Tone.Success)
                                if (release.prerelease) StatusPill(stringResource(R.string.changelog_prerelease), tone = Tone.Warning)
                            }
                            FbText(release.title, style = if (release.featured) Fb.type.headline else Fb.type.title)
                            if (release.description.isNotBlank()) FbText(release.description, style = Fb.type.small, color = Fb.palette.textMuted, maxLines = 3)
                            parseInstant(release.publishedAt)?.let { FbText(formatAbsolute(it), style = Fb.type.caption, color = Fb.palette.textSubtle) }
                        }
                    }
                }
                if (lastPage > 1) {
                    item {
                        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.md), verticalAlignment = Alignment.CenterVertically) {
                            FbButton(stringResource(R.string.changelog_newer), { page-- }, Modifier.weight(1f), style = ButtonStyle.Secondary, enabled = page > 1)
                            FbText("$page / $lastPage", style = Fb.type.numeric, color = Fb.palette.textMuted)
                            FbButton(stringResource(R.string.changelog_older), { page++ }, Modifier.weight(1f), style = ButtonStyle.Secondary, enabled = page < lastPage)
                        }
                    }
                }
            }
        }
    }
}

/** One release, rendered natively from the instance's sanitised node tree. */
@Composable
fun ReleaseScreen(tag: String) {
    val app = LocalAppModel.current
    val navigator = LocalNavigator.current
    val context = LocalContext.current
    val resources = LocalResources.current
    val settings by app.settings.collectAsStateCompat()
    var data by remember(tag) { mutableStateOf<JSONObject?>(null) }
    var failed by remember(tag) { mutableStateOf(false) }

    LaunchedEffect(tag, settings.instance) {
        runCatching { withContext(Dispatchers.IO) { JSONObject(app.app.accountSessions.service(settings.instance).changelogReleaseJson(tag)) } }
            .onSuccess { data = it }
            .onFailure { failed = true }
    }
    val release = data?.optJSONObject("release")
    val webUrl = release?.optString("html_url")?.takeIf(String::isNotBlank)
    SubScreen(
        tag,
        actions = {
            if (release != null) {
                IconAction(R.drawable.ic_approved_link, stringResource(R.string.changelog_copy_link), {
                    copySensitive(context, tag, "${settings.instance}/changelog/$tag")
                    app.toast(resources.getString(R.string.copied), Tone.Success)
                })
            }
        },
    ) {
        when {
            failed -> item { EmptyState(R.drawable.ic_approved_alert, stringResource(R.string.changelog_unavailable_title), tone = Tone.Warning) }
            release == null -> item { Box(Modifier.fillMaxWidth().padding(Space.xxxl), contentAlignment = Alignment.Center) { Spinner() } }
            else -> {
                item {
                    Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                        Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                            if (release.optBoolean("latest")) StatusPill(stringResource(R.string.changelog_latest), tone = Tone.Success)
                            if (release.optBoolean("prerelease")) StatusPill(stringResource(R.string.changelog_prerelease), tone = Tone.Warning)
                        }
                        FbText(release.optString("title").ifBlank { tag }, Modifier.semantics { heading() }, style = Fb.type.display)
                        val meta = listOfNotNull(parseInstant(release.optString("published_at"))?.let(::formatAbsolute), release.optString("author").takeIf { it.isNotBlank() && it != "null" }?.let { "@$it" })
                        if (meta.isNotEmpty()) FbText(meta.joinToString(" · "), style = Fb.type.small, color = Fb.palette.textMuted)
                    }
                }
                val blocks = release.optJSONObject("document")?.optJSONArray("blocks") ?: JSONArray()
                val headings = buildList { for (i in 0 until blocks.length()) collectHeadings(blocks.getJSONObject(i).optJSONArray("nodes"), this) }
                if (headings.size > 2) {
                    item {
                        Panel(Modifier.fillMaxWidth()) {
                            Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
                                FbText(stringResource(R.string.changelog_contents).uppercase(), style = Fb.type.eyebrow, color = Fb.palette.textMuted)
                                headings.forEach { FbText(it, style = Fb.type.small, color = Fb.palette.accentText, maxLines = 1) }
                            }
                        }
                    }
                }
                for (index in 0 until blocks.length()) {
                    val block = blocks.getJSONObject(index)
                    if (block.optString("type") == "content") {
                        val nodes = block.optJSONArray("nodes") ?: continue
                        item(key = block.optString("id", "block-$index")) { Column(verticalArrangement = Arrangement.spacedBy(Space.md)) { Nodes(nodes) } }
                    }
                }
                item { Hairline() }
                item {
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                        val newer = data?.optJSONObject("newer")?.optString("tag")?.takeIf(String::isNotBlank)
                        val older = data?.optJSONObject("older")?.optString("tag")?.takeIf(String::isNotBlank)
                        FbButton(newer ?: stringResource(R.string.changelog_newer), { newer?.let { navigator.replace(Route.Release(it)) } }, Modifier.weight(1f), style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_arrow_left, enabled = newer != null)
                        FbButton(older ?: stringResource(R.string.changelog_older), { older?.let { navigator.replace(Route.Release(it)) } }, Modifier.weight(1f), style = ButtonStyle.Secondary, trailingIcon = R.drawable.ic_approved_arrow_right, enabled = older != null)
                    }
                }
                if (webUrl != null) {
                    item {
                        FbButton(stringResource(R.string.changelog_view_source), {
                            runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, webUrl.toUri())) }
                        }, Modifier.fillMaxWidth(), style = ButtonStyle.Quiet, trailingIcon = R.drawable.ic_approved_arrow_up_right)
                    }
                }
            }
        }
    }
}

private val blockTags = setOf("section", "div", "p", "blockquote", "pre", "hr", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "table", "img")

private fun collectHeadings(nodes: JSONArray?, into: MutableList<String>) {
    if (nodes == null) return
    for (i in 0 until nodes.length()) {
        val node = nodes.optJSONObject(i) ?: continue
        if (node.optString("t") in setOf("h2", "h3")) into += plainText(node.optJSONArray("c")).trim() else collectHeadings(node.optJSONArray("c"), into)
    }
}

private fun plainText(nodes: JSONArray?): String {
    if (nodes == null) return ""
    val out = StringBuilder()
    for (i in 0 until nodes.length()) {
        when (val node = nodes.get(i)) {
            is String -> out.append(node)
            is JSONObject -> if (node.optString("t") == "br") out.append('\n') else out.append(plainText(node.optJSONArray("c")))
        }
    }
    return out.toString()
}

/** Renders a list of nodes: block elements stack, runs of inline content flow as one paragraph. */
@Composable
private fun Nodes(nodes: JSONArray) {
    var inline = JSONArray()
    val groups = mutableListOf<Any>()
    for (i in 0 until nodes.length()) {
        val node = nodes.get(i)
        if (node is JSONObject && node.optString("t") in blockTags) {
            if (inline.length() > 0) {
                groups += inline
                inline = JSONArray()
            }
            groups += node
        } else if (!(node is String && node.isBlank() && inline.length() == 0)) inline.put(node)
    }
    if (inline.length() > 0) groups += inline
    groups.forEach { group ->
        when (group) {
            is JSONArray -> Paragraph(group, Fb.type.body)
            is JSONObject -> Block(group)
        }
    }
}

@Composable
private fun Paragraph(nodes: JSONArray, style: androidx.compose.ui.text.TextStyle, modifier: Modifier = Modifier) {
    val palette = Fb.palette
    val links = TextLinkStyles(SpanStyle(color = palette.accentText, textDecoration = TextDecoration.Underline))
    val code = SpanStyle(fontFamily = JetBrainsMono, background = palette.hover, fontSize = style.fontSize * 0.9f)
    val text = remember(nodes, palette) { AnnotatedString.Builder().apply { appendInline(nodes, links, code) }.toAnnotatedString() }
    if (text.text.isNotBlank()) Text(text, modifier, style = style, color = palette.text)
}

private fun AnnotatedString.Builder.appendInline(nodes: JSONArray?, links: TextLinkStyles, code: SpanStyle) {
    if (nodes == null) return
    for (i in 0 until nodes.length()) {
        when (val node = nodes.get(i)) {
            is String -> append(node)
            is JSONObject -> {
                val children = node.optJSONArray("c")
                val attributes = node.optJSONObject("a")
                when (node.optString("t")) {
                    "br" -> append('\n')
                    "strong" -> withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { appendInline(children, links, code) }
                    "em" -> withStyle(SpanStyle(fontStyle = FontStyle.Italic)) { appendInline(children, links, code) }
                    "del" -> withStyle(SpanStyle(textDecoration = TextDecoration.LineThrough)) { appendInline(children, links, code) }
                    "code" -> withStyle(code) { appendInline(children, links, code) }
                    "a" -> {
                        val href = attributes?.optString("href").orEmpty()
                        // Only web links open; anything else renders as plain text.
                        if (href.startsWith("https://") || href.startsWith("http://")) withLink(LinkAnnotation.Url(href, links)) { appendInline(children, links, code) }
                        else appendInline(children, links, code)
                    }
                    "input" -> append(if (attributes?.has("checked") == true) "☑ " else "☐ ")
                    "img" -> append(attributes?.optString("alt").orEmpty())
                    else -> appendInline(children, links, code)
                }
            }
        }
    }
}

@Composable
private fun Block(node: JSONObject) {
    val palette = Fb.palette
    val children = node.optJSONArray("c") ?: JSONArray()
    when (val tag = node.optString("t")) {
        "h1", "h2" -> Paragraph(children, Fb.type.headline, Modifier.padding(top = Space.md).semantics { heading() })
        "h3" -> Paragraph(children, Fb.type.title, Modifier.padding(top = Space.sm).semantics { heading() })
        "h4", "h5", "h6" -> Paragraph(children, Fb.type.subtitle, Modifier.semantics { heading() })
        "p" -> Paragraph(children, Fb.type.body)
        "hr" -> Hairline()
        "pre" -> Box(Modifier.fillMaxWidth().clip(Shapes.control).background(palette.editor.background).border(1.dp, palette.editor.border, Shapes.control).horizontalScroll(rememberScrollState()).padding(Space.md)) {
            FbText(plainText(children).trimEnd(), style = Fb.type.codeSmall, color = palette.editor.text, maxLines = Int.MAX_VALUE)
        }
        "blockquote" -> Row(Modifier.height(androidx.compose.foundation.layout.IntrinsicSize.Min)) {
            Box(Modifier.width(3.dp).fillMaxHeight().clip(Shapes.pill).background(palette.choiceBorder))
            Column(Modifier.padding(start = Space.md), verticalArrangement = Arrangement.spacedBy(Space.sm)) { Nodes(children) }
        }
        "ul", "ol" -> Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            var number = node.optJSONObject("a")?.optString("start")?.toIntOrNull() ?: 1
            for (i in 0 until children.length()) {
                val item = children.optJSONObject(i) ?: continue
                if (item.optString("t") != "li") continue
                Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                    FbText(if (tag == "ol") "${number++}." else "•", style = Fb.type.body, color = palette.accentText)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Space.xs)) { Nodes(item.optJSONArray("c") ?: JSONArray()) }
                }
            }
        }
        "table" -> Column(Modifier.fillMaxWidth().clip(Shapes.control).border(1.dp, palette.border, Shapes.control)) {
            val rows = buildList { collectRows(children, this) }
            rows.forEachIndexed { index, row ->
                if (index > 0) Hairline()
                Row(Modifier.fillMaxWidth().background(if (index == 0) palette.sunken else androidx.compose.ui.graphics.Color.Transparent).padding(Space.md), horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                    row.forEach { cell -> FbText(cell, Modifier.weight(1f), style = if (index == 0) Fb.type.smallStrong else Fb.type.small) }
                }
            }
        }
        "img" -> node.optJSONObject("a")?.optString("alt")?.takeIf(String::isNotBlank)?.let { FbText(it, style = Fb.type.small, color = palette.textMuted) }
        else -> Column(verticalArrangement = Arrangement.spacedBy(Space.md)) { Nodes(children) }
    }
}

private fun collectRows(nodes: JSONArray, into: MutableList<List<String>>) {
    for (i in 0 until nodes.length()) {
        val node = nodes.optJSONObject(i) ?: continue
        val children = node.optJSONArray("c") ?: continue
        if (node.optString("t") == "tr") {
            into += (0 until children.length()).mapNotNull { children.optJSONObject(it) }.map { plainText(it.optJSONArray("c")).trim() }
        } else collectRows(children, into)
    }
}
