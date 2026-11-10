package io.filebeam.android.ui.feature.about

import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import io.filebeam.android.BuildConfig
import io.filebeam.android.R
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbSheet
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.FilebeamMark
import io.filebeam.android.ui.design.Hairline
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.Segment
import io.filebeam.android.ui.design.SegmentedRail
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.design.code.CodeEditor
import io.filebeam.android.ui.feature.account.LinkRow
import io.filebeam.android.ui.feature.send.CopyField
import io.filebeam.android.ui.feature.send.copySensitive
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.Route
import io.filebeam.android.ui.shell.SectionLabel
import io.filebeam.android.ui.shell.Staggered
import io.filebeam.android.ui.shell.SubScreen
import io.filebeam.android.ui.shell.collectAsStateCompat

private enum class Info { About, Privacy, Help, Cli, Licenses }

private const val CLI_UNIX = "curl -fsSL 'https://releases.filebeam.io/cli/install.sh' | sh"
private const val CLI_WINDOWS = "irm 'https://releases.filebeam.io/cli/install.ps1' | iex"

/** What Filebeam is, the connected instance, where to get help, and the open-source notices. */
@Composable
fun AboutScreen() {
    val app = LocalAppModel.current
    val navigator = LocalNavigator.current
    val context = LocalContext.current
    val profile by app.instance.collectAsStateCompat()
    val palette = Fb.palette
    var open by rememberSaveable { mutableStateOf<Info?>(null) }
    val openLink: (String) -> Unit = { url ->
        runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, url.toUri())) }
            .onFailure { app.toast(context.getString(R.string.about_no_browser), Tone.Danger) }
    }

    SubScreen(stringResource(R.string.about_title)) {
        item {
            Staggered(0) {
                Panel(Modifier.fillMaxWidth()) {
                    Row(Modifier.padding(Space.lg), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.lg)) {
                        FilebeamMark(height = 52.dp)
                        Column(Modifier.weight(1f)) {
                            FbText(stringResource(R.string.app_name), style = Fb.type.headline)
                            FbText(stringResource(R.string.about_tagline), style = Fb.type.small, color = palette.textMuted)
                            FbText(stringResource(R.string.about_app_version, BuildConfig.VERSION_NAME), style = Fb.type.caption, color = palette.textSubtle)
                        }
                    }
                }
            }
        }
        item { SectionLabel(stringResource(R.string.about_learn)) }
        item {
            Staggered(1) {
                Panel(Modifier.fillMaxWidth()) {
                    LinkRow(R.drawable.ic_approved_bolt, stringResource(R.string.tour_title), { navigator.push(Route.Tour) })
                    if (profile.changelogEnabled) {
                        Hairline()
                        LinkRow(R.drawable.ic_approved_updates, stringResource(R.string.changelog_title), { navigator.push(Route.Changelog) }, detail = profile.version?.let { "v$it" })
                    }
                    Hairline()
                    LinkRow(R.drawable.ic_approved_info, stringResource(R.string.about_title), { open = Info.About })
                    Hairline()
                    LinkRow(R.drawable.ic_approved_shield, stringResource(R.string.about_privacy), { open = Info.Privacy })
                    Hairline()
                    LinkRow(R.drawable.ic_approved_help, stringResource(R.string.about_help), { open = Info.Help })
                    Hairline()
                    LinkRow(R.drawable.ic_approved_terminal, stringResource(R.string.about_cli), { open = Info.Cli })
                }
            }
        }
        item { SectionLabel(stringResource(R.string.about_instance)) }
        item {
            Staggered(2) {
                Panel(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
                        FbText(profile.name, style = Fb.type.bodyStrong)
                        FbText(profile.origin, style = Fb.type.codeSmall, color = palette.textMuted)
                        profile.version?.let { FbText(stringResource(R.string.about_server_version, it), style = Fb.type.caption, color = palette.textSubtle) }
                        profile.copyright?.takeIf(String::isNotBlank)?.let { FbText("© $it", style = Fb.type.caption, color = palette.textSubtle) }
                    }
                    profile.githubUrl?.let { url ->
                        Hairline()
                        LinkRow(R.drawable.ic_approved_code, stringResource(R.string.about_source), { openLink(url) })
                    }
                    profile.communityLinks.forEach { (name, url) ->
                        Hairline()
                        LinkRow(R.drawable.ic_approved_globe, name.replaceFirstChar(Char::uppercase), { openLink(url) })
                    }
                }
            }
        }
        item {
            Staggered(3) {
                Panel(Modifier.fillMaxWidth()) { LinkRow(R.drawable.ic_approved_book, stringResource(R.string.about_licenses), { open = Info.Licenses }) }
            }
        }
    }

    when (open) {
        Info.About -> InfoSheet(stringResource(R.string.about_title), stringResource(R.string.about_about_body)) { open = null }
        Info.Privacy -> InfoSheet(stringResource(R.string.about_privacy), stringResource(R.string.about_privacy_body)) { open = null }
        Info.Help -> InfoSheet(stringResource(R.string.about_help), stringResource(R.string.about_help_body)) { open = null }
        Info.Cli -> {
            var platform by remember { mutableStateOf("unix") }
            FbSheet({ open = null }, title = stringResource(R.string.about_cli), subtitle = stringResource(R.string.about_cli_body)) {
                SegmentedRail(listOf(Segment("unix", stringResource(R.string.about_cli_unix)), Segment("windows", stringResource(R.string.about_cli_windows))), platform, { platform = it })
                val command = if (platform == "unix") CLI_UNIX else CLI_WINDOWS
                CopyField(R.drawable.ic_approved_terminal, command, stringResource(R.string.copy), primary = true) {
                    copySensitive(context, context.getString(R.string.about_cli), command)
                    app.toast(context.getString(R.string.cli_copied), Tone.Success)
                }
                FbText(stringResource(R.string.about_cli_usage), style = Fb.type.small, color = palette.textMuted)
                CodeEditor("beam up ./file.zip\nbeam down <url or id>\nbeam", "plain", minHeight = 96.dp)
            }
        }
        Info.Licenses -> {
            val notices = remember {
                listOf("LICENSE", "icons-NOTICE", "fonts-Inter-OFL.txt", "fonts-JetBrainsMono-OFL.txt").mapNotNull { name ->
                    runCatching { name to context.assets.open(name).bufferedReader().use { it.readText() } }.getOrNull()
                }
            }
            FbSheet({ open = null }, title = stringResource(R.string.about_licenses)) {
                notices.forEach { (name, text) ->
                    FbText(name, style = Fb.type.bodyStrong)
                    FbText(text, style = Fb.type.codeSmall, color = palette.textMuted)
                }
            }
        }
        null -> Unit
    }
}

@Composable
private fun InfoSheet(title: String, body: String, onDismiss: () -> Unit) {
    FbSheet(onDismiss, title = title) {
        FbText(body, style = Fb.type.body, color = Fb.palette.textMuted)
        FbButton(stringResource(R.string.done), onDismiss, Modifier.fillMaxWidth())
    }
}
