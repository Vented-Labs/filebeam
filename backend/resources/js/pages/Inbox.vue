<script setup lang="ts">
import { Head, router } from '@inertiajs/vue3';
import { ref } from 'vue';
import { AnimatedReveal, RouteSurface, type FilebeamConfig } from '@filebeam/ui';
import InboxController from '@/actions/App/Http/Controllers/InboxController';
import AppLink from '../../../../ui/src/components/primitives/AppLink.vue';
import Button from '../../../../ui/src/components/primitives/Button.vue';
import Icon from '../../../../ui/src/components/primitives/Icon.vue';
import { formatBytes } from '../../../../ui/src/lib/format';
import { csrfHeaders } from '../../../../ui/src/lib/csrf';

defineOptions({ layout: RouteSurface });
defineProps<{
    transfers: Array<{
        id: string;
        ciphertext_bytes: number;
        item_count: number;
        completed_at: string;
        expires_at: string;
    }>;
    filebeam: FilebeamConfig;
    auth: { user: { name: string; username?: string | null } };
}>();
const removing = ref('');
const confirming = ref('');
const error = ref('');

async function remove(id: string): Promise<void> {
    if (removing.value) return;
    removing.value = id;
    error.value = '';
    try {
        const response = await fetch(InboxController.destroy(id).url, {
            method: 'DELETE',
            headers: csrfHeaders(),
        });
        if (!response.ok || response.redirected) throw new Error('Could not delete this transfer.');
        confirming.value = '';
        router.reload({ only: ['transfers', 'auth'] });
    } catch (reason) {
        error.value = reason instanceof Error ? reason.message : 'Could not delete this transfer.';
    } finally {
        removing.value = '';
    }
}
</script>

<template>
    <div>
        <Head title="Inbox" />
        <section class="inbox-page">
            <header class="inbox-page__header">
                <div>
                    <h1>Inbox</h1>
                </div>
                <AppLink href="/account" class="fb-text-link">Receiving settings</AppLink>
            </header>
            <p class="inbox-page__intro">
                Received files stay encrypted until you unlock them. Filenames are never included in
                notifications.
            </p>
            <AnimatedReveal :show="Boolean(error)">
                <p role="alert" class="inbox-page__error">{{ error }}</p>
            </AnimatedReveal>
            <div v-if="!transfers.length" class="inbox-empty">
                <span><Icon name="folder" :size="30" /></span>
                <h2>No incoming files yet</h2>
                <p>
                    Share your receiving page from your profile. Completed transfers will appear
                    here until they expire.
                </p>
            </div>
            <TransitionGroup v-else name="inbox-row" tag="ul" class="inbox-list">
                <li v-for="transfer in transfers" :key="transfer.id" class="inbox-row">
                    <span class="inbox-row__icon"><Icon name="lock" :size="18" /></span>
                    <div class="inbox-row__details">
                        <AppLink :href="InboxController.show(transfer.id).url" class="fb-text-link"
                            >{{ transfer.item_count }} encrypted
                            {{ transfer.item_count === 1 ? 'file' : 'files' }}</AppLink
                        >
                        <p>
                            {{ formatBytes(transfer.ciphertext_bytes) }}. Received
                            {{ new Date(transfer.completed_at).toLocaleString() }}
                        </p>
                        <small>
                            Expires
                            {{ new Date(transfer.expires_at).toLocaleString() }}
                        </small>
                    </div>
                    <div
                        class="inbox-row__action"
                        :class="{
                            'inbox-row__action--confirming': confirming === transfer.id,
                        }"
                    >
                        <Transition name="inbox-action">
                            <div v-if="confirming === transfer.id" class="inbox-row__confirm">
                                <span>Permanently delete?</span>
                                <Button
                                    variant="danger"
                                    :disabled="Boolean(removing)"
                                    @click="remove(transfer.id)"
                                    >Delete files</Button
                                >
                                <Button
                                    variant="ghost"
                                    :disabled="Boolean(removing)"
                                    @click="confirming = ''"
                                    >Cancel</Button
                                >
                            </div>
                            <Button
                                v-else
                                variant="ghost"
                                :aria-label="`Delete transfer ${transfer.id}`"
                                @click="confirming = transfer.id"
                                ><Icon name="trash" :size="18"
                            /></Button>
                        </Transition>
                    </div>
                </li>
            </TransitionGroup>
        </section>
    </div>
</template>

