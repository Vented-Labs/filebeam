<script setup lang="ts">
import { Head, router } from '@inertiajs/vue3';
import { computed, ref } from 'vue';
import { RouteSurface } from '@filebeam/ui';
import AppLink from '../../../../ui/src/components/primitives/AppLink.vue';
import Button from '../../../../ui/src/components/primitives/Button.vue';
import { formatBytes } from '../../../../ui/src/lib/format';
import { csrfHeaders } from '../../../../ui/src/lib/csrf';

defineOptions({ layout: RouteSurface });
type Entry = {
    id: string;
    kind: string;
    driver: string;
    delivery: string;
    status: string;
    item_count: number;
    ciphertext_bytes: number;
    declared_ciphertext_bytes: number;
    created_at: string;
    completed_at: string | null;
    published_at: string | null;
    expires_at: string;
    removed_at: string | null;
    retention_hours: number;
    maximum_retention_hours: number;
    maximum_expires_at: string | null;
    can_delete: boolean;
    can_extend: boolean;
    burn_on_read: boolean;
};
const props = defineProps<{ history: { data: Entry[]; next_cursor: string | null } }>();
const params = new URLSearchParams(typeof window === 'undefined' ? '' : window.location.search);
const status = ref(params.get('status') ?? '');
const kind = ref(params.get('kind') ?? '');
const driver = ref(params.get('driver') ?? '');
const pending = ref('');
const error = ref('');
const confirming = ref('');
const extending = ref('');
const hours = ref(48);
const statuses = [
    'pending',
    'available',
    'live',
    'ended',
    'deleting',
    'expired',
    'deleted',
    'abandoned',
    'burned',
    'removed',
];
const labels: Record<string, string> = {
    pending: 'Uploading',
    deleting: 'Awaiting cleanup',
    burned: 'Burned after reading',
    abandoned: 'Abandoned upload',
};
const label = (value: string) => labels[value] ?? value.charAt(0).toUpperCase() + value.slice(1);
const date = (value: string) => new Date(value).toLocaleString();
const selected = computed(() => props.history.data.find((entry) => entry.id === extending.value));
const proposedExpiry = computed(() => {
    const entry = selected.value;
    if (!entry) return '';
    const base =
        entry.driver === 'webrtc' ? (entry.published_at ?? entry.created_at) : entry.completed_at;
    if (!base || !entry.maximum_expires_at) return '';
    return date(
        new Date(
            Math.min(
                new Date(base).getTime() + hours.value * 3_600_000,
                new Date(entry.maximum_expires_at).getTime(),
            ),
        ).toISOString(),
    );
});
function visit(cursor?: string | null): void {
    router.get(
        '/account/history',
        {
            ...(status.value ? { status: status.value } : {}),
            ...(kind.value ? { kind: kind.value } : {}),
            ...(driver.value ? { driver: driver.value } : {}),
            ...(cursor ? { cursor } : {}),
        },
        { preserveState: true, preserveScroll: true },
    );
}
async function mutate(entry: Entry, action: 'delete' | 'extend'): Promise<void> {
    pending.value = entry.id;
    error.value = '';
    try {
        const response = await fetch(
            `/account/history/${entry.id}${action === 'extend' ? '/retention' : ''}`,
            {
                method: action === 'extend' ? 'PATCH' : 'DELETE',
                headers: {
                    ...csrfHeaders(),
                    Accept: 'application/json',
                    'Content-Type': 'application/json',
                },
                ...(action === 'extend'
                    ? { body: JSON.stringify({ retention_hours: hours.value }) }
                    : {}),
            },
        );
        if (!response.ok || response.redirected) {
            const body = await response.json().catch(() => null);
            throw new Error(
                body?.errors?.retention_hours?.[0] ??
                    body?.message ??
                    'Could not update this transfer.',
            );
        }
        confirming.value = '';
        extending.value = '';
        router.reload({ only: ['history'] });
    } catch (reason) {
        error.value = reason instanceof Error ? reason.message : 'Could not update this transfer.';
    } finally {
        pending.value = '';
    }
}
</script>

