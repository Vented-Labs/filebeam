<script setup lang="ts">
import { computed, ref } from 'vue';
import Icon from '../primitives/Icon.vue';
import { copyText } from '../../lib/copy-text';
import { onTabKeydown } from './tabs';

const emit = defineEmits<{ notify: [message: string] }>();

type Token = string | { text: string; kind: 'heading' | 'string' | 'property' | 'number' };
const notes: Record<string, { label: string; filename: string; text: string; lines: Token[][] }> = {
    markdown: {
        label: 'Markdown',
        filename: 'readme.md',
        text: '# Project files\n\nFinal exports are in /delivery.\nKeep the originals for the next pass.',
        lines: [
            [{ text: '# Project files', kind: 'heading' }],
            [],
            ['Final exports are in ', { text: '/delivery', kind: 'string' }, '.'],
            ['Keep the originals for the next pass.'],
        ],
    },
    json: {
        label: 'JSON',
        filename: 'config.json',
        text: '{\n  "folder": "delivery",\n  "revision": 4,\n  "keep_originals": true\n}',
        lines: [
            ['{'],
            [
                '  ',
                { text: '"folder"', kind: 'property' },
                ': ',
                { text: '"delivery"', kind: 'string' },
                ',',
            ],
            [
                '  ',
                { text: '"revision"', kind: 'property' },
                ': ',
                { text: '4', kind: 'number' },
                ',',
            ],
            [
                '  ',
                { text: '"keep_originals"', kind: 'property' },
                ': ',
                { text: 'true', kind: 'number' },
            ],
            ['}'],
        ],
    },
    plain: {
        label: 'Plain text',
        filename: 'note.txt',
        text: 'Here are the final exports.\nThe source files are included.\n\nLet me know when they arrive.',
        lines: [
            ['Here are the final exports.'],
            ['The source files are included.'],
            [],
            ['Let me know when they arrive.'],
        ],
    },
};
const language = ref<keyof typeof notes>('markdown');
const mode = ref<'attached' | 'standalone'>('attached');
const note = computed(() => notes[language.value]!);

async function copy(): Promise<void> {
    emit(
        'notify',
        (await copyText(note.value.text)) ? 'Example note copied.' : 'Clipboard unavailable.',
    );
}
</script>

<template>
    <article class="feature-card notes-card" aria-labelledby="notes-title">
        <div class="card-heading">
            <div class="section-icon"><Icon class="icon" name="note" /></div>
            <span class="pill">New</span>
        </div>
        <h2 id="notes-title">Send the context, too.</h2>
        <p>Attach an encrypted note to your files.<br />Or share a note on its own.</p>
        <div class="notes-demo" :data-mode="mode">
            <div class="note-mode-tabs" role="tablist" aria-label="Note sharing mode">
                <button
                    v-for="(label, key) in { attached: 'Attached note', standalone: 'Standalone' }"
                    :id="`note-mode-${key}`"
                    :key="key"
                    type="button"
                    role="tab"
                    aria-controls="note-panel"
                    :aria-selected="mode === key"
                    :tabindex="mode === key ? 0 : -1"
                    @click="mode = key"
                    @keydown="onTabKeydown"
                >
                    {{ label }}
                </button>
            </div>
            <div v-if="mode === 'attached'" class="attached-file">
                <Icon class="icon" name="archive" />
                <div>project-files.zip<span>14 MB</span></div>
                <Icon class="icon" name="check" />
            </div>
            <div
                id="note-panel"
                class="note-editor"
                role="tabpanel"
                :aria-labelledby="`note-mode-${mode}`"
            >
                <div class="editor-toolbar">
                    <span
                        ><Icon class="icon" name="note" /> <span>{{ note.filename }}</span></span
                    >
                    <label
                        ><span class="sr-only">Example note language</span
                        ><select v-model="language">
                            <option v-for="(item, key) in notes" :key="key" :value="key">
                                {{ item.label }}
                            </option>
                        </select></label
                    >
                    <button
                        type="button"
                        class="icon-button small"
                        aria-label="Copy example note"
                        @click="copy"
                    >
                        <Icon class="icon" name="copy" />
                    </button>
                </div>
                <div class="code-content">
                    <div v-for="(line, index) in note.lines" :key="index" class="code-line">
                        <span class="line-number" aria-hidden="true">{{ index + 1 }}</span
                        ><span
                            ><template v-for="(token, part) in line" :key="part"
                                ><span
                                    v-if="typeof token !== 'string'"
                                    :class="`syntax-${token.kind}`"
                                    >{{ token.text }}</span
                                ><template v-else>{{ token }}</template></template
                            ><template v-if="!line.length">&nbsp;</template></span
                        >
                    </div>
                </div>
                <div class="editor-status">
                    <Icon class="icon" name="lock" />
                    <span>{{
                        mode === 'attached'
                            ? 'Same encryption. Same expiry as the files.'
                            : 'Encrypted notes, with optional burn-on-read.'
                    }}</span>
                </div>
            </div>
        </div>
    </article>
</template>
