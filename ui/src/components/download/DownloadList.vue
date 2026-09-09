<script setup lang="ts">
import type { ManifestItem } from '../../composables/useEncryptedDownload';
import { formatBytes } from '../../lib/format';
import Icon from '../primitives/Icon.vue';

defineProps<{ items: ManifestItem[] }>();
</script>

<template>
    <ul class="download-list">
        <li v-for="item in items" :key="item.id">
            <span class="download-list__icon"><Icon name="file" :size="18" /></span>
            <span class="download-list__name" :title="item.name">{{ item.name }}</span>
            <span class="download-list__size">{{ formatBytes(item.size) }}</span>
        </li>
    </ul>
</template>

<style scoped>
.download-list {
    display: grid;
    gap: 0.375rem;
    margin-block: 0;
    padding: 0.375rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.875rem;
    background: var(--fb-surface-sunken);
    list-style: none;
}
.download-list li {
    display: grid;
    min-height: 3.75rem;
    grid-template-columns: 2.375rem minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.75rem;
    padding: 0.5rem 0.75rem;
    border: 1px solid #ffffff08;
    border-radius: 0.6875rem;
    background: #ffffff03;
}
.download-list__icon {
    display: grid;
    width: 2.375rem;
    height: 2.375rem;
    place-items: center;
    border-radius: 0.625rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.download-list__name {
    overflow: hidden;
    color: var(--fb-text);
    font-size: 0.875rem;
    font-weight: 550;
    text-overflow: ellipsis;
    white-space: nowrap;
}
.download-list__size {
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
    font-variant-numeric: tabular-nums;
}
</style>
