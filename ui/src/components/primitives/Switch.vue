<script setup lang="ts">
import { SwitchRoot, SwitchThumb } from 'reka-ui';

withDefaults(defineProps<{ disabled?: boolean }>(), { disabled: false });
const checked = defineModel<boolean>({ default: false });
</script>

<template>
    <SwitchRoot v-model="checked" :disabled="disabled" class="fb-switch">
        <SwitchThumb class="fb-switch-thumb" />
    </SwitchRoot>
</template>
