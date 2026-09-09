<script setup lang="ts">
import { computed } from 'vue';
import { RadioGroupItem, RadioGroupRoot } from 'reka-ui';
import type { TransferDriver, TransferLimits } from '../../types';
import { formatBytes } from '../../lib/format';
import Icon from '../primitives/Icon.vue';
import AnimatedReveal from '../layout/AnimatedReveal.vue';

const props = defineProps<{
    enabledDrivers: TransferDriver[];
    webRtcSupported: boolean;
    disabled: boolean;
    limits: TransferLimits;
    mode: 'files' | 'note';
}>();

const driver = defineModel<TransferDriver>({ required: true });
const visibleDrivers = computed(() =>
    (['http', 'webrtc'] as const).filter((item) => props.enabledDrivers.includes(item)),
);
const selectedIndex = computed(() => Math.max(0, visibleDrivers.value.indexOf(driver.value)));
const indicatorStyle = computed(() => ({
    '--driver-count': visibleDrivers.value.length,
    '--driver-index': selectedIndex.value,
}));

function selectWithKeyboard(event: KeyboardEvent): void {
    if (props.disabled || (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight')) return;
    const selectable = visibleDrivers.value.filter(
        (item) => item !== 'webrtc' || props.webRtcSupported,
    );
    if (selectable.length < 2) return;
    const current = Math.max(0, selectable.indexOf(driver.value));
    const direction =
        getComputedStyle(event.currentTarget as HTMLElement).direction === 'rtl' ? -1 : 1;
    const delta = event.key === 'ArrowRight' ? direction : -direction;
    // Reka moves roving focus for controlled radios but does not update this model on arrow keys.
    driver.value = selectable[(current + delta + selectable.length) % selectable.length]!;
}

const limitLabel = computed(() => {
    const maximum =
        props.mode === 'files'
            ? props.limits.maximum_transfer_bytes
            : props.limits.maximum_note_bytes;
    if (maximum === null)
        return props.mode === 'files' ? 'Unlimited transfer size' : 'Unlimited note size';
    return `${formatBytes(maximum)} per ${props.mode === 'files' ? 'transfer' : 'note'}`;
});
const countLabel = computed(() =>
    props.limits.maximum_file_count === null
        ? 'Unlimited files'
        : `Up to ${props.limits.maximum_file_count} files`,
);

function hideOutgoing(element: Element): void {
    const detail = element as HTMLElement;
    detail.inert = true;
    detail.setAttribute('aria-hidden', 'true');
}
</script>

<template>
    <section
        class="transfer-method"
        data-testid="prism-transport-rail"
        aria-label="Transfer method"
    >
        <RadioGroupRoot
            v-model="driver"
            class="transfer-method__choices"
            :class="{ 'transfer-method__choices--single': visibleDrivers.length === 1 }"
            :disabled="disabled"
            orientation="horizontal"
            aria-label="Transfer method"
            @keydown="selectWithKeyboard"
        >
            <span
                class="transfer-method__indicator"
                data-testid="prism-driver-indicator"
                aria-hidden="true"
                :style="indicatorStyle"
            />
            <RadioGroupItem
                v-for="item in visibleDrivers"
                :key="item"
                :value="item"
                class="transfer-method__card"
                :disabled="disabled || (item === 'webrtc' && !webRtcSupported)"
                :aria-label="item === 'http' ? 'HTTP (stored)' : 'WebRTC (live)'"
            >
                <span class="transfer-method__icon" aria-hidden="true">
                    <Icon :name="item === 'http' ? 'archive' : 'bolt'" :size="18" />
                </span>
                <span class="transfer-method__copy">
                    <strong>{{ item === 'http' ? 'HTTP' : 'WebRTC' }}</strong>
                    <small>{{ item === 'http' ? 'Download later.' : 'Keep tabs open.' }}</small>
                </span>
            </RadioGroupItem>
        </RadioGroupRoot>
        <div class="transfer-method__context">
            <div class="transfer-method__limit-stack" aria-live="polite" aria-atomic="true">
                <Transition name="transfer-method-detail" @before-leave="hideOutgoing">
                    <p :key="limitLabel" class="transfer-method__limits">
                        <Icon name="storage" :size="17" aria-hidden="true" />
                        {{ limitLabel }}
                    </p>
                </Transition>
            </div>
            <AnimatedReveal data-testid="prism-file-count-reveal" :show="mode === 'files'">
                <div class="transfer-method__count-stack">
                    <Transition name="transfer-method-detail" @before-leave="hideOutgoing">
                        <p :key="countLabel" class="transfer-method__count">{{ countLabel }}</p>
                    </Transition>
                </div>
            </AnimatedReveal>
        </div>
        <AnimatedReveal
            class="transfer-method__unsupported-reveal"
            :show="enabledDrivers.includes('webrtc') && !webRtcSupported"
        >
            <p class="transfer-method__unsupported">
                WebRTC is not supported by this browser. Choose HTTP to share.
            </p>
        </AnimatedReveal>
    </section>
</template>

<style scoped>
.transfer-method {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 1.25rem;
    min-width: 0;
    padding: 1.25rem 1.5rem;
    border-bottom: 1px solid #ffffff08;
    background: #ffffff01;
}
.transfer-method__choices {
    --driver-gap: 0.25rem;
    position: relative;
    isolation: isolate;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--driver-gap);
    width: min(27.75rem, 100%);
    flex: none;
    padding: 0.25rem;
    border: 1px solid #322c3d;
    border-radius: 0.9375rem;
    background: var(--fb-surface-sunken);
    box-shadow: inset 0 1px 2px #0002;
}
.transfer-method__choices--single {
    grid-template-columns: minmax(0, 1fr);
}
.transfer-method__indicator {
    position: absolute;
    z-index: -1;
    top: 0.25rem;
    bottom: 0.25rem;
    left: 0.25rem;
    width: calc(
        (100% - 0.5rem - (var(--driver-count) - 1) * var(--driver-gap)) / var(--driver-count)
    );
    border: 1px solid #806191;
    border-radius: 0.6875rem;
    background: #32253f;
    box-shadow: inset 0 1px 0 #ffffff0a;
    pointer-events: none;
    transform: translateX(calc(var(--driver-index) * (100% + var(--driver-gap))));
    transition: transform var(--fb-duration-selection) var(--fb-ease);
}
.transfer-method__card {
    display: flex;
    align-items: center;
    gap: 0.6875rem;
    min-width: 0;
    height: 3.5625rem;
    padding: 0.5rem 0.875rem;
    border: 0;
    border-radius: 0.6875rem;
    color: var(--fb-text-muted);
    background: transparent;
    text-align: left;
    cursor: pointer;
    transition:
        background var(--fb-duration-control) ease,
        transform var(--fb-duration-control) var(--fb-ease);
}
.transfer-method__card:hover:not([data-disabled]) {
    background: #ffffff05;
}
.transfer-method__card:active:not([data-disabled]) {
    background: #0002;
    transform: translateY(1px) scale(0.982);
}
.transfer-method__card:focus-visible {
    outline: 2px solid var(--fb-focus);
    outline-offset: 2px;
}
.transfer-method__card[data-state='checked'] {
    color: var(--fb-text);
}
.transfer-method__card[data-disabled] {
    cursor: not-allowed;
    opacity: 0.45;
}
.transfer-method__icon {
    display: grid;
    width: 2rem;
    height: 2rem;
    flex: none;
    place-items: center;
    border-radius: 0.5rem;
    color: #9e90af;
    background: #ffffff04;
    transition:
        color var(--fb-duration-control) ease,
        background var(--fb-duration-control) ease;
}
.transfer-method__card[data-state='checked'] .transfer-method__icon {
    color: #d4b3fa;
    background: #b18ac116;
}
.transfer-method__copy {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 0.0625rem;
}
.transfer-method__copy strong {
    color: inherit;
    font-size: 0.75rem;
    font-weight: 600;
    line-height: 1.4;
}
.transfer-method__copy small {
    color: var(--fb-text-muted);
    font-size: 0.625rem;
    line-height: 1.4;
    white-space: nowrap;
}
.transfer-method__context {
    display: flex;
    min-width: 10.9375rem;
    flex: none;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.25rem;
}
.transfer-method__limit-stack,
.transfer-method__count-stack {
    display: grid;
}
.transfer-method__limits {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 0.375rem;
    color: #cfc3df;
    font-size: 0.75rem;
    font-weight: 500;
    text-align: right;
}
.transfer-method__limit-stack > *,
.transfer-method__count-stack > * {
    grid-area: 1 / 1;
}
.transfer-method__limits :deep(.fb-icon) {
    color: var(--fb-accent-text);
}
.transfer-method__count,
.transfer-method__unsupported {
    color: var(--fb-text-subtle);
    font-size: 0.625rem;
}
.transfer-method__unsupported {
    max-width: 15rem;
    color: var(--fb-danger);
    text-align: right;
}
.transfer-method__unsupported-reveal {
    width: 100%;
}
.transfer-method-detail-enter-active,
.transfer-method-detail-leave-active {
    transition:
        opacity var(--fb-duration-control) var(--fb-ease),
        transform var(--fb-duration-switch) var(--fb-ease);
}
.transfer-method-detail-enter-from {
    opacity: 0;
    transform: translateY(0.375rem);
}
.transfer-method-detail-leave-to {
    opacity: 0;
    transform: translateY(-0.375rem);
}
@media (max-width: 730px) {
    .transfer-method {
        align-items: stretch;
        flex-direction: column;
        gap: 0.6875rem;
        padding: 0.9375rem;
    }
    .transfer-method__choices {
        width: 100%;
    }
    .transfer-method__card {
        height: 3.4375rem;
        gap: 0.625rem;
        padding: 0.4375rem 0.6875rem;
    }
    .transfer-method__context {
        min-width: 0;
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
        padding-inline: 0.25rem;
    }
    .transfer-method__unsupported {
        max-width: none;
        text-align: left;
    }
}
@media (max-width: 380px) {
    .transfer-method {
        padding: 0.75rem;
    }
    .transfer-method__card {
        gap: 0.4375rem;
        padding-inline: 0.5625rem;
    }
    .transfer-method__icon {
        width: 1.75rem;
    }
    .transfer-method__copy small {
        font-size: 0.5625rem;
    }
}
@media (prefers-reduced-motion: reduce) {
    .transfer-method__indicator,
    .transfer-method__card,
    .transfer-method-detail-enter-active,
    .transfer-method-detail-leave-active {
        transition: none;
    }
    .transfer-method__card:active:not([data-disabled]) {
        transform: none;
    }
}
</style>
