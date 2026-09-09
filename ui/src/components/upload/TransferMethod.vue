<script setup lang="ts">
import type { TransferDriver, TransferLimits } from '../../types';
import { formatBytes } from '../../lib/format';
import Icon from '../primitives/Icon.vue';

defineProps<{
    enabledDrivers: TransferDriver[];
    webRtcSupported: boolean;
    disabled: boolean;
    limits: TransferLimits;
    mode: 'files' | 'note';
}>();

const driver = defineModel<TransferDriver>({ required: true });
</script>

<template>
    <section class="transfer-method" aria-label="Transfer method">
        <fieldset
            class="transfer-method__choices"
            :class="{ 'transfer-method__choices--single': enabledDrivers.length === 1 }"
            aria-label="Transfer method"
        >
            <legend class="sr-only">Transfer method</legend>
            <label
                v-if="enabledDrivers.includes('http')"
                class="transfer-method__card"
                :class="{
                    'transfer-method__card--selected': driver === 'http',
                    'transfer-method__card--disabled': disabled,
                }"
            >
                <input
                    v-model="driver"
                    type="radio"
                    name="transfer-method"
                    value="http"
                    :disabled="disabled"
                    aria-label="HTTP (stored)"
                />
                <Icon name="archive" :size="22" />
                <span><strong>HTTP</strong><small>Download later.</small></span>
            </label>
            <label
                v-if="enabledDrivers.includes('webrtc')"
                class="transfer-method__card"
                :class="{
                    'transfer-method__card--selected': driver === 'webrtc',
                    'transfer-method__card--disabled': disabled || !webRtcSupported,
                }"
            >
                <input
                    v-model="driver"
                    type="radio"
                    name="transfer-method"
                    value="webrtc"
                    :disabled="disabled || !webRtcSupported"
                    aria-label="WebRTC (live)"
                />
                <Icon name="bolt" :size="22" />
                <span><strong>WebRTC</strong><small>Keep tabs open.</small></span>
            </label>
        </fieldset>
        <p
            v-if="enabledDrivers.includes('webrtc') && !webRtcSupported"
            class="transfer-method__unsupported"
        >
            WebRTC is not supported by this browser. Choose HTTP to share.
        </p>
        <p class="transfer-method__limits" aria-live="polite">
            {{
                mode === 'files'
                    ? limits.maximum_transfer_bytes === null
                        ? 'Unlimited transfer size'
                        : `${formatBytes(limits.maximum_transfer_bytes)} per transfer`
                    : limits.maximum_note_bytes === null
                      ? 'Unlimited note size'
                      : `${formatBytes(limits.maximum_note_bytes)} per note`
            }}
            <template v-if="mode === 'files'">
                &middot;
                {{
                    limits.maximum_file_count === null
                        ? 'Unlimited files'
                        : `Up to ${limits.maximum_file_count} files`
                }}</template
            >
        </p>
    </section>
</template>

<style scoped>
.transfer-method {
    min-width: 0;
    padding: 0.5rem;
    border: 1px solid var(--fb-border);
    border-radius: 1rem;
    background: var(--fb-surface);
}
.transfer-method__choices {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.625rem;
    margin: 0;
    padding: 0;
    border: 0;
}
.transfer-method__choices--single {
    grid-template-columns: 1fr;
}
.transfer-method__card {
    position: relative;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    gap: 0.625rem;
    min-height: 3.5rem;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--fb-control-border);
    border-radius: 0.875rem;
    background: var(--fb-surface-raised);
    color: var(--fb-text-muted);
    cursor: pointer;
    transition:
        border-color var(--fb-duration-fast) ease,
        background var(--fb-duration-fast) ease;
}
.transfer-method__card:has(input:focus-visible) {
    outline: 2px solid var(--fb-focus);
    outline-offset: 2px;
}
.transfer-method__card--selected {
    border-color: var(--fb-focus);
    background: var(--fb-selected-surface);
    color: var(--fb-text);
}
.transfer-method__card--disabled {
    cursor: not-allowed;
    opacity: 0.5;
}
.transfer-method__card input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    opacity: 0;
    cursor: inherit;
}
.transfer-method__card strong,
.transfer-method__card small {
    display: block;
}
.transfer-method__card strong {
    color: var(--fb-text);
    font-size: 0.875rem;
}
.transfer-method__card small {
    margin-top: 0.15rem;
    font-size: 0.75rem;
    line-height: 1rem;
}
.transfer-method__unsupported {
    margin-top: 0.5rem;
    color: var(--fb-danger);
    font-size: 0.75rem;
}
.transfer-method__limits {
    margin-top: 0.5rem;
    padding: 0.5rem 0.25rem 0;
    border-top: 1px solid var(--fb-border);
    color: var(--fb-text-muted);
    font-size: 0.75rem;
}
@media (max-width: 380px) {
    .transfer-method__card {
        gap: 0.5rem;
        padding: 0.5rem;
    }
    .transfer-method__card small {
        font-size: 0.6875rem;
        line-height: 0.9rem;
    }
}
</style>
