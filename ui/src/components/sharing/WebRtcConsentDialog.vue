<script setup lang="ts">
import {
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
} from 'reka-ui';
import Button from '../primitives/Button.vue';

defineProps<{ open: boolean }>();
const emit = defineEmits<{ accept: []; decline: [] }>();

function updateOpen(value: boolean): void {
    if (!value) emit('decline');
}
</script>

<template>
    <DialogRoot :open="open" @update:open="updateOpen">
        <DialogPortal>
            <DialogOverlay class="fb-dialog__overlay webrtc-consent__overlay" />
            <DialogContent class="fb-dialog__content webrtc-consent__dialog">
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
                    <Button variant="ghost" @click="emit('decline')">Not now</Button>
                    <Button @click="emit('accept')">Accept and continue</Button>
                </div>
            </DialogContent>
        </DialogPortal>
    </DialogRoot>
</template>

<style scoped>
.webrtc-consent__overlay {
    z-index: 60;
}
.webrtc-consent__dialog {
    z-index: 61;
    width: min(calc(100% - 2rem), 31rem);
    padding: 1.5rem;
}
</style>
