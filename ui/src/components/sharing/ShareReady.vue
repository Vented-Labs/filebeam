<script setup lang="ts">
import Button from '../primitives/Button.vue';
import CopyButton from '../primitives/CopyButton.vue';
import Input from '../primitives/Input.vue';
import Icon from '../primitives/Icon.vue';
import TransferExpiry from '../download/TransferExpiry.vue';
import DownloadMonitor from './DownloadMonitor.vue';
import SmoothProgress from '../primitives/SmoothProgress.vue';
import type { DownloadSession, ShareResult, TransferMode } from '../../upload-types';
import CliDownloadCard from '../cli/CliDownloadCard.vue';

defineProps<{
    share: ShareResult;
    mode: TransferMode;
    deleting: boolean;
    uploading?: boolean;
    progress?: number;
    activity?: string;
    sessions?: DownloadSession[];
    monitoringUnavailable?: boolean;
    canRestartHttp?: boolean;
}>();
const emit = defineEmits<{ reset: []; delete: []; cancel: []; restartHttp: [] }>();
</script>

<template>
    <section class="share-ready">
        <div class="share-ready__emblem">
            <Icon :name="uploading ? 'loader' : 'check'" :size="26" />
        </div>
        <p v-if="share.turbo" class="share-ready__eyebrow">Turbo Transfer</p>
        <p v-if="share.driver === 'webrtc'" class="share-ready__eyebrow">WebRTC live transfer</p>
        <h1>
            {{
                share.driver === 'webrtc'
                    ? 'Your live transfer is ready'
                    : 'Your encrypted link is ready'
            }}
        </h1>
        <p v-if="share.turbo" class="share-ready__status" aria-live="polite">
            {{
                uploading
                    ? 'Uploading is still in progress. Keep this tab open until it finishes.'
                    : 'Upload complete. This link is ready to use.'
            }}
        </p>
        <p v-if="share.driver === 'webrtc'" class="share-ready__status">
            Keep this tab open while recipients download. No payload is stored by Filebeam.
        </p>
        <p v-if="share.driver === 'webrtc'" class="share-ready__activity" aria-live="polite">
            {{ activity ?? 'Waiting for a recipient to connect.' }}
            <span v-if="progress !== undefined" class="tabular-nums"
                >{{ Math.round(progress) }}%</span
            >
        </p>
        <p class="share-ready__key-note">
            {{
                share.includeKey
                    ? 'The key stays in the link fragment and is never sent to the server.'
                    : share.passwordProtected
                      ? 'Share the generated key and password separately.'
                      : 'Share the generated key separately from this link.'
            }}
        </p>
        <div v-if="share.turbo" class="share-ready__progress">
            <SmoothProgress
                :value="uploading ? (progress ?? 0) : 100"
                :active="uploading"
                label="Turbo Transfer upload"
            >
                <template #label="{ percentage }">
                    <div class="share-ready__progress-label">
                        <span>{{
                            uploading ? (activity ?? 'Encrypting and uploading') : 'Upload complete'
                        }}</span>
                        <span class="tabular-nums">{{ percentage }}%</span>
                    </div>
                </template>
            </SmoothProgress>
        </div>
        <div class="share-ready__field">
            <Icon name="link" :size="15" />
            <Input id="share-link" readonly :value="share.link" aria-label="Share link" />
            <CopyButton :value="share.link" label="Copy link" :disabled="deleting" />
        </div>
        <CliDownloadCard
            class="share-ready__cli"
            :target="share.link"
            :transfer="{
                kind: mode,
                driver: share.driver ?? 'http',
                available: !deleting && !uploading,
                expiresAt: share.expiresAt,
            }"
            :password-protected="share.passwordProtected"
        />
        <div class="share-ready__field">
            <Icon name="key" :size="15" />
            <Input
                id="generated-key"
                readonly
                :value="share.key"
                class="fb-code"
                aria-label="Generated decryption key"
            />
            <CopyButton
                :value="share.key"
                label="Copy key"
                variant="secondary"
                :disabled="deleting"
            />
        </div>
        <div v-if="share.driver !== 'webrtc'" class="share-ready__expiry">
            <Transition name="share-copy">
                <p v-if="uploading" key="uploading">Retention starts when uploading finishes.</p>
                <TransferExpiry
                    v-else-if="share.expiresAt"
                    key="expiry"
                    :expires-at="share.expiresAt"
                />
            </Transition>
        </div>
        <DownloadMonitor
            v-if="share.turbo"
            :sessions="sessions ?? []"
            :unavailable="monitoringUnavailable ?? false"
        />
        <section
            v-if="share.driver === 'webrtc'"
            class="share-ready__recipients"
            aria-live="polite"
        >
            <h2>Recipient activity</h2>
            <p v-if="!sessions?.length">Waiting for a recipient to connect.</p>
            <TransitionGroup v-else name="recipient" tag="ul">
                <li v-for="session in sessions" :key="session.id">
                    <span class="share-ready__recipient-icon">
                        <Icon
                            :name="session.status === 'completed' ? 'check' : 'user'"
                            :size="17"
                        />
                    </span>
                    <span>Recipient {{ session.status }}</span>
                    <span class="tabular-nums">{{ Math.round(session.progress) }}%</span>
                </li>
            </TransitionGroup>
        </section>
        <div class="share-ready__actions">
            <Button variant="secondary" :disabled="uploading || deleting" @click="emit('reset')">
                <Icon name="plus" :size="15" />New transfer
            </Button>
            <Button
                v-if="uploading || share.driver === 'webrtc'"
                variant="ghost"
                @click="emit('cancel')"
            >
                {{ share.driver === 'webrtc' ? 'Stop live transfer' : 'Cancel upload' }}
            </Button>
            <Button
                v-if="share.driver === 'webrtc' && canRestartHttp"
                variant="secondary"
                :disabled="deleting"
                @click="emit('restartHttp')"
            >
                Restart as stored HTTP
            </Button>
            <Button
                v-else-if="!uploading && share.driver !== 'webrtc'"
                variant="ghost"
                :disabled="deleting"
                @click="emit('delete')"
            >
                Delete now
            </Button>
        </div>
    </section>
