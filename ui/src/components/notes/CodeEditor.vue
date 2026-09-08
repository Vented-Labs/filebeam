<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import type { Compartment, EditorState, Extension } from '@codemirror/state';
import type { StringStream } from '@codemirror/language';
import type { EditorView, ViewUpdate } from '@codemirror/view';
import { normalizeNoteLanguage } from '../../lib/note-languages';

const props = withDefaults(
    defineProps<{
        modelValue: string;
        language: string;
        readOnly?: boolean;
        wrap?: boolean;
    }>(),
    { readOnly: false, wrap: true },
);
const emit = defineEmits<{ 'update:modelValue': [value: string] }>();

const editorHost = ref<HTMLElement>();
const fallback = ref(props.modelValue);
const ready = ref(false);
let view: EditorView | undefined;
let languageCompartment: Compartment | undefined;
let readOnlyCompartment: Compartment | undefined;
let wrapCompartment: Compartment | undefined;
let readOnlyExtension: ((readOnly: boolean) => Extension) | undefined;
let lineWrappingExtension: Extension | undefined;
let destroyed = false;
const expensiveFeatureLimit = 250_000;

async function languageExtension(language: string): Promise<Extension> {
    switch (normalizeNoteLanguage(language)) {
        case 'php':
            return (await import('@codemirror/lang-php')).php();
        case 'javascript':
            return (await import('@codemirror/lang-javascript')).javascript();
        case 'typescript':
            return (await import('@codemirror/lang-javascript')).javascript({ typescript: true });
        case 'json':
            return (await import('@codemirror/lang-json')).json();
        case 'markdown':
            return (await import('@codemirror/lang-markdown')).markdown();
        case 'css':
            return (await import('@codemirror/lang-css')).css();
        case 'html':
            return (await import('@codemirror/lang-html')).html();
        case 'dotenv': {
            const { StreamLanguage } = await import('@codemirror/language');
            return StreamLanguage.define({
                startState: () => null,
                token(stream: StringStream) {
                    if (stream.sol() && stream.match(/^\s*#/)) {
                        stream.skipToEnd();
                        return 'comment';
                    }
                    if (stream.match(/^\s*(?:export\s+)?[A-Za-z_][\w.-]*(?=\s*=)/))
                        return 'keyword';
                    if (stream.match(/^\s*=\s*/)) return null;
                    if (stream.match(/^\s*(?:"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')/))
                        return 'string';
                    if (stream.match(/^\s*[^#\n]+/)) return 'string';
                    stream.next();
                    return null;
                },
            });
        }
        default:
            return [];
    }
}

async function safeLanguageExtension(language: string): Promise<Extension> {
    try {
        return await languageExtension(language);
    } catch {
        return [];
    }
}

async function mountEditor(): Promise<void> {
    if (!editorHost.value || typeof window === 'undefined') return;
    try {
        const [
            { basicSetup, EditorView },
            { EditorState, Compartment },
            { HighlightStyle, syntaxHighlighting },
            { tags },
            { indentWithTab },
            { keymap },
        ] = await Promise.all([
            import('codemirror'),
            import('@codemirror/state'),
            import('@codemirror/language'),
            import('@lezer/highlight'),
            import('@codemirror/commands'),
            import('@codemirror/view'),
        ]);
        if (destroyed || !editorHost.value) return;
        languageCompartment = new Compartment();
        readOnlyCompartment = new Compartment();
        wrapCompartment = new Compartment();
        lineWrappingExtension = EditorView.lineWrapping;
        readOnlyExtension = (readOnly: boolean) => [
            EditorState.readOnly.of(readOnly),
            EditorView.editable.of(!readOnly),
        ];
        let initialModel = '';
        let initialLanguage = 'plain';
        let initialLanguageExtension: Extension = [];
        do {
            initialModel = props.modelValue;
            initialLanguage = props.language;
            initialLanguageExtension =
                initialModel.length <= expensiveFeatureLimit
                    ? await safeLanguageExtension(initialLanguage)
                    : [];
        } while (
            !destroyed &&
            (props.modelValue !== initialModel || props.language !== initialLanguage)
        );
        if (destroyed || !editorHost.value) return;
        const extensions = [
            basicSetup,
            keymap.of([indentWithTab]),
            EditorView.theme(
                {
                    '&': {
                        height: '100%',
                        backgroundColor: 'var(--fb-editor-bg)',
                        color: 'var(--fb-text)',
                        fontFamily: 'var(--fb-font-code)',
                        fontSize: 'var(--fb-font-code-size)',
                        fontVariantLigatures: 'none',
                        fontFeatureSettings: '"liga" 0, "calt" 0',
                    },
                    '.cm-scroller': {
                        fontFamily: 'var(--fb-font-code)',
                        fontSize: 'var(--fb-font-code-size)',
                        lineHeight: 'var(--fb-line-code)',
                        fontVariantLigatures: 'none',
                        fontFeatureSettings: '"liga" 0, "calt" 0',
                    },
                    '.cm-gutters': {
                        backgroundColor: 'var(--fb-surface)',
                        color: 'var(--fb-text-muted)',
                        borderRight: '1px solid var(--fb-border)',
                    },
                    // CodeMirror draws selection behind the active line; keep this translucent.
                    '.cm-activeLine': { backgroundColor: 'rgba(192, 132, 252, 0.08)' },
                    '.cm-activeLineGutter': {
                        backgroundColor: 'var(--fb-selected-surface)',
                        color: 'var(--fb-accent-text)',
                    },
                    '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
                        backgroundColor: 'var(--fb-selection)',
                    },
                    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--fb-focus)' },
                    '.cm-matchingBracket': {
                        backgroundColor: 'var(--fb-selected-surface)',
                        outline: '1px solid var(--fb-focus)',
                    },
                    '.cm-content': { caretColor: 'var(--fb-focus)', padding: '16px 0' },
                },
                { dark: true },
            ),
            syntaxHighlighting(
                HighlightStyle.define([
                    { tag: tags.keyword, color: 'var(--fb-accent-text)' },
                    { tag: tags.string, color: 'var(--fb-success)' },
                    { tag: tags.number, color: 'var(--fb-warning)' },
                    { tag: tags.comment, color: 'var(--fb-text-muted)', fontStyle: 'italic' },
                    { tag: tags.propertyName, color: 'var(--fb-info)' },
                    { tag: tags.heading, color: 'var(--fb-brand-fold)', fontWeight: '700' },
                ]),
            ),
            EditorView.contentAttributes.of({ 'aria-label': 'Secure note editor' }),
            languageCompartment.of(
                initialModel.length <= expensiveFeatureLimit ? initialLanguageExtension : [],
            ),
            readOnlyCompartment.of(readOnlyExtension(props.readOnly)),
            wrapCompartment.of(props.wrap ? lineWrappingExtension : []),
            EditorView.updateListener.of((update: ViewUpdate) => {
                if (update.docChanged) emit('update:modelValue', update.state.doc.toString());
            }),
        ];
        const state: EditorState = EditorState.create({ doc: initialModel, extensions });
        view = new EditorView({ state, parent: editorHost.value });
        ready.value = true;
    } catch {
        // The textarea remains functional when dynamic imports or browser APIs are unavailable.
        ready.value = false;
    }
}

watch(
    () => props.modelValue,
    (value) => {
        fallback.value = value;
        if (!view || value === view.state.doc.toString()) return;
        view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } });
    },
);
watch(
    () => props.language,
    async (language) => {
        if (!view || !languageCompartment) return;
        const extension =
            view.state.doc.length <= expensiveFeatureLimit
                ? await safeLanguageExtension(language)
                : [];
        if (!destroyed && view && props.language === language)
            view.dispatch({ effects: languageCompartment.reconfigure(extension) });
    },
);
watch(
    () => props.wrap,
    (wrap) => {
        if (view && wrapCompartment)
            view.dispatch({
                effects: wrapCompartment.reconfigure(wrap ? (lineWrappingExtension ?? []) : []),
            });
    },
);
watch(
    () => props.readOnly,
    (readOnly) => {
        if (view && readOnlyCompartment && readOnlyExtension)
            view.dispatch({
                effects: readOnlyCompartment.reconfigure(readOnlyExtension(readOnly)),
            });
    },
);

function updateFallback(event: Event): void {
    emit('update:modelValue', (event.target as HTMLTextAreaElement).value);
}

onMounted(() => void mountEditor());
onBeforeUnmount(() => {
    destroyed = true;
    view?.destroy();
    view = undefined;
});
</script>

<template>
    <div class="h-full min-h-0" data-testid="note-editor">
        <textarea
            v-if="!ready"
            id="private-note"
            :value="fallback"
            :readonly="readOnly"
            aria-label="Secure note editor"
            class="fb-code h-full min-h-72 w-full resize-y bg-[var(--fb-editor-bg)] px-4 py-4 text-[var(--fb-text)] outline-none placeholder:text-[var(--fb-text-muted)]"
            placeholder="Write a private note..."
            @input="updateFallback"
        />
        <div
            v-show="ready"
            ref="editorHost"
            class="h-full min-h-72"
            aria-label="Secure note editor"
        />
    </div>
</template>

<style scoped>
@media (pointer: coarse) {
    .fb-code,
    :deep(.cm-editor),
    :deep(.cm-editor .cm-scroller) {
        font-size: 1rem !important;
    }
}
</style>
