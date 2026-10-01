<script setup lang="ts">
import { nextTick, provide, ref } from 'vue';
import type { CliConfig } from '../../types';
import CliInstallDialog from './CliInstallDialog.vue';
import { cliInstallKey } from './context';
import { restoreDialogFocus } from '../../lib/dialog-focus';

defineProps<{ config?: CliConfig }>();
const open = ref(false);
let trigger: HTMLElement | undefined;

provide(cliInstallKey, (event) => {
    trigger = event.currentTarget as HTMLElement;
    open.value = true;
});

function restoreFocus(event: Event): void {
    event.preventDefault();
    void nextTick(() => restoreDialogFocus(trigger));
}
</script>

<template>
    <slot />
    <CliInstallDialog v-model:open="open" :config="config" @close-auto-focus="restoreFocus" />
</template>
