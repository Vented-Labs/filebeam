<script setup lang="ts">
import { ref } from 'vue';

withDefaults(
    defineProps<{
        invalid?: boolean;
        modelValue?: string | number;
        value?: string | number;
    }>(),
    { invalid: false },
);
const emit = defineEmits<{ 'update:modelValue': [value: string] }>();
const input = ref<HTMLInputElement>();
defineExpose({ focus: (options?: FocusOptions) => input.value?.focus(options) });
</script>

<template>
    <input
        ref="input"
        :value="modelValue ?? value ?? ''"
        class="fb-input"
        :aria-invalid="invalid || undefined"
        @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
    />
</template>
