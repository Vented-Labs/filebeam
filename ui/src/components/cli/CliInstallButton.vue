<script setup lang="ts">
import { inject } from 'vue';
import Button from '../primitives/Button.vue';
import Icon from '../primitives/Icon.vue';
import { cliInstallKey } from './context';

const openInstall = inject(cliInstallKey);
</script>

<template>
    <Button v-if="openInstall" variant="ghost" class="cli-install-button" @click="openInstall">
        <Icon name="code" :size="16" />Install CLI
    </Button>
</template>

<style scoped>
.cli-install-button {
    flex: none;
    white-space: nowrap;
}
</style>
