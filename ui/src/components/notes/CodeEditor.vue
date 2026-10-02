<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import type { Compartment, EditorState, Extension } from '@codemirror/state';
import type { StringStream } from '@codemirror/language';
import type { EditorView, ViewUpdate } from '@codemirror/view';
import { normalizeNoteLanguage } from '../../lib/note-languages';
import { useAppearance } from '../../composables/useAppearance';

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
const { mode } = useAppearance();

const editorHost = ref<HTMLElement>();
const fallback = ref(props.modelValue);
const ready = ref(false);
const fallbackInput = ref<HTMLTextAreaElement>();
let focusPending = false;
function focusEnd(): void {
    if (view) {
        view.dispatch({ selection: { anchor: view.state.doc.length }, scrollIntoView: true });
        view.focus();
        focusPending = false;
    } else {
        focusPending = true;
        fallbackInput.value?.focus();
        fallbackInput.value?.setSelectionRange(props.modelValue.length, props.modelValue.length);
    }
}
defineExpose({ focusEnd });
let view: EditorView | undefined;
let languageCompartment: Compartment | undefined;
let readOnlyCompartment: Compartment | undefined;
let wrapCompartment: Compartment | undefined;
let appearanceCompartment: Compartment | undefined;
let appearanceExtension: ((dark: boolean) => Extension) | undefined;
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
            { filebeamEditorChrome, filebeamHighlighting },
            { indentWithTab },
            { keymap },
        ] = await Promise.all([
            import('codemirror'),
            import('@codemirror/state'),
            import('../../lib/editor-theme'),
            import('@codemirror/commands'),
            import('@codemirror/view'),
        ]);
        if (destroyed || !editorHost.value) return;
        languageCompartment = new Compartment();
        readOnlyCompartment = new Compartment();
        wrapCompartment = new Compartment();
        appearanceCompartment = new Compartment();
        appearanceExtension = (dark: boolean) => EditorView.darkTheme.of(dark);
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
            appearanceCompartment.of(appearanceExtension(mode.value === 'dark')),
            basicSetup,
            keymap.of([indentWithTab]),
            filebeamEditorChrome,
            filebeamHighlighting,
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
        const transferFocus = focusPending && document.activeElement === fallbackInput.value;
        ready.value = true;
        await nextTick();
        if (transferFocus && !destroyed) focusEnd();
        else focusPending = false;
    } catch {
        // The textarea remains functional when dynamic imports or browser APIs are unavailable.
        ready.value = false;
    }
}

watch(mode, (value) => {
    if (view && appearanceCompartment && appearanceExtension) {
        view.dispatch({
            effects: appearanceCompartment.reconfigure(appearanceExtension(value === 'dark')),
        });
    }
});

watch(
    () => props.modelValue,
    (value) => {
        fallback.value = value;
        if (!view || value === view.state.doc.toString()) return;
        view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } });
    },
);
watch(
    [() => props.language, () => props.modelValue.length > expensiveFeatureLimit],
    async ([language, large]) => {
        if (!view || !languageCompartment) return;
        const extension = large ? [] : await safeLanguageExtension(language);
        if (
            !destroyed &&
            view &&
            props.language === language &&
            view.state.doc.length > expensiveFeatureLimit === large
        )
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
            ref="fallbackInput"
            :value="fallback"
            :readonly="readOnly"
            aria-label="Secure note editor"
            class="fb-code fb-editor-fallback h-full min-h-72 w-full resize-y bg-[var(--fb-editor-bg)] px-4 py-4 text-[var(--fb-editor-fg)] outline-none placeholder:text-[var(--fb-editor-comment)]"
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
.fb-editor-fallback {
    caret-color: var(--fb-editor-caret);
    transition-property: none;
}
.fb-editor-fallback::selection {
    background: var(--fb-editor-selection);
}
@media (pointer: coarse) {
    .fb-code,
    :deep(.cm-editor),
    :deep(.cm-editor .cm-scroller) {
        font-size: 1rem !important;
    }
}
</style>
