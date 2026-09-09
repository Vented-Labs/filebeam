<script setup lang="ts">
import { CheckboxIndicator, CheckboxRoot } from 'reka-ui';
import Icon from './Icon.vue';

withDefaults(defineProps<{ disabled?: boolean }>(), { disabled: false });
const checked = defineModel<boolean>({ default: false });
</script>

<template>
    <CheckboxRoot v-model="checked" :disabled="disabled" class="fb-checkbox">
        <CheckboxIndicator class="fb-checkbox-indicator">
            <Icon name="check" :size="13" />
        </CheckboxIndicator>
    </CheckboxRoot>
</template>
