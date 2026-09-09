<script lang="ts">
let acceptedInSession = false;
</script>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';
import WebRtcConsentDialog from './WebRtcConsentDialog.vue';

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
    <WebRtcConsentDialog :open="open" @accept="accept" @decline="decline" />
</template>
