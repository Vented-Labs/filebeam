<script setup lang="ts">
import { ref } from 'vue';
import {
    ContextMenuRoot,
    ContextMenuTrigger,
    ContextMenuPortal,
    ContextMenuContent,
    ContextMenuItem,
    DialogRoot,
    DialogPortal,
    DialogOverlay,
    DialogContent,
    DialogTitle,
    DialogDescription,
    DialogClose,
} from 'reka-ui';
import Button from '../primitives/Button.vue';
import { pastedContent, readClipboard, type PasteContent } from '../../lib/clipboard';

const props = defineProps<{ disabled: boolean }>();
const emit = defineEmits<{ paste: [content: PasteContent] }>();
const fallbackOpen = ref(false);
const reading = ref(false);
const message = ref('');

function accept(content: PasteContent): void {
    if (props.disabled) return;
    if (!content.files.length && !content.text) {
        message.value = 'The clipboard has no supported text or files.';
        return;
    }
    fallbackOpen.value = false;
    message.value = '';
    emit('paste', content);
}
async function paste(): Promise<void> {
    if (props.disabled || reading.value) return;
    reading.value = true;
    try {
        accept(await readClipboard());
    } catch {
        if (!props.disabled) fallbackOpen.value = true;
    } finally {
        reading.value = false;
    }
}
function nativePaste(event: ClipboardEvent): void {
    event.stopPropagation();
    if (!event.clipboardData) return;
    event.preventDefault();
    accept(pastedContent(event.clipboardData));
}
function openWithKeyboard(event: KeyboardEvent): void {
    if (props.disabled || !(event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')))
        return;
    event.preventDefault();
    const target = event.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    target.dispatchEvent(
        new MouseEvent('contextmenu', {
            bubbles: true,
            cancelable: true,
            clientX: rect.left + 16,
            clientY: rect.top + 16,
        }),
    );
}
</script>

<template>
    <ContextMenuRoot>
        <ContextMenuTrigger as-child :disabled="disabled">
            <div
                :tabindex="disabled ? -1 : 0"
                aria-label="File input and paste menu"
                @keydown="openWithKeyboard"
            >
                <slot />
                <div class="paste-input">
                    <Button variant="secondary" :disabled="disabled || reading" @click="paste"
                        >Paste</Button
                    >
                    <small>Paste text or images. Touch and hold here for Paste.</small>
                    <p v-if="message" role="status">{{ message }}</p>
                </div>
            </div>
        </ContextMenuTrigger>
        <ContextMenuPortal>
            <ContextMenuContent class="fb-select-content" :side-offset="4">
                <ContextMenuItem
                    class="fb-select-item"
                    :disabled="disabled || reading"
                    @select="paste"
                    >Paste</ContextMenuItem
                >
            </ContextMenuContent>
        </ContextMenuPortal>
    </ContextMenuRoot>
    <DialogRoot v-model:open="fallbackOpen">
        <DialogPortal>
            <DialogOverlay class="paste-overlay" />
            <DialogContent class="paste-dialog">
                <DialogTitle>Paste from your clipboard</DialogTitle>
                <DialogDescription
                    >Touch and hold the field and choose Paste, or use your keyboard paste shortcut.
                    If your browser cannot paste an image, use Choose files.</DialogDescription
                >
                <textarea
                    aria-label="Paste text or images here"
                    placeholder="Paste here"
                    @paste="nativePaste"
                />
                <DialogClose as-child><Button variant="secondary">Close</Button></DialogClose>
            </DialogContent>
        </DialogPortal>
    </DialogRoot>
</template>

<style scoped>
.paste-input {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
    padding: 0.75rem 1rem 1rem;
}
.paste-input small,
.paste-input p {
    color: var(--fb-text-muted);
}
.paste-overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: #0008;
}
.paste-dialog {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    z-index: 61;
    width: min(28rem, calc(100vw - 2rem));
    padding: 1.5rem;
    border-radius: 1rem;
    background: var(--fb-surface);
    color: var(--fb-text);
}
.paste-dialog textarea {
    display: block;
    width: 100%;
    min-height: 6rem;
    margin-block: 1rem;
    padding: 0.5rem;
    color: var(--fb-text);
    background: var(--fb-surface-sunken);
    font-size: 1rem;
}
</style>
