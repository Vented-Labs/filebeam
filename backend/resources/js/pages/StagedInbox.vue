<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import { onMounted, ref } from 'vue';
import { RouteSurface } from '@filebeam/ui';
import {
    browserStagedTransfers,
    dismissBrowserStaging,
} from '../../../../ui/src/lib/inbox-staging';
import AppLink from '../../../../ui/src/components/primitives/AppLink.vue';
import Button from '../../../../ui/src/components/primitives/Button.vue';
import type { Transfer } from '../../../../ui/src/composables/useEncryptedDownload';
import AccountPage from '../../../../ui/src/components/layout/AccountPage.vue';
import AccountSection from '../../../../ui/src/components/layout/AccountSection.vue';
import Icon from '../../../../ui/src/components/primitives/Icon.vue';
import { formatBytes } from '../../../../ui/src/lib/format';
defineOptions({ layout: RouteSurface });
const props = defineProps<{ auth: { user: { id: number } } }>();
const staged = ref<Array<{ id: string; transfer: Transfer }>>([]);
const error = ref('');
async function load(): Promise<void> {
    try {
        staged.value = await browserStagedTransfers(props.auth.user.id);
    } catch {
        error.value = 'Browser staging is unavailable.';
    }
}
async function remove(id: string): Promise<void> {
    try {
        await dismissBrowserStaging(props.auth.user.id, id);
        await load();
    } catch {
        error.value = 'Could not remove local ciphertext.';
    }
}
onMounted(load);
</script>
<template>
    <AccountPage
        title="Privately staged files"
        description="Downloaded ciphertext stays in this browser until you unlock, verify, and save it."
    >
        <Head title="Staged files" />
        <template #actions
            ><AppLink href="/account/inbox" class="fb-button fb-button--secondary"
                ><Icon name="folder" :size="16" />Back to inbox</AppLink
            ></template
        >
        <AccountSection
            title="Ready to unlock"
            description="Fully staged files remain available after server expiry."
            icon="lock"
        >
            <p v-if="error" role="alert">{{ error }}</p>
            <div v-if="!staged.length" class="staged-empty">
                <Icon name="download" :size="30" />
                <h3>No completed automatic downloads in this browser.</h3>
                <p>
                    Enable private staging in your inbox to catch up on eligible friend deliveries.
                </p>
            </div>
            <article v-for="item in staged" :key="item.id">
                <span class="staged-icon"><Icon name="lock" :size="20" /></span>
                <div class="staged-details">
                    <AppLink :href="`/account/inbox/staged/${item.id}`" class="fb-text-link"
                        >{{ item.transfer.items.length }} encrypted files — unlock and save</AppLink
                    >
                    <p>
                        {{ formatBytes(item.transfer.ciphertext_bytes) }} · Encrypted and stored
                        locally
                    </p>
                </div>
                <Button variant="ghost" @click="remove(item.id)"
                    ><Icon name="trash" :size="16" />Remove local ciphertext</Button
                >
            </article>
        </AccountSection>
    </AccountPage>
</template>
<style scoped>
p {
    color: var(--fb-text-muted);
    line-height: 1.6;
}
article {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 1rem;
    padding: 1rem;
    margin-top: 1rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.875rem;
    background: var(--fb-surface-sunken);
}
.staged-icon {
    display: grid;
    width: 2.75rem;
    height: 2.75rem;
    place-items: center;
    border-radius: 0.75rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.staged-details {
    min-width: 0;
    flex: 1;
    font-size: 0.875rem;
}
.staged-details p {
    margin: 0.375rem 0 0;
    font-size: 0.75rem;
}
.staged-empty {
    display: grid;
    justify-items: center;
    padding-block: 3rem;
    color: var(--fb-accent-text);
    text-align: center;
}
.staged-empty h3 {
    margin: 1rem 0 0;
    color: var(--fb-text);
    font-size: 1.125rem;
}
.staged-empty p {
    max-width: 28rem;
    margin: 0.5rem 0 0;
    font-size: 0.8125rem;
}
@media (max-width: 640px) {
    article > .fb-button {
        width: 100%;
    }
}
</style>
