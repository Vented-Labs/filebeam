<script setup lang="ts">
import CodeEditor from './CodeEditor.vue';
import CopyButton from '../primitives/CopyButton.vue';

defineProps<{ content: string; language: string }>();
</script>

<template>
    <section
        class="overflow-hidden rounded-xl border border-[var(--fb-border)] bg-[var(--fb-surface)]"
    >
        <div
            class="flex items-center justify-between gap-3 border-b border-[var(--fb-border)] px-4 py-2 text-sm text-[var(--fb-text-muted)]"
        >
            Decrypted note
            <CopyButton :value="content" label="Copy note" variant="ghost" icon-only />
        </div>
        <div class="h-96 max-h-[65vh]">
            <CodeEditor :model-value="content" :language="language" read-only />
        </div>
    </section>
</template>
