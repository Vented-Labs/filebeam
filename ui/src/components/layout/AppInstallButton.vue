<script setup lang="ts">
import { defineAsyncComponent, nextTick, onMounted, ref } from 'vue';
import Icon from '../primitives/Icon.vue';
import { browserPlatformDetails, isMobileDevice } from '../../lib/platform';
import { restoreDialogFocus } from '../../lib/dialog-focus';

defineOptions({ inheritAttrs: false });
const DesktopInstallDialog = defineAsyncComponent(
    () => import('../desktop/DesktopInstallDialog.vue'),
);
const visible = ref(false);
const open = ref(false);
const loaded = ref(false);
const trigger = ref<HTMLButtonElement>();
onMounted(() => {
    visible.value = !isMobileDevice(browserPlatformDetails());
});
function showInstall(): void {
    loaded.value = true;
    open.value = true;
}
function restoreFocus(event: Event): void {
    event.preventDefault();
    void nextTick(() => restoreDialogFocus(trigger.value));
}
</script>

<template>
    <button
        v-if="visible"
        ref="trigger"
        v-bind="$attrs"
        type="button"
        aria-haspopup="dialog"
        :aria-expanded="open"
        class="fb-button fb-button--ghost app-install-entry"
        data-app-install-entry
        @click="showInstall"
    >
        <Icon name="monitor" :size="17" />
        Install Desktop App
    </button>
    <DesktopInstallDialog v-if="loaded" v-model:open="open" @close-auto-focus="restoreFocus" />
</template>

<style scoped>
.app-install-entry {
    flex: none;
    white-space: nowrap;
    border: 1px solid var(--fb-border);
    background: var(--fb-surface);
    font-size: 0.75rem;
}
.app-install-entry :deep(.fb-icon) {
    color: var(--fb-text);
}
@media (max-width: 900px) {
    .app-install-entry {
        display: none;
    }
}
</style>
