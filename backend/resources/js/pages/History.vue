<script setup lang="ts">
import { Head, router } from '@inertiajs/vue3';
import { computed, ref } from 'vue';
import {
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
} from 'reka-ui';
import { RouteSurface } from '@filebeam/ui';
import AppLink from '../../../../ui/src/components/primitives/AppLink.vue';
import Button from '../../../../ui/src/components/primitives/Button.vue';
import Icon from '../../../../ui/src/components/primitives/Icon.vue';
import Input from '../../../../ui/src/components/primitives/Input.vue';
import Select from '../../../../ui/src/components/primitives/Select.vue';
import CopyButton from '../../../../ui/src/components/primitives/CopyButton.vue';
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
const deleting = computed(() => props.history.data.find((entry) => entry.id === confirming.value));
const deleteOpen = computed({
    get: () => !!confirming.value,
    set: (open: boolean) => {
        if (!open) {
            confirming.value = '';
            error.value = '';
        }
    },
});
const extendOpen = computed({
    get: () => !!extending.value,
    set: (open: boolean) => {
        if (!open) {
            extending.value = '';
            error.value = '';
        }
    },
});
const retentionValid = computed(
    () =>
        !!selected.value &&
        Number.isSafeInteger(hours.value) &&
        hours.value > selected.value.retention_hours &&
        hours.value <= selected.value.maximum_retention_hours,
);
const proposedExpiry = computed(() => {
    const entry = selected.value;
    if (!entry || !Number.isSafeInteger(hours.value)) return '';
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
function extend(entry: Entry): void {
    error.value = '';
    hours.value = Math.min(entry.maximum_retention_hours, entry.retention_hours + 24);
    extending.value = entry.id;
}
const statusTone = (value: string) =>
    ['available', 'live'].includes(value)
        ? 'success'
        : ['pending', 'deleting', 'abandoned'].includes(value)
          ? 'warning'
          : 'muted';
async function mutate(entry: Entry, action: 'delete' | 'extend'): Promise<void> {
    if (pending.value) return;
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
        <header class="history-header">
            <div>
                <h1>Transfer history</h1>
                <p>Your outgoing encrypted files and notes, across devices.</p>
            </div>
            <AppLink href="/account" class="fb-button fb-button--secondary"
                ><Icon name="user" :size="16" />Account</AppLink
            >
        </header>
        <div class="history-toolbar">
            <div class="history-filters">
                <div class="history-filter">
                    <span>Status</span
                    ><Select
                        v-model="status"
                        label="Status"
                        :options="[
                            { value: '', label: 'All states' },
                            ...statuses.map((state) => ({ value: state, label: label(state) })),
                        ]"
                        @change="visit()"
                    />
                </div>
                <div class="history-filter">
                    <span>Type</span
                    ><Select
                        v-model="kind"
                        label="Type"
                        :options="[
                            { value: '', label: 'Files and notes' },
                            { value: 'files', label: 'Files' },
                            { value: 'note', label: 'Notes' },
                        ]"
                        @change="visit()"
                    />
                </div>
                <div class="history-filter">
                    <span>Transport</span
                    ><Select
                        v-model="driver"
                        label="Transport"
                        :options="[
                            { value: '', label: 'All transports' },
                            { value: 'http', label: 'Stored (HTTP)' },
                            { value: 'webrtc', label: 'Live (WebRTC)' },
                        ]"
                        @change="visit()"
                    />
                </div>
                <Button variant="secondary" @click="router.reload({ only: ['history'] })"
                    ><Icon name="rotate-right" :size="16" />Refresh</Button
                >
            </div>
            <p class="history-privacy">
                <Icon name="lock" :size="14" />Names and share-link keys stay on your device.
            </p>
        </div>
        <p v-if="error && !confirming && !extending" role="alert" class="history-error">
            <Icon name="alert" :size="16" />{{ error }}
        </p>
        <div v-if="!history.data.length" class="history-empty">
            <span class="history-empty__mark"><Icon name="clock" :size="28" /></span>
            <h2>No transfers to show</h2>
            <p>No outgoing transfers match these filters.</p>
            <AppLink href="/" class="fb-button fb-button--secondary"
                ><Icon name="arrow-up" :size="16" />Send a transfer</AppLink
            >
        </div>
        <ul v-else class="history-list">
            <li v-for="entry in history.data" :key="entry.id" class="history-entry">
                <div class="history-entry__header">
                    <span class="history-entry__mark"
                        ><Icon :name="entry.kind === 'note' ? 'note' : 'folder'" :size="20"
                    /></span>
                    <div class="history-entry__identity">
                        <div class="history-entry__heading">
                            <h2>
                                {{
                                    entry.kind === 'note'
                                        ? 'Encrypted note'
                                        : `${entry.item_count} encrypted ${entry.item_count === 1 ? 'file' : 'files'}`
                                }}
                            </h2>
                            <span class="history-status" :data-tone="statusTone(entry.status)"
                                ><span />{{ label(entry.status) }}</span
                            >
                        </div>
                        <div class="history-entry__id">
                            <code>{{ entry.id }}</code
                            ><CopyButton
                                :value="entry.id"
                                label="Copy transfer ID"
                                variant="ghost"
                                icon-only
                            />
                        </div>
                    </div>
                    <div class="history-actions">
                        <Button
                            v-if="entry.can_extend"
                            variant="secondary"
                            :disabled="!!pending"
                            @click="extend(entry)"
                            ><Icon name="clock" :size="15" />Extend</Button
                        >
                        <Button
                            v-if="entry.can_delete"
                            variant="ghost"
                            class="history-delete"
                            :disabled="!!pending"
                            @click="
                                error = '';
                                confirming = entry.id;
                            "
                            ><Icon name="trash" :size="15" />Delete</Button
                        >
                    </div>
                </div>
                <dl class="history-entry__metadata">
                    <div>
                        <dt>Encrypted size</dt>
                        <dd>
                            {{
                                formatBytes(
                                    entry.ciphertext_bytes || entry.declared_ciphertext_bytes,
                                )
                            }}
                        </dd>
                    </div>
                    <div>
                        <dt>Delivery</dt>
                        <dd>
                            {{ entry.driver === 'webrtc' ? 'Live WebRTC' : 'Stored HTTP' }} ·
                            {{ entry.delivery === 'inbox' ? 'Inbox' : 'Link share'
                            }}<span v-if="entry.burn_on_read"> · Burn on read</span>
                        </dd>
                    </div>
                    <div>
                        <dt>Created</dt>
                        <dd>{{ date(entry.created_at) }}</dd>
                    </div>
                    <div>
                        <dt>{{ entry.removed_at ? 'Removed' : 'Expires' }}</dt>
                        <dd>{{ date(entry.removed_at ?? entry.expires_at) }}</dd>
                    </div>
                </dl>
            </li>
        </ul>
        <footer class="history-footer">
            <p>Removed transfers remain in your history for 90 days.</p>
            <nav aria-label="History pagination">
                <Button variant="ghost" @click="visit()">Newest</Button
                ><Button
                    v-if="history.next_cursor"
                    variant="secondary"
                    @click="visit(history.next_cursor)"
                    >Older transfers<Icon name="arrow-right" :size="15"
                /></Button>
            </nav>
        </footer>
    </section>

    <DialogRoot v-model:open="deleteOpen">
        <DialogPortal
            ><DialogOverlay class="fb-dialog__overlay" /><DialogContent class="fb-dialog__content">
                <DialogTitle class="fb-dialog__title">Delete this transfer?</DialogTitle>
                <DialogDescription class="fb-dialog__description"
                    >Delete this transfer for everyone? Its encrypted content will be removed. Your
                    history summary will remain for 90 days.</DialogDescription
                >
                <p v-if="error" role="alert" class="history-error">{{ error }}</p>
                <div class="history-dialog__actions">
                    <DialogClose as-child
                        ><Button variant="ghost" :disabled="!!pending">Cancel</Button></DialogClose
                    ><Button
                        v-if="deleting"
                        variant="danger"
                        :disabled="!!pending"
                        @click="mutate(deleting, 'delete')"
                        ><Icon name="trash" :size="16" />{{
                            pending ? 'Deleting…' : 'Confirm deletion'
                        }}</Button
                    >
                </div>
                <DialogClose class="fb-dialog__close" aria-label="Close deletion confirmation"
                    ><Icon name="x" :size="18"
                /></DialogClose> </DialogContent
        ></DialogPortal>
    </DialogRoot>
    <DialogRoot v-model:open="extendOpen">
        <DialogPortal
            ><DialogOverlay class="fb-dialog__overlay" /><DialogContent class="fb-dialog__content">
                <DialogTitle class="fb-dialog__title">Extend transfer retention</DialogTitle>
                <DialogDescription class="fb-dialog__description"
                    >Choose the total lifetime from publication. The maximum allowed by your plan is
                    {{ selected?.maximum_retention_hours }} hours.</DialogDescription
                >
                <form
                    v-if="selected"
                    class="history-retention"
                    @submit.prevent="retentionValid && mutate(selected, 'extend')"
                >
                    <label for="history-retention-hours">Total retention (hours)</label
                    ><Input
                        id="history-retention-hours"
                        :model-value="hours"
                        type="number"
                        min="1"
                        :max="selected.maximum_retention_hours"
                        :disabled="!!pending"
                        @update:model-value="hours = Number($event)"
                    />
                    <div v-if="proposedExpiry" class="history-retention__expiry">
                        <Icon name="clock" :size="16" /><span
                            >New expiry<strong>{{ proposedExpiry }}</strong></span
                        >
                    </div>
                    <p v-if="selected.driver === 'webrtc'" class="history-dialog__help">
                        The live-session limit still applies. Keep the sender running.
                    </p>
                    <p v-if="error" role="alert" class="history-error">{{ error }}</p>
                    <div class="history-dialog__actions">
                        <DialogClose as-child
                            ><Button variant="ghost" :disabled="!!pending"
                                >Cancel</Button
                            ></DialogClose
                        ><Button type="submit" :disabled="!!pending || !retentionValid">{{
                            pending ? 'Saving…' : 'Save retention'
                        }}</Button>
                    </div>
                </form>
                <DialogClose class="fb-dialog__close" aria-label="Close retention settings"
                    ><Icon name="x" :size="18"
                /></DialogClose> </DialogContent
        ></DialogPortal>
    </DialogRoot>
