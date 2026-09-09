<script setup lang="ts">
import type { ManifestItem } from '../../composables/useEncryptedDownload';
import { formatBytes } from '../../lib/format';
import Icon from '../primitives/Icon.vue';

defineProps<{ items: ManifestItem[] }>();
</script>

<template>
    <ul
        class="divide-y divide-[var(--fb-border)] overflow-hidden rounded-xl border border-[var(--fb-border)] bg-[var(--fb-surface-raised)]"
    >
        <li
            v-for="item in items"
            :key="item.id"
            class="flex items-center justify-between gap-4 px-4 py-3"
        >
            <span class="flex min-w-0 items-center gap-3"
                ><Icon name="file" class="shrink-0 text-[var(--fb-accent-text)]" /><span
                    class="truncate"
                    >{{ item.name }}</span
                ></span
            >
            <span class="shrink-0 text-sm text-[var(--fb-text-muted)]">{{
                formatBytes(item.size)
            }}</span>
        </li>
    </ul>
</template>
