<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps<{
    id: string;
    label: string;
    error?: string;
    description?: string;
    optional?: boolean;
}>();

const descriptionId = `${props.id}-description`;
const errorId = `${props.id}-error`;
const describedBy = computed(() =>
    [props.description && !props.error ? descriptionId : null, props.error ? errorId : null]
        .filter(Boolean)
        .join(' '),
);
</script>

<template>
    <div class="fb-form-field">
        <div class="fb-field-label-row">
            <label :for="id" class="fb-field-label"
                >{{ label
                }}<span v-if="optional" class="font-normal text-[var(--fb-text-muted)]">
                    (optional)</span
                ></label
            >
            <slot name="label-action" />
        </div>
        <slot :id="id" :described-by="describedBy || undefined" :invalid="Boolean(error)" />
        <p v-if="description && !error" :id="descriptionId" class="fb-field-description">
            {{ description }}
        </p>
        <p v-if="error" :id="errorId" class="fb-field-error" role="alert">
            {{ error }}
        </p>
    </div>
</template>