<template>
    <Head title="History" />
    <section class="history-page">
        <header>
            <h1>Transfer history</h1>
            <AppLink href="/account" class="fb-text-link">Account</AppLink>
        </header>
        <p class="history-intro">
            Your outgoing encrypted files and notes. Removed transfers remain here for 90 days.
            Names and share-link keys stay on your device.
        </p>
        <div class="history-filters">
            <label
                >Status<select v-model="status" aria-label="Status" @change="visit()">
                    <option value="">All states</option>
                    <option v-for="state in statuses" :key="state" :value="state">
                        {{ label(state) }}
                    </option>
                </select></label
            >
            <label
                >Type<select v-model="kind" aria-label="Type" @change="visit()">
                    <option value="">Files and notes</option>
                    <option value="files">Files</option>
                    <option value="note">Notes</option>
                </select></label
            >
            <label
                >Transport<select v-model="driver" aria-label="Transport" @change="visit()">
                    <option value="">All transports</option>
                    <option value="http">Stored (HTTP)</option>
                    <option value="webrtc">Live (WebRTC)</option>
                </select></label
            >
            <Button variant="secondary" @click="router.reload({ only: ['history'] })"
                >Refresh</Button
            >
        </div>
        <p v-if="error" role="alert" class="history-error">{{ error }}</p>
        <p v-if="!history.data.length" class="history-empty">
            No outgoing transfers match these filters.
        </p>
        <ul v-else class="history-list">
            <li v-for="entry in history.data" :key="entry.id" class="history-entry">
                <div class="history-details">
                    <h2>
                        {{
                            entry.kind === 'note'
                                ? 'Encrypted note'
                                : `${entry.item_count} encrypted ${entry.item_count === 1 ? 'file' : 'files'}`
                        }}
                        <span>{{ label(entry.status) }}</span>
                    </h2>
                    <code>{{ entry.id }}</code>
                    <p>
                        {{ formatBytes(entry.ciphertext_bytes || entry.declared_ciphertext_bytes) }}
                        encrypted ·
                        {{ entry.driver === 'webrtc' ? 'Live WebRTC' : 'Stored HTTP' }} ·
                        {{ entry.delivery === 'inbox' ? 'Inbox delivery' : 'Link share'
                        }}<template v-if="entry.burn_on_read"> · Burn on read</template>
                    </p>
                    <p>Created {{ date(entry.created_at) }}</p>
                    <p v-if="entry.removed_at">Removed {{ date(entry.removed_at) }}</p>
                    <p v-else>Expires {{ date(entry.expires_at) }}</p>
                </div>
                <div class="history-actions">
                    <template v-if="confirming === entry.id">
                        <p>
                            Delete this transfer for everyone? Its encrypted content will be
                            removed.
                        </p>
                        <Button
                            variant="danger"
                            :disabled="!!pending"
                            @click="mutate(entry, 'delete')"
                            >Confirm deletion</Button
                        >
                        <Button variant="ghost" :disabled="!!pending" @click="confirming = ''"
                            >Cancel</Button
                        >
                    </template>
                    <template v-else-if="extending === entry.id">
                        <label
                            >Total retention (hours)<input
                                v-model.number="hours"
                                type="number"
                                min="1"
                                :max="entry.maximum_retention_hours"
                        /></label>
                        <p>New expiry: {{ proposedExpiry }}</p>
                        <p v-if="entry.driver === 'webrtc'">
                            The live-session limit still applies. Keep the sender running.
                        </p>
                        <Button
                            variant="secondary"
                            :disabled="
                                !!pending ||
                                hours <= entry.retention_hours ||
                                hours > entry.maximum_retention_hours
                            "
                            @click="mutate(entry, 'extend')"
                            >Save retention</Button
                        >
                        <Button variant="ghost" :disabled="!!pending" @click="extending = ''"
                            >Cancel</Button
                        >
                    </template>
                    <template v-else>
                        <Button
                            v-if="entry.can_extend"
                            variant="secondary"
                            :disabled="!!pending"
                            @click="
                                extending = entry.id;
                                hours = Math.min(
                                    entry.maximum_retention_hours,
                                    entry.retention_hours + 24,
                                );
                            "
                            >Extend</Button
                        >
                        <Button
                            v-if="entry.can_delete"
                            variant="danger"
                            :disabled="!!pending"
                            @click="confirming = entry.id"
                            >Delete</Button
                        >
                    </template>
                </div>
            </li>
        </ul>
        <nav class="history-pagination" aria-label="History pagination">
            <Button variant="ghost" @click="visit()">Newest</Button
            ><Button
                v-if="history.next_cursor"
                variant="secondary"
                @click="visit(history.next_cursor)"
                >Older transfers</Button
            >
        </nav>
    </section>
</template>

<style scoped>
.history-page {
    width: min(100% - 2rem, 68rem);
    margin-inline: auto;
    padding-block: 3rem;
    color: var(--fb-text);
}
header,
.history-filters,
.history-pagination {
    display: flex;
    gap: 1rem;
    align-items: center;
    flex-wrap: wrap;
}
header {
    justify-content: space-between;
}
h1 {
    margin: 0;
    font-size: 2rem;
}
.history-intro,
.history-details p,
.history-actions p {
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
    line-height: 1.6;
}
.history-filters {
    margin-block: 1.5rem;
}
label {
    display: grid;
    gap: 0.375rem;
    font-size: 0.8125rem;
}
select,
input {
    padding: 0.6rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.5rem;
    background: var(--fb-surface);
    color: var(--fb-text);
}
.history-list {
    display: grid;
    gap: 0.75rem;
    list-style: none;
    padding: 0;
}
.history-entry {
    display: flex;
    justify-content: space-between;
    gap: 1.5rem;
    padding: 1.25rem;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
}
.history-details {
    min-width: 0;
}
h2 {
    font-size: 1rem;
    margin: 0 0 0.5rem;
}
h2 span {
    display: inline-block;
    margin-left: 0.5rem;
    font-size: 0.75rem;
    color: var(--fb-text-muted);
}
code {
    overflow-wrap: anywhere;
    font-size: 0.75rem;
}
.history-details p {
    margin: 0.375rem 0 0;
}
.history-actions {
    display: flex;
    align-items: center;
    justify-content: end;
    flex-wrap: wrap;
    gap: 0.5rem;
    max-width: 22rem;
}
.history-actions p {
    width: 100%;
    margin: 0;
}
.history-error {
    color: var(--fb-danger);
}
.history-empty {
    padding: 3rem 1rem;
    text-align: center;
    color: var(--fb-text-muted);
}
.history-pagination {
    justify-content: end;
    margin-top: 1.5rem;
}
@media (max-width: 640px) {
    .history-entry {
        flex-direction: column;
    }
    .history-actions {
        max-width: none;
        justify-content: start;
    }
}
</style>