</template>

<style scoped>
.share-ready {
    display: flex;
    min-height: 21.875rem;
    box-sizing: border-box;
    align-items: center;
    flex-direction: column;
    padding: 2rem 2rem 1.625rem;
    background: var(--fb-surface);
    text-align: center;
}
.share-ready__emblem {
    display: grid;
    width: 3.8125rem;
    height: 3.8125rem;
    place-items: center;
    border: 1px solid #78598666;
    border-radius: 999px;
    color: #d4b6f3;
    background: radial-gradient(circle at 30% 0%, #a679ff28, #342740);
}
.share-ready__eyebrow {
    margin: 0.875rem 0 0;
    color: var(--fb-accent-text);
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
}
.share-ready h1 {
    margin: 0.75rem 0 0;
    color: var(--fb-text);
    font-size: 1.6875rem;
    font-weight: 560;
    letter-spacing: -0.035em;
}
.share-ready__status,
.share-ready__activity,
.share-ready__key-note {
    max-width: 39rem;
    margin: 0.5625rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    line-height: 1.7;
}
.share-ready__activity {
    color: var(--fb-accent-text);
}
.share-ready__activity span {
    margin-left: 0.375rem;
}
.share-ready__key-note {
    min-height: 1.25rem;
    color: var(--fb-text-subtle);
}
.share-ready__progress {
    width: min(39rem, 100%);
    margin-top: 1rem;
    text-align: left;
}
.share-ready__progress-label {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.5rem;
    color: var(--fb-text-muted);
    font-size: 0.6875rem;
}
.share-ready__field {
    display: flex;
    width: min(39.375rem, 100%);
    min-height: 3.125rem;
    box-sizing: border-box;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.75rem;
    padding: 0.375rem 0.375rem 0.375rem 0.875rem;
    border: 1px solid #554060;
    border-radius: 0.75rem;
    color: var(--fb-text-subtle);
    background: var(--fb-surface-sunken);
}
.share-ready__cli {
    max-width: 39.375rem;
}
.share-ready__field .fb-input {
    min-width: 0;
    flex: 1;
    padding: 0;
    border: 0;
    background: transparent;
    font-size: 0.6875rem;
}
.share-ready__field :deep(.fb-button) {
    min-height: 2.3125rem;
    border-radius: 0.5rem;
    font-size: 0.6875rem;
}
.share-ready__expiry {
    position: relative;
    min-height: 1.5rem;
    margin-top: 1rem;
    color: var(--fb-text-muted);
    font-size: 0.6875rem;
}
.share-ready__expiry p {
    margin: 0;
}
.share-copy-enter-active,
.share-copy-leave-active {
    transition:
        opacity var(--fb-duration-switch) var(--fb-ease),
        transform var(--fb-duration-switch) var(--fb-ease);
}
.share-copy-leave-active {
    position: absolute;
    inset: 0;
    width: 100%;
    pointer-events: none;
}
.share-copy-enter-from {
    opacity: 0;
    transform: translateY(5px);
}
.share-copy-leave-to {
    opacity: 0;
    transform: translateY(-5px);
}
.share-ready__recipients {
    width: min(39.375rem, 100%);
    margin-top: 1.25rem;
    padding-top: 1rem;
    border-top: 1px solid #ffffff08;
    text-align: left;
}
.share-ready__recipients h2 {
    margin: 0;
    color: var(--fb-text);
    font-size: 0.75rem;
    font-weight: 600;
}
.share-ready__recipients > p {
    margin: 0.75rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.6875rem;
}
.share-ready__recipients ul {
    display: grid;
    gap: 0.375rem;
    margin: 0.75rem 0 0;
    padding: 0;
    list-style: none;
}
.share-ready__recipients li {
    display: grid;
    grid-template-columns: 1.75rem minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.625rem;
    min-height: 3rem;
    padding: 0.5rem 0.75rem;
    border: 1px solid #ffffff08;
    border-radius: 0.625rem;
    color: var(--fb-text-muted);
    background: var(--fb-surface-sunken);
    font-size: 0.6875rem;
    overflow: hidden;
    max-height: 4rem;
}
.share-ready__recipient-icon {
    display: grid;
    width: 1.75rem;
    height: 1.75rem;
    place-items: center;
    border-radius: 0.5rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.share-ready__actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 0.75rem;
    margin-top: 1.5rem;
}
.recipient-enter-active,
.recipient-leave-active {
    transition:
        opacity var(--fb-duration-control) ease,
        transform var(--fb-duration-control) var(--fb-ease),
        max-height var(--fb-duration-switch) var(--fb-ease),
        min-height var(--fb-duration-switch) var(--fb-ease),
        padding-block var(--fb-duration-switch) var(--fb-ease);
}
.recipient-enter-from,
.recipient-leave-to {
    min-height: 0;
    max-height: 0;
    padding-block: 0;
    opacity: 0;
    transform: translateX(18px);
}
@media (max-width: 640px) {
    .share-ready {
        padding: 1.75rem 1rem 1.375rem;
    }
    .share-ready__field {
        align-items: stretch;
        flex-wrap: wrap;
    }
    .share-ready__field > .fb-icon {
        align-self: center;
    }
    .share-ready__field :deep(.copy-button) {
        width: 100%;
    }
    .share-ready__field :deep(.fb-button) {
        width: 100%;
    }
}
@media (prefers-reduced-motion: reduce) {
    .recipient-enter-active,
    .recipient-leave-active,
    .share-copy-enter-active,
    .share-copy-leave-active {
        transition: none;
    }
}
</style>
