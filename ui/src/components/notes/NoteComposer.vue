<script setup lang="ts">
import {
    SelectContent,
    SelectItem,
    SelectItemIndicator,
    SelectItemText,
    SelectPortal,
    SelectRoot,
    SelectTrigger,
    SelectValue,
    SelectViewport,
} from 'reka-ui';
import { computed, ref } from 'vue';
import { noteFilename, noteLanguages } from '../../lib/note-languages';
import CodeEditor from './CodeEditor.vue';
import Icon from '../primitives/Icon.vue';
import Switch from '../primitives/Switch.vue';

const props = defineProps<{
    modelValue: string;
    language: string;
    disabled: boolean;
    maximumBytes: number | null;
    retentionHours: number;
    title: string;
}>();
const emit = defineEmits<{
    'update:modelValue': [value: string];
    'update:language': [value: string];
    'update:title': [value: string];
}>();
const wrap = ref(true);
const bytes = computed(() => new TextEncoder().encode(props.modelValue).byteLength);
const filename = computed(() => noteFilename(props.language));
function formatBytes(value: number): string {
    return value < 1024 ? `${value} B` : `${(value / 1024).toFixed(1)} KB`;
}
</script>

<template>
    <section class="note-composer">
        <div class="note-composer__frame">
            <div class="note-composer__toolbar">
                <div class="note-composer__title">
                    <Icon name="note" :size="15" />
                    <label for="note-title" class="sr-only">Note title (optional)</label>
                    <input
                        id="note-title"
                        :value="title"
                        :disabled="disabled"
                        maxlength="160"
                        autocomplete="off"
                        placeholder="Untitled note"
                        class="fb-note-title"
                        @input="emit('update:title', ($event.target as HTMLInputElement).value)"
                    />
                </div>
                <div class="note-composer__tools">
                    <span class="fb-code note-composer__filename">{{ filename }}</span>
                    <SelectRoot
                        :model-value="language"
                        :disabled="disabled"
                        @update:model-value="emit('update:language', $event)"
                    >
                        <SelectTrigger
                            aria-label="Note language"
                            class="fb-select-trigger fb-select-trigger--compact note-language-trigger"
                            ><SelectValue /><Icon name="chevron-down" :size="15" class="ml-auto"
                        /></SelectTrigger>
                        <SelectPortal
                            ><SelectContent
                                position="popper"
                                :body-lock="false"
                                class="fb-select-content"
                                ><SelectViewport
                                    ><SelectItem
                                        v-for="item in noteLanguages"
                                        :key="item.value"
                                        :value="item.value"
                                        class="fb-select-item"
                                        ><SelectItemText>{{ item.label }}</SelectItemText
                                        ><SelectItemIndicator
                                            ><Icon
                                                name="check"
                                                :size="
                                                    15
                                                " /></SelectItemIndicator></SelectItem></SelectViewport></SelectContent
                        ></SelectPortal>
                    </SelectRoot>
                    <label class="note-composer__wrap">
                        Wrap <Switch v-model="wrap" :disabled="disabled" aria-label="Wrap" />
                    </label>
                </div>
            </div>
            <div class="note-composer__body">
                <CodeEditor
                    :model-value="modelValue"
                    :language="language"
                    :read-only="disabled"
                    :wrap="wrap"
                    @update:model-value="emit('update:modelValue', $event)"
                />
            </div>
            <div class="fb-code note-composer__footer">
                <span><Icon name="lock" :size="12" />Encrypted in your browser before upload.</span>
                <span
                    :class="{
                        'text-[var(--fb-danger)]': maximumBytes !== null && bytes > maximumBytes,
                    }"
                    >{{ formatBytes(bytes) }} /
                    {{ maximumBytes === null ? 'Unlimited' : formatBytes(maximumBytes) }}</span
                >
            </div>
        </div>
    </section>
</template>

<style scoped>
.note-language-trigger {
    width: 8.5rem;
    flex: none;
    white-space: nowrap;
}
.note-language-trigger :deep(span) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
.note-composer {
    padding: 1.375rem 1.5rem 1.5625rem;
    background: var(--fb-surface);
}
.note-composer__frame {
    overflow: hidden;
    border: 1px solid #44384f;
    border-radius: 0.8125rem;
    background: var(--fb-surface-sunken);
}
.note-composer__toolbar {
    display: flex;
    min-height: 2.9375rem;
    align-items: center;
    gap: 1rem;
    padding: 0.625rem 0.75rem;
    border-bottom: 1px solid var(--fb-border);
    background: radial-gradient(ellipse at 30% 0%, #a679ff12, transparent 72%), #211b2b;
}
.note-composer__title {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: 0.625rem;
}
.note-composer__title :deep(.fb-icon) {
    color: var(--fb-accent-text);
}
.fb-note-title {
    width: 100%;
    min-width: 0;
    padding: 0.25rem 0;
    border: 0;
    border-radius: var(--fb-radius-sm);
    outline: 0;
    color: var(--fb-text);
    background: transparent;
    font-size: 0.75rem;
}
.fb-note-title::placeholder {
    color: var(--fb-text-muted);
}
.fb-note-title:focus-visible {
    outline: 2px solid var(--fb-focus);
    outline-offset: 3px;
}
.fb-note-title:disabled {
    opacity: 0.48;
    cursor: not-allowed;
}
.note-composer__tools {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.75rem;
}
.note-composer__filename {
    max-width: 9rem;
    overflow: hidden;
    color: var(--fb-text-subtle);
    font-size: 0.625rem;
    text-overflow: ellipsis;
    white-space: nowrap;
}
.note-composer__wrap {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 0.5rem;
    color: var(--fb-text-muted);
    font-size: 0.625rem;
}
.note-composer__body {
    height: 16rem;
    min-height: 16rem;
    background: var(--fb-surface-sunken);
}
.note-composer__footer {
    display: flex;
    min-height: 2rem;
    box-sizing: border-box;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.5rem 0.75rem;
    border-top: 1px solid var(--fb-border);
    color: var(--fb-text-subtle);
    background: #1a1422;
    font-size: 0.625rem;
}
.note-composer__footer > span:first-child {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
}
@media (max-width: 480px) {
    .note-language-trigger {
        width: 7.75rem;
    }
    .note-composer {
        padding: 0.9375rem;
    }
    .note-composer__toolbar {
        align-items: stretch;
        flex-direction: column;
        gap: 0.5rem;
    }
    .note-composer__tools {
        justify-content: flex-end;
    }
    .note-composer__filename {
        margin-right: auto;
    }
    .note-composer__body {
        height: 20rem;
    }
    .note-composer__footer > span:first-child {
        display: none;
    }
}
</style>
