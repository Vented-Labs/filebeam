<script setup lang="ts">
import { computed, ref } from 'vue';
import AuthLink from '../auth/AuthLink.vue';
import AppLink from '../primitives/AppLink.vue';
import Icon from '../primitives/Icon.vue';
import { onTabKeydown } from './tabs';

defineProps<{ signedIn: boolean }>();

// Static example rows; nothing here reads or changes real history.
const rows = [
    {
        id: '01K7…J2H',
        kind: 'file',
        type: '2 files · HTTP · 16.4 MB',
        active: true,
        retention: '1 day',
    },
    {
        id: '01K6…R8N',
        kind: 'note',
        type: 'Note · HTTP · 1.1 KB',
        active: true,
        retention: '3 days',
    },
    {
        id: '01K5…E9P',
        kind: 'file',
        type: '1 file · HTTP · 4.2 MB',
        active: false,
        retention: 'Expired',
    },
] as const;
const filter = ref<'all' | 'active'>('all');
const visible = computed(() => rows.filter((row) => filter.value === 'all' || row.active));
</script>

<template>
    <article class="feature-card history-card" aria-labelledby="history-title">
        <div class="card-heading">
            <div class="section-icon"><Icon class="icon" name="clock" /></div>
            <span class="pill">New</span>
        </div>
        <h2 id="history-title">Keep track of transfers.</h2>
        <p>
            Review what you sent. Extend retention or remove a transfer.<br />Available on web,
            desktop, and CLI.
        </p>
        <div class="history-demo">
            <div class="history-toolbar">
                <span>History</span>
                <div role="tablist" class="small-tabs" aria-label="Example history filter">
                    <button
                        v-for="(label, key) in { all: 'All', active: 'Active' }"
                        :id="`history-filter-${key}`"
                        :key="key"
                        type="button"
                        role="tab"
                        aria-controls="history-rows"
                        :aria-selected="filter === key"
                        :tabindex="filter === key ? 0 : -1"
                        @click="filter = key"
                        @keydown="onTabKeydown"
                    >
                        {{ label }}
                    </button>
                </div>
            </div>
            <div id="history-rows" role="tabpanel" :aria-labelledby="`history-filter-${filter}`">
                <div v-for="row in visible" :key="row.id" class="history-row">
                    <span class="history-file"><Icon class="icon" :name="row.kind" /></span>
                    <div class="history-row-info">
                        <code>{{ row.id }}</code
                        ><small>{{ row.type }}</small>
                    </div>
                    <div class="history-retention" :class="{ ended: !row.active }">
                        <strong>{{ row.retention }}</strong
                        ><span>{{ row.active ? 'retention' : 'summary' }}</span>
                    </div>
                </div>
            </div>
            <div class="history-caption">
                <Icon class="icon" name="lock" /> Operational details. No filenames or share-link
                keys.
            </div>
        </div>
        <AppLink v-if="signedIn" href="/account/history" class="feature-cta"
            >Open your history <Icon class="icon" name="arrow-right"
        /></AppLink>
        <AuthLink v-else href="/login" class="feature-cta"
            >Sign in to keep history <Icon class="icon" name="arrow-right"
        /></AuthLink>
    </article>
</template>