</template>

<style scoped>
.history-page {
    width: min(100% - 2rem, 72rem);
    margin-inline: auto;
    padding-block: 3.5rem 4.5rem;
    color: var(--fb-text);
}
.history-header {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 2rem;
    margin-bottom: 1.5rem;
}
.history-header h1 {
    margin: 0;
    font-size: 2.25rem;
    letter-spacing: -0.04em;
}
.history-header p {
    margin: 0.625rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
    line-height: 1.7;
}
.history-toolbar {
    overflow: hidden;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-settings-surface);
    box-shadow: var(--fb-shadow-panel);
}
.history-filters {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr)) auto;
    align-items: end;
    gap: 1rem;
    padding: 1.25rem 1.5rem;
}
.history-filter {
    display: grid;
    min-width: 0;
    gap: 0.5rem;
}
.history-filter > span {
    color: var(--fb-text-muted);
    font-size: 0.6875rem;
    font-weight: 600;
}
.history-filter :deep(.fb-select-trigger) {
    width: 100%;
    min-width: 0;
}
.history-privacy {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin: 0;
    padding: 0.75rem 1.5rem;
    border-top: 1px solid var(--fb-border);
    color: var(--fb-text-subtle);
    background: var(--fb-surface);
    font-size: 0.6875rem;
    line-height: 1.6;
}
.history-privacy :deep(.fb-icon) {
    flex: none;
}
.history-list {
    display: grid;
    gap: 0.75rem;
    margin: 1.5rem 0 0;
    padding: 0;
    list-style: none;
}
.history-entry {
    overflow: hidden;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
    box-shadow: var(--fb-shadow-panel);
}
.history-entry__header {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 1.25rem 1.5rem;
}
.history-entry__mark,
.history-empty__mark {
    display: grid;
    width: 2.75rem;
    height: 2.75rem;
    flex: none;
    place-items: center;
    border: 1px solid var(--fb-card-border);
    border-radius: 0.8125rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.history-entry__identity {
    min-width: 0;
    flex: 1;
}
.history-entry__heading {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.75rem;
}
.history-entry h2 {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 600;
    letter-spacing: -0.02em;
}
.history-entry__id {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    margin-top: 0.25rem;
    color: var(--fb-text-subtle);
}
.history-entry code {
    overflow-wrap: anywhere;
    font-family: var(--fb-font-code);
    font-size: 0.6875rem;
}
.history-entry__id :deep(.fb-button) {
    width: 1.75rem;
    min-height: 1.75rem;
    padding: 0;
}
.history-status {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.25rem 0.5rem;
    border: 1px solid var(--fb-border);
    border-radius: 99rem;
    color: var(--fb-text-muted);
    font-size: 0.625rem;
    font-weight: 500;
}
.history-status > span {
    width: 0.3125rem;
    height: 0.3125rem;
    border-radius: 50%;
    background: currentColor;
}
.history-status[data-tone='success'] {
    color: var(--fb-success);
}
.history-status[data-tone='warning'] {
    color: var(--fb-warning);
}
.history-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
}
.history-delete {
    color: var(--fb-danger);
}
.history-entry__metadata {
    display: grid;
    grid-template-columns: 0.7fr 1fr 1fr 1fr;
    gap: 1rem;
    margin: 0;
    padding: 1rem 1.5rem;
    border-top: 1px solid var(--fb-border);
    background: var(--fb-settings-surface);
}
.history-entry dt {
    margin-bottom: 0.375rem;
    color: var(--fb-text-subtle);
    font-size: 0.625rem;
    font-weight: 500;
}
.history-entry dd {
    margin: 0;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    line-height: 1.6;
}
.history-empty {
    display: grid;
    min-height: 20rem;
    align-content: center;
    justify-items: center;
    gap: 0.75rem;
    margin-top: 1.5rem;
    padding: 2rem;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
    box-shadow: var(--fb-shadow-panel);
    text-align: center;
}
.history-empty__mark {
    width: 3.75rem;
    height: 3.75rem;
    border-radius: 1rem;
}
.history-empty h2 {
    margin: 0.25rem 0 0;
    font-size: 1.25rem;
    letter-spacing: -0.025em;
}
.history-empty p {
    margin: 0 0 0.5rem;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
}
.history-footer {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    margin-top: 1.25rem;
}
.history-footer > p {
    margin: 0;
    color: var(--fb-text-subtle);
    font-size: 0.6875rem;
}
.history-footer nav {
    display: flex;
    gap: 0.5rem;
}
.history-error {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 1rem;
    color: var(--fb-danger);
    font-size: 0.8125rem;
}
.history-dialog__actions {
    display: flex;
    justify-content: end;
    gap: 0.625rem;
    margin-top: 1.5rem;
}
.history-retention {
    margin-top: 1.5rem;
}
.history-retention > label {
    display: block;
    margin-bottom: 0.5rem;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    font-weight: 500;
}
.history-retention__expiry {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    margin-top: 1rem;
    padding: 0.875rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.6875rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.history-retention__expiry span {
    color: var(--fb-text-muted);
    font-size: 0.6875rem;
}
.history-retention__expiry strong {
    display: block;
    margin-top: 0.25rem;
    color: var(--fb-text);
    font-size: 0.8125rem;
    font-weight: 500;
}
.history-dialog__help {
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    line-height: 1.6;
}
@media (max-width: 760px) {
    .history-page {
        padding-block: 2rem 3rem;
    }
    .history-header {
        align-items: start;
        gap: 1rem;
    }
    .history-header h1 {
        font-size: 1.75rem;
    }
    .history-filters {
        grid-template-columns: repeat(2, minmax(0, 1fr));
        padding: 1rem;
        gap: 0.875rem;
    }
    .history-privacy {
        padding-inline: 1rem;
    }
    .history-entry__header {
        flex-wrap: wrap;
        padding: 1rem;
        gap: 0.75rem;
    }
    .history-actions {
        width: 100%;
        justify-content: end;
    }
    .history-entry__metadata {
        grid-template-columns: repeat(2, minmax(0, 1fr));
        padding: 1rem;
        gap: 1rem;
    }
    .history-footer {
        align-items: start;
    }
}
@media (max-width: 380px) {
    .history-filters {
        grid-template-columns: 1fr;
    }
    .history-header {
        flex-wrap: wrap;
    }
    .history-header .fb-button {
        min-height: 2rem;
    }
    .history-entry__mark {
        width: 2.25rem;
        height: 2.25rem;
    }
    .history-status {
        margin-top: 0.125rem;
    }
}
</style>