<style scoped>
.inbox-page {
    width: min(100% - 2rem, 58rem);
    margin-inline: auto;
    padding-block: 3.5rem 4.5rem;
}
.inbox-page__header {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 1.5rem;
}
.inbox-page__header h1 {
    margin: 0;
    font-size: 2rem;
    letter-spacing: -0.04em;
}
.inbox-page__intro {
    max-width: 42rem;
    margin: 0.75rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.875rem;
    line-height: 1.7;
}
.inbox-page__error {
    margin: 1.25rem 0 0;
    color: var(--fb-danger);
    font-size: 0.8125rem;
}
.inbox-empty {
    display: grid;
    min-height: 21rem;
    place-items: center;
    align-content: center;
    margin-top: 2rem;
    padding: 2rem;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
    box-shadow: var(--fb-shadow-panel);
    text-align: center;
}
.inbox-empty > span {
    display: grid;
    width: 3.75rem;
    height: 3.75rem;
    place-items: center;
    border: 1px solid #78598666;
    border-radius: 1rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.inbox-empty h2 {
    margin: 1rem 0 0;
    font-size: 1.25rem;
}
.inbox-empty p {
    max-width: 31rem;
    margin: 0.5rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
    line-height: 1.65;
}
.inbox-list {
    display: grid;
    gap: 0.625rem;
    margin: 2rem 0 0;
    padding: 0;
    list-style: none;
}
.inbox-row {
    position: relative;
    display: grid;
    grid-template-columns: 2.75rem minmax(0, 1fr) auto;
    align-items: center;
    gap: 1rem;
    min-height: 5.5rem;
    padding: 0.875rem 1rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.875rem;
    background: var(--fb-surface);
    box-shadow: var(--fb-shadow-panel);
}
.inbox-row__icon {
    display: grid;
    width: 2.75rem;
    height: 2.75rem;
    place-items: center;
    border-radius: 0.75rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.inbox-row__details {
    min-width: 0;
}
.inbox-row__details .fb-text-link {
    font-size: 0.875rem;
    font-weight: 650;
}
.inbox-row__details p,
.inbox-row__details small {
    display: block;
    margin: 0.375rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
}
.inbox-row__details small {
    color: var(--fb-text-subtle);
}
.inbox-row__confirm {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.75rem;
}
.inbox-row__action {
    position: relative;
    display: flex;
    min-width: 17rem;
    min-height: 2.75rem;
    align-items: center;
    justify-content: flex-end;
}
.inbox-action-enter-active,
.inbox-action-leave-active {
    transition:
        opacity var(--fb-duration-switch) var(--fb-ease),
        transform var(--fb-duration-switch) var(--fb-ease),
        filter var(--fb-duration-switch) var(--fb-ease);
}
.inbox-action-leave-active {
    position: absolute;
    right: 0;
    pointer-events: none;
}
.inbox-action-enter-from {
    opacity: 0;
    filter: blur(2px);
    transform: translateX(8px);
}
.inbox-action-leave-to {
    opacity: 0;
    filter: blur(2px);
    transform: translateX(-8px);
}
.inbox-row-enter-active,
.inbox-row-leave-active {
    transition:
        opacity var(--fb-duration-control) var(--fb-ease),
        transform var(--fb-duration-control) var(--fb-ease);
}
.inbox-row-leave-active {
    position: absolute;
}
.inbox-row-enter-from,
.inbox-row-leave-to {
    opacity: 0;
    transform: translateY(10px);
}
@media (max-width: 640px) {
    .inbox-page {
        padding-block: 2rem 3rem;
    }
    .inbox-row {
        grid-template-columns: 2.5rem minmax(0, 1fr) auto;
        gap: 0.75rem;
        padding-inline: 0.75rem;
    }
    .inbox-row__icon {
        width: 2.5rem;
        height: 2.5rem;
    }
    .inbox-row__confirm {
        grid-column: 1 / -1;
        justify-content: flex-end;
    }
    .inbox-row__action {
        width: 2.75rem;
        min-width: 2.75rem;
    }
    .inbox-row__action--confirming {
        position: absolute;
        z-index: 2;
        inset: 0;
        width: 100%;
        box-sizing: border-box;
        padding: 0.75rem;
        border-radius: inherit;
        background: var(--fb-surface);
    }
}
@media (prefers-reduced-motion: reduce) {
    .inbox-row-enter-active,
    .inbox-row-leave-active,
    .inbox-action-enter-active,
    .inbox-action-leave-active {
        transition: none;
    }
}
</style>
