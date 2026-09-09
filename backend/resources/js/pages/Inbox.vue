<script setup lang="ts">
import { Head, router } from '@inertiajs/vue3';
import { ref } from 'vue';
import { RouteSurface, type FilebeamConfig } from '@filebeam/ui';
import InboxController from '@/actions/App/Http/Controllers/InboxController';
import AppShell from '../../../../ui/src/components/layout/AppShell.vue';
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
        <AppShell
            :github-url="filebeam.github_url"
            :copyright-holder="filebeam.copyright_holder"
            :user="auth.user"
        >
            <section class="mx-auto max-w-4xl px-5 py-12 sm:px-8">
                <div class="flex items-center justify-between gap-4">
                    <h1 class="text-3xl font-semibold">Inbox</h1>
                    <AppLink href="/account" class="fb-text-link">Receiving settings</AppLink>
                </div>
                <p class="mt-3 text-[var(--fb-text-muted)]">
                    Received files stay encrypted until you unlock them. Filenames are never
                    included in notifications.
                </p>
                <p v-if="error" role="alert" class="mt-5 text-[var(--fb-danger)]">
                    {{ error }}
                </p>
                <div
                    v-if="!transfers.length"
                    class="mt-8 rounded-2xl border border-[var(--fb-border)] bg-[var(--fb-surface)] p-10 text-center"
                >
                    <Icon name="folder" :size="36" class="mx-auto" />
                    <h2 class="mt-4 text-xl font-semibold">No incoming files yet</h2>
                    <p class="mt-2 text-sm text-[var(--fb-text-muted)]">
                        Share your receiving page from your profile. Completed transfers will appear
                        here until they expire.
                    </p>
                </div>
                <ul v-else class="mt-8 space-y-3">
                    <li
                        v-for="transfer in transfers"
                        :key="transfer.id"
                        class="flex flex-wrap items-center justify-between gap-5 rounded-2xl border border-[var(--fb-border)] bg-[var(--fb-surface)] p-5 sm:p-6"
                    >
                        <div class="min-w-0">
                            <AppLink
                                :href="InboxController.show(transfer.id).url"
                                class="fb-text-link font-semibold"
                                >{{ transfer.item_count }} encrypted
                                {{ transfer.item_count === 1 ? 'file' : 'files' }}</AppLink
                            >
                            <p class="mt-2 text-sm text-[var(--fb-text-muted)]">
                                {{ formatBytes(transfer.ciphertext_bytes) }}. Received
                                {{ new Date(transfer.completed_at).toLocaleString() }}
                            </p>
                            <p class="mt-1 text-xs text-[var(--fb-text-muted)]">
                                Expires
                                {{ new Date(transfer.expires_at).toLocaleString() }}
                            </p>
                        </div>
                        <div
                            v-if="confirming === transfer.id"
                            class="flex flex-wrap items-center gap-2"
                        >
                            <span class="text-sm">Permanently delete?</span>
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
                    </li>
                </ul>
            </section>
        </AppShell>
    </div>
</template>
