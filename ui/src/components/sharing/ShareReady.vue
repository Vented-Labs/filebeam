<script setup lang="ts">
import Button from '../primitives/Button.vue';
import CopyButton from '../primitives/CopyButton.vue';
import Input from '../primitives/Input.vue';
import FilebeamIcon from '../primitives/FilebeamIcon.vue';
import TransferExpiry from '../download/TransferExpiry.vue';
import DownloadMonitor from './DownloadMonitor.vue';
import SmoothProgress from '../primitives/SmoothProgress.vue';
import type { DownloadSession, ShareResult } from '../../upload-types';
defineProps<{
    share: ShareResult;
    deleting: boolean;
    uploading?: boolean;
    progress?: number;
    activity?: string;
    sessions?: DownloadSession[];
    monitoringUnavailable?: boolean;
}>();
const emit = defineEmits<{ reset: []; delete: []; cancel: [] }>();
</script>

<template>
    <section
        class="share-ready mx-auto w-full max-w-2xl rounded-[var(--share-radius)] border border-[var(--fb-border)] bg-[var(--fb-surface)] p-6 text-center shadow-2xl shadow-black/30 sm:p-8"
    >
        <div
            class="mx-auto grid size-12 place-items-center rounded-full bg-[var(--fb-selected-surface)] text-2xl text-[var(--fb-success)]"
        >
            <FilebeamIcon :name="uploading ? 'loader' : 'check'" :size="24" />
        </div>
        <p
            v-if="share.turbo"
            class="mt-4 text-xs font-semibold uppercase tracking-[0.14em] text-[var(--fb-accent-text)]"
        >
            Turbo Transfer
        </p>
        <h1 class="mt-4 text-3xl font-semibold text-[var(--fb-text)]">
            Your encrypted link is ready
        </h1>
        <p v-if="share.turbo" class="mt-2 min-h-6 text-[var(--fb-text-muted)]" aria-live="polite">
            {{
                uploading
                    ? 'Uploading is still in progress. Keep this tab open until it finishes.'
                    : 'Upload complete. This link is ready to use.'
            }}
        </p>
        <p class="mt-2 min-h-12 text-[var(--fb-text-muted)]">
            {{
                share.includeKey
                    ? 'The key stays in the link fragment and is never sent to the server.'
                    : share.passwordProtected
                      ? 'Share the generated key and password separately.'
                      : 'Share the generated key separately from this link.'
            }}
        </p>
        <div v-if="share.turbo" class="mt-5 text-left">
            <SmoothProgress
                :value="uploading ? (progress ?? 0) : 100"
                :active="uploading"
                label="Turbo Transfer upload"
            >
                <template #label="{ percentage }">
                    <div
                        class="mb-2 flex items-center justify-between gap-3 text-sm text-[var(--fb-text-muted)]"
                    >
                        <span>{{
                            uploading ? (activity ?? 'Encrypting and uploading') : 'Upload complete'
                        }}</span
                        ><span class="tabular-nums">{{ percentage }}%</span>
                    </div>
                </template>
            </SmoothProgress>
        </div>
        <div class="mt-6 flex gap-2">
            <Input
                id="share-link"
                readonly
                :value="share.link"
                class="min-w-0 flex-1"
                aria-label="Share link"
            /><CopyButton :value="share.link" label="Copy link" :disabled="deleting" />
        </div>
        <div class="mt-3 flex gap-2">
            <Input
                id="generated-key"
                readonly
                :value="share.key"
                class="fb-code min-w-0 flex-1"
                aria-label="Generated decryption key"
            /><CopyButton
                :value="share.key"
                label="Copy key"
                variant="secondary"
                :disabled="deleting"
            />
        </div>
        <div class="mt-4 flex min-h-6 justify-center text-sm">
            <p v-if="uploading" class="text-[var(--fb-text-muted)]">
                Retention starts when uploading finishes.
            </p>
            <TransferExpiry v-else-if="share.expiresAt" :expires-at="share.expiresAt" />
        </div>
        <DownloadMonitor
            v-if="share.turbo"
            :sessions="sessions ?? []"
            :unavailable="monitoringUnavailable ?? false"
        />
        <div class="mt-6 flex justify-center gap-4">
            <Button variant="secondary" :disabled="uploading || deleting" @click="emit('reset')"
                >New transfer</Button
            ><Button v-if="uploading" variant="ghost" @click="emit('cancel')">Cancel upload</Button
            ><Button v-else variant="ghost" :disabled="deleting" @click="emit('delete')"
                >Delete now</Button
            >
        </div>
    </section>
</template>

<style scoped>
.share-ready {
    --share-radius: 1.75rem;
}
</style>
