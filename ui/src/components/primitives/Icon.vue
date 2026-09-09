<script setup lang="ts">
import { computed, useAttrs, watchEffect } from 'vue';
import { iconMarkup, type IconName } from './icons.generated';

export type { IconName } from './icons.generated';

defineOptions({ inheritAttrs: false });

const props = defineProps<{
    name: IconName;
    size?: number;
    label?: string;
}>();

const attrs = useAttrs();
const markup = computed(() =>
    Object.hasOwn(iconMarkup, props.name) ? iconMarkup[props.name] : undefined,
);
const renderedSize = computed(() =>
    typeof props.size === 'number' && Number.isFinite(props.size) && props.size > 0
        ? props.size
        : 20,
);
const accessibleLabel = computed(() =>
    typeof props.label === 'string' && props.label.trim() ? props.label : undefined,
);

if (import.meta.env.DEV) {
    watchEffect(() => {
        if (!markup.value) console.warn(`[Filebeam] Unknown icon name: ${String(props.name)}`);
    });
}
</script>

<template>
    <svg
        v-if="markup"
        class="fb-icon"
        :class="[attrs.class, { 'fb-icon--loader': name === 'loader' }]"
        :style="attrs.style"
        :width="renderedSize"
        :height="renderedSize"
        viewBox="0 0 24 24"
        fill="none"
        focusable="false"
        :aria-hidden="accessibleLabel ? undefined : true"
        :role="accessibleLabel ? 'img' : undefined"
        :aria-label="accessibleLabel"
    >
        <g v-html="markup" />
    </svg>
</template>

<style scoped>
.fb-icon {
    flex-shrink: 0;
}
.fb-icon--loader {
    animation: fb-icon-spin 900ms linear infinite;
}
@keyframes fb-icon-spin {
    to {
        transform: rotate(360deg);
    }
}
@media (prefers-reduced-motion: reduce) {
    .fb-icon--loader {
        animation: none;
    }
}
</style>
