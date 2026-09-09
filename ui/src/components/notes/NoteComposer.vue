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
    maximumBytes: number;
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
    <section
        class="note-composer overflow-hidden rounded-2xl border border-[var(--fb-border)] bg-[var(--fb-surface)] shadow-2xl shadow-black/30"
    >
        <div class="border-b border-[var(--fb-border)] px-5 pb-4 pt-5 sm:px-6">
            <label for="note-title" class="sr-only">Note title (optional)</label>
            <input
                id="note-title"
                :value="title"
                :disabled="disabled"
                maxlength="160"
                autocomplete="off"
                placeholder="Title (optional)"
                class="fb-note-title w-full border-0 bg-transparent px-1 py-2 text-xl font-medium tracking-tight text-[var(--fb-text)] placeholder:text-[var(--fb-text-muted)]"
                @input="emit('update:title', ($event.target as HTMLInputElement).value)"
            />
        </div>
        <div
            class="flex min-h-12 items-center gap-3 border-b border-[var(--fb-border)] bg-[var(--fb-surface-raised)] px-3 sm:px-4"
        >
            <span class="fb-code min-w-0 truncate text-[var(--fb-text)]">{{ filename }}</span>
            <div class="ml-auto flex items-center gap-2">
                <SelectRoot
                    :model-value="language"
                    :disabled="disabled"
                    @update:model-value="emit('update:language', $event)"
                >
                    <SelectTrigger
                        aria-label="Note language"
                        class="fb-select-trigger fb-select-trigger--compact note-language-trigger"
                        ><Icon
                            name="file"
                            :size="15"
                            class="text-[var(--fb-text-muted)]" /><SelectValue /><Icon
                            name="chevron-down"
                            :size="15"
                            class="ml-auto"
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
                <label
                    class="inline-flex shrink-0 items-center gap-2 text-xs text-[var(--fb-text-muted)]"
                >
                    Wrap <Switch v-model="wrap" :disabled="disabled" aria-label="Wrap" />
                </label>
            </div>
        </div>
        <div class="h-[22rem] min-h-72">
            <CodeEditor
                :model-value="modelValue"
                :language="language"
                :read-only="disabled"
                :wrap="wrap"
                @update:model-value="emit('update:modelValue', $event)"
            />
        </div>
        <div
            class="fb-code flex items-center justify-end border-t border-[var(--fb-border)] bg-[var(--fb-surface-raised)] px-4 py-2 text-[var(--fb-text-muted)]"
        >
            <span :class="{ 'text-[var(--fb-danger)]': bytes > maximumBytes }"
                >{{ formatBytes(bytes) }} / {{ formatBytes(maximumBytes) }}</span
            >
        </div>
    </section>
</template>

<style scoped>
.note-language-trigger {
    width: 10rem;
    flex: none;
}
.fb-note-title {
    border-radius: var(--fb-radius-sm);
}
.fb-note-title:disabled {
    opacity: 0.48;
    cursor: not-allowed;
}
@media (max-width: 480px) {
    .note-language-trigger {
        width: 8.5rem;
    }
}
.note-composer {
    animation: note-enter 180ms ease-out both;
}
@keyframes note-enter {
    from {
        opacity: 0;
        transform: translateY(4px);
    }
    to {
        opacity: 1;
        transform: translateY(0);
    }
}
@media (prefers-reduced-motion: reduce) {
    .note-composer {
        animation: none;
    }
}
</style>
