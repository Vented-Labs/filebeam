<script setup lang="ts">
import { computed } from 'vue';
import AnimatedReveal from '../layout/AnimatedReveal.vue';

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
        <AnimatedReveal :show="Boolean(error || description)" class="fb-field-message">
            <Transition name="fb-field-copy">
                <p v-if="error" :id="errorId" key="error" class="fb-field-error" role="alert">
                    {{ error }}
                </p>
                <p v-else :id="descriptionId" key="description" class="fb-field-description">
                    {{ description }}
                </p>
            </Transition>
        </AnimatedReveal>
    </div>
</template>

<style scoped>
.fb-field-message :deep(.fb-reveal__inner) {
    position: relative;
}
.fb-field-copy-enter-active,
.fb-field-copy-leave-active {
    transition:
        opacity var(--fb-duration-switch) var(--fb-ease),
        transform var(--fb-duration-switch) var(--fb-ease);
}
.fb-field-copy-leave-active {
    position: absolute;
    inset: 0;
    width: 100%;
    pointer-events: none;
}
.fb-field-copy-enter-from {
    opacity: 0;
    transform: translateY(4px);
}
.fb-field-copy-leave-to {
    opacity: 0;
    transform: translateY(-4px);
}
@media (prefers-reduced-motion: reduce) {
    .fb-field-copy-enter-active,
    .fb-field-copy-leave-active {
        transition: none;
    }
}
</style>
