<script setup lang="ts">
import { CheckboxIndicator, CheckboxRoot } from 'reka-ui';
import FilebeamIcon from './FilebeamIcon.vue';

withDefaults(defineProps<{ disabled?: boolean }>(), { disabled: false });
const checked = defineModel<boolean>({ default: false });
</script>

<template>
    <CheckboxRoot v-model="checked" :disabled="disabled" class="fb-checkbox">
        <CheckboxIndicator class="fb-checkbox-indicator">
            <FilebeamIcon name="check" :size="13" :stroke-width="2.4" />
        </CheckboxIndicator>
    </CheckboxRoot>
</template>
