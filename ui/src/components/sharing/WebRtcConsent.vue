<script lang="ts">
let acceptedInSession = false;
</script>

<script setup lang="ts">
import {
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
} from 'reka-ui';
import { onBeforeUnmount, onMounted, ref } from 'vue';
import Button from '../primitives/Button.vue';

const consent = defineModel<boolean>({ required: true });
const open = ref(false);
let resolveRequest: ((accepted: boolean) => void) | undefined;
let pendingRequest: Promise<boolean> | undefined;

function hasAcceptedCookie(): boolean {
    return document.cookie.split('; ').includes('webRTCRiskAccepted=1');
}
function persistAcceptance(): void {
    const secure = location.protocol === 'https:' ? '; Secure' : '';
    document.cookie = `webRTCRiskAccepted=1; Path=/; Max-Age=31536000; SameSite=Lax${secure}`;
}
function accept(): void {
    persistAcceptance();
    acceptedInSession = true;
    consent.value = true;
    open.value = false;
    resolve(true);
}
function decline(): void {
    open.value = false;
    resolve(false);
}
function updateOpen(value: boolean): void {
    open.value = value;
    if (!value && !consent.value) {
        resolve(false);
    }
}
function resolve(accepted: boolean): void {
    resolveRequest?.(accepted);
    resolveRequest = undefined;
    pendingRequest = undefined;
}
function requestConsent(): Promise<boolean> {
    if (hasAcceptedCookie() || acceptedInSession) {
        consent.value = true;
        return Promise.resolve(true);
    }
    if (pendingRequest) return pendingRequest;
    consent.value = false;
    open.value = true;
    pendingRequest = new Promise((resolve) => {
        resolveRequest = resolve;
    });
    return pendingRequest;
}

onMounted(() => {
    if (hasAcceptedCookie() || acceptedInSession) {
        consent.value = true;
    }
});
onBeforeUnmount(() => resolve(false));
defineExpose({ requestConsent });
</script>

<template>
    <DialogRoot :open="open" @update:open="updateOpen">
        <DialogPortal>
            <DialogOverlay class="webrtc-consent__overlay" />
            <DialogContent class="webrtc-consent__dialog">
                <DialogTitle class="text-xl font-semibold text-[var(--fb-text)]">
                    WebRTC privacy
                </DialogTitle>
                <DialogDescription class="mt-3 text-sm leading-6 text-[var(--fb-text-muted)]">
                    WebRTC connects your browser to the other participant and can expose your IP
                    address to them. ICE connection services may also see your IP address and
                    network information. Both browsers must remain open. Filebeam does not store the
                    transferred content. Encryption stays in your browser.
                </DialogDescription>
                <div class="mt-6 flex flex-wrap justify-end gap-3">
                    <Button variant="ghost" @click="decline">Not now</Button>
                    <Button @click="accept">Accept and continue</Button>
                </div>
            </DialogContent>
        </DialogPortal>
    </DialogRoot>
</template>

<style scoped>
.webrtc-consent__overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: color-mix(in srgb, black 62%, transparent);
    animation: consent-fade-in 180ms ease-out;
}
.webrtc-consent__dialog {
    position: fixed;
    top: 50%;
    left: 50%;
    z-index: 51;
    width: min(calc(100% - 2rem), 31rem);
    transform: translate(-50%, -50%);
    border: 1px solid var(--fb-border);
    border-radius: 1rem;
    background: var(--fb-surface);
    padding: 1.5rem;
    box-shadow: 0 25px 60px rgb(0 0 0 / 0.38);
    animation: consent-dialog-in 220ms cubic-bezier(0.2, 0.8, 0.2, 1);
}
.webrtc-consent__overlay[data-state='closed'] {
    animation: consent-fade-out 160ms ease-in;
}
.webrtc-consent__dialog[data-state='closed'] {
    animation: consent-dialog-out 160ms ease-in;
}
@keyframes consent-fade-in {
    from {
        opacity: 0;
    }
    to {
        opacity: 1;
    }
}
@keyframes consent-fade-out {
    from {
        opacity: 1;
    }
    to {
        opacity: 0;
    }
}
@keyframes consent-dialog-in {
    from {
        opacity: 0;
        transform: translate(-50%, calc(-50% + 0.75rem)) scale(0.98);
    }
    to {
        opacity: 1;
        transform: translate(-50%, -50%) scale(1);
    }
}
@keyframes consent-dialog-out {
    from {
        opacity: 1;
        transform: translate(-50%, -50%) scale(1);
    }
    to {
        opacity: 0;
        transform: translate(-50%, calc(-50% + 0.5rem)) scale(0.985);
    }
}
@media (prefers-reduced-motion: reduce) {
    .webrtc-consent__overlay,
    .webrtc-consent__dialog {
        animation: none;
    }
}
</style>
