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
    <section class="staged-inbox">
        <Head title="Staged files" />
        <h1>Privately staged files</h1>
        <p>
            Ciphertext is stored in this browser. Unlock and verify files before saving them. Fully
            staged files remain available after server expiry.
        </p>
        <AppLink href="/account/inbox" class="fb-text-link">Back to inbox</AppLink>
        <p v-if="error" role="alert">{{ error }}</p>
        <p v-if="!staged.length">No completed automatic downloads in this browser.</p>
        <article v-for="item in staged" :key="item.id">
            <AppLink :href="`/account/inbox/staged/${item.id}`" class="fb-text-link"
                >{{ item.transfer.items.length }} encrypted files — unlock and save</AppLink
            >
            <Button variant="ghost" @click="remove(item.id)">Remove local ciphertext</Button>
        </article>
    </section>
</template>
<style scoped>
.staged-inbox {
    width: min(100% - 2rem, 58rem);
    margin-inline: auto;
    padding-block: 3rem;
}
p {
    color: var(--fb-text-muted);
    line-height: 1.6;
}
article {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 1rem;
    margin-top: 1rem;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
}
</style>
