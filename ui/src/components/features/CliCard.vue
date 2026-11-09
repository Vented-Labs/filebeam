<script setup lang="ts">
import { computed, ref } from 'vue';
import Icon from '../primitives/Icon.vue';
import { copyText } from '../../lib/copy-text';
import { onTabKeydown } from './tabs';

const props = defineProps<{ host: string; docsUrl: string }>();
const emit = defineEmits<{ notify: [message: string] }>();

const commands = {
    send: {
        label: 'Send',
        command: 'beam up field-notes.zip',
        args: 'up',
        target: 'field-notes.zip',
        description: 'Upload an encrypted transfer and get a share link.',
    },
    receive: {
        label: 'Receive',
        command: 'beam down <url>',
        args: 'down',
        target: '<url>',
        description: 'Download and decrypt from a Filebeam share link.',
    },
    tui: { label: 'TUI', command: 'beam', args: '', target: '', description: '' },
} as const;
type Mode = keyof typeof commands;
const mode = ref<Mode>('send');
const current = computed(() => commands[mode.value]);
// Rows of the terminal UI's mark, matching crates/cli/src/presentation.rs.
const tuiMark = [
    '        ▄▄██',
    '    ▄▄████▀▀',
    '▄▄████▀▀    ',
    '████▄▄      ',
    '██▀▀████▄   ',
    '██▄   ▀▀    ',
    '▀███▄       ',
];
const tuiRows = [
    ['DIR', 'backend', '-'],
    ['DIR', 'crates', '-'],
    ['DIR', 'desktop', '-'],
    ['DIR', 'docs', '-'],
    ['DIR', 'icons', '-'],
    ['TXT', 'AGENTS.md', '1.8 KiB'],
    ['TXT', 'CONTRIBUTING.md', '9.7 KiB'],
    ['FILE', 'LICENSE', '1.0 KiB'],
];

async function copy(): Promise<void> {
    emit(
        'notify',
        (await copyText(current.value.command)) ? 'Command copied.' : 'Clipboard unavailable.',
    );
}
</script>

<template>
    <article class="terminal-feature feature-card" aria-labelledby="cli-title">
        <div class="section-icon"><Icon class="icon" name="terminal" /></div>
        <h2 id="cli-title">A proper CLI.</h2>
        <p>Send, receive, or open the interactive terminal UI.</p>
        <div class="cli-demo" :data-mode="mode">
            <div class="cli-toolbar">
                <span>beam</span>
                <div role="tablist" class="small-tabs" aria-label="Example CLI command">
                    <button
                        v-for="(item, key) in commands"
                        :id="`cli-tab-${key}`"
                        :key="key"
                        type="button"
                        role="tab"
                        aria-controls="cli-example"
                        :aria-selected="mode === key"
                        :tabindex="mode === key ? 0 : -1"
                        @click="mode = key"
                        @keydown="onTabKeydown"
                    >
                        {{ item.label }}
                    </button>
                </div>
                <button
                    type="button"
                    class="icon-button small"
                    :aria-label="`Copy ${current.command}`"
                    @click="copy"
                >
                    <Icon class="icon" name="copy" />
                </button>
            </div>
            <div
                id="cli-example"
                class="cli-example"
                role="tabpanel"
                :aria-labelledby="`cli-tab-${mode}`"
            >
                <div v-if="mode !== 'tui'" class="cli-text-mode">
                    <div class="command-line">
                        <span class="prompt" aria-hidden="true">$</span
                        ><span class="verb">beam</span> {{ current.args }}
                        <span class="muted">{{ current.target }}</span>
                    </div>
                    <p>{{ current.description }}</p>
                </div>
                <!-- An illustration of the real terminal UI layout, not a second implementation. -->
                <div
                    v-else
                    class="tui-mock"
                    role="img"
                    aria-label="The Filebeam terminal UI with a file browser and a transfer pane"
                >
                    <div class="tui-topbar" aria-hidden="true">
                        <div class="tui-brand">
                            <pre class="tui-mark"><span
                                    v-for="(row, index) in tuiMark"
                                    :key="index"
                                    :class="{ 'tui-mark__fold': index === 3 }"
                                    >{{ row }}{{ '\n' }}</span
                                ></pre>
                            <div class="tui-brand-copy">
                                <strong>Filebeam</strong><span>Send and receive files</span
                                ><small>Encrypted on your device.</small>
                            </div>
                        </div>
                        <div class="tui-instance">
                            <span class="dot" />Filebeam&nbsp;&nbsp;{{ props.host }}
                        </div>
                    </div>
                    <div class="tui-tabs" aria-hidden="true">
                        <b>1 Send</b><span>2 Receive</span><span>3 Transfers</span>
                    </div>
                    <div class="tui-panels" aria-hidden="true">
                        <div class="tui-panel">
                            <div class="tui-panel-title">Choose what to share</div>
                            <div class="tui-path">~/projects/field-notes</div>
                            <div class="tui-search">/ Search this folder</div>
                            <div class="tui-list">
                                <div
                                    v-for="(row, index) in tuiRows"
                                    :key="row[1]"
                                    class="tui-row"
                                    :class="{ active: index === 0 }"
                                >
                                    <span class="tui-type">{{ row[0] }}</span
                                    ><span>{{ row[1] }}</span
                                    ><span class="tui-size">{{ row[2] }}</span>
                                </div>
                            </div>
                        </div>
                        <div class="tui-panel">
                            <div class="tui-panel-title">Your transfer</div>
                            <div class="tui-meta">0 files · 0 B</div>
                            <div class="tui-empty">
                                <strong>No files selected</strong>
                                <div>Select files with Space</div>
                                <div>Tab to review your selection</div>
                            </div>
                            <div class="tui-transfer-footer">24h retention</div>
                            <div class="tui-actions">
                                <span>Send Encrypted ↵</span
                                ><span>Turbo Transfer&nbsp;&nbsp;Shift+↵</span>
                            </div>
                            <div class="tui-helper">Share the link with your recipient.</div>
                        </div>
                    </div>
                    <div class="tui-shortcuts" aria-hidden="true">
                        <b>Space</b> select <b>Enter</b> open/select <b>Shift+Enter</b> Turbo
                        transfer <b>Tab</b> focus <b>/</b> search <b>p</b> more actions
                        <b>?</b> help
                    </div>
                </div>
            </div>
        </div>
        <div class="platform-line">
            Linux <span>·</span> macOS <span>·</span> Windows
            <a :href="docsUrl" target="_blank" rel="noopener noreferrer"
                >CLI docs <Icon class="icon" name="arrow-up-right"
            /></a>
        </div>
    </article>
</template>
