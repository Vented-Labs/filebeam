<script setup lang="ts">
import FilebeamIcon from '../primitives/FilebeamIcon.vue';
import AppLink from '../primitives/AppLink.vue';

defineProps<{ message: string }>();
</script>

<template>
    <div class="mx-auto flex max-w-md flex-col items-center py-12 text-center sm:py-16">
        <div
            class="mb-7 grid size-24 place-items-center rounded-full border border-[var(--fb-border)] bg-[var(--fb-selected-surface)] text-[var(--fb-accent-text)]"
        >
            <FilebeamIcon name="file" :size="48" :stroke-width="1.4" />
        </div>
        <h1 class="text-3xl font-semibold tracking-tight">Transfer unavailable</h1>
        <p class="mt-3 max-w-sm text-base leading-relaxed text-[var(--fb-text-muted)]" role="alert">
            {{ message || 'This transfer is unavailable or has expired.' }}
        </p>
        <AppLink href="/" class="fb-button fb-button--secondary mt-8 gap-2"
            >Start a new transfer<FilebeamIcon name="arrow-right" :size="17"
        /></AppLink>
    </div>
</template>
