<script setup lang="ts">
withDefaults(
    defineProps<{
        invalid?: boolean;
        modelValue?: string | number;
        value?: string | number;
    }>(),
    { invalid: false },
);
const emit = defineEmits<{ 'update:modelValue': [value: string] }>();
</script>

<template>
    <input
        :value="modelValue ?? value ?? ''"
        class="fb-input"
        :aria-invalid="invalid || undefined"
        @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
    />
</template>
