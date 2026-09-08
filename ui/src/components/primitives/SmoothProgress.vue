<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';

const props = withDefaults(
    defineProps<{
        value: number;
        label: string;
        size?: 'small' | 'default';
        active?: boolean;
    }>(),
    { size: 'default', active: true },
);

const target = computed(() => clamp(props.value));
const displayed = ref(0);
const percentage = computed(() =>
    displayed.value > 0 && displayed.value < 1 ? '<1' : Math.floor(displayed.value),
);
const visible = ref(false);
const allowsMotion = ref(false);
const shining = computed(
    () =>
        props.active &&
        target.value > 0 &&
        target.value < 100 &&
        visible.value &&
        allowsMotion.value,
);
let frame = 0;
let startedAt = 0;
let from = 0;
let reducedMotion: MediaQueryList | undefined;

function clamp(value: number): number {
    return Number.isFinite(value) ? Math.min(100, Math.max(0, value)) : 0;
}

function stop(): void {
    cancelAnimationFrame(frame);
    frame = 0;
}

function easeOut(value: number): number {
    return 1 - (1 - value) ** 3;
}

function animate(now: number): void {
    const difference = target.value - from;
    if (difference <= 0 || document.visibilityState !== 'visible' || reducedMotion?.matches) {
        displayed.value = target.value;
        frame = 0;
        return;
    }
    const duration = Math.min(1800, Math.max(600, difference * 32));
    const elapsed = Math.min(1, (now - startedAt) / duration);
    // The interpolation is bounded by the latest server or worker progress target.
    displayed.value = Math.min(target.value, from + difference * easeOut(elapsed));
    if (elapsed < 1 && displayed.value < target.value) frame = requestAnimationFrame(animate);
    else {
        displayed.value = target.value;
        frame = 0;
    }
}

function move(): void {
    stop();
    if (
        target.value <= displayed.value ||
        document.visibilityState !== 'visible' ||
        reducedMotion?.matches
    ) {
        displayed.value = target.value;
        return;
    }
    from = displayed.value;
    startedAt = performance.now();
    frame = requestAnimationFrame(animate);
}

function syncMotion(): void {
    allowsMotion.value = !reducedMotion?.matches;
    if (reducedMotion?.matches) {
        stop();
        displayed.value = target.value;
    } else move();
}

function syncVisibility(): void {
    visible.value = document.visibilityState === 'visible';
    if (document.visibilityState !== 'visible') stop();
    else {
        displayed.value = target.value;
        move();
    }
}

watch(target, (next, previous) => {
    if (next < previous || next < displayed.value) {
        stop();
        displayed.value = next;
        return;
    }
    move();
});

onMounted(() => {
    reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
    allowsMotion.value = !reducedMotion.matches;
    visible.value = document.visibilityState === 'visible';
    reducedMotion.addEventListener('change', syncMotion);
    document.addEventListener('visibilitychange', syncVisibility);
    move();
});

onBeforeUnmount(() => {
    stop();
    reducedMotion?.removeEventListener('change', syncMotion);
    document.removeEventListener('visibilitychange', syncVisibility);
});
</script>

<template>
    <div
        class="smooth-progress"
        :class="[`smooth-progress--${size}`, { 'smooth-progress--shining': shining }]"
        role="progressbar"
        :aria-label="label"
        :aria-valuemin="0"
        :aria-valuemax="100"
        :aria-valuenow="percentage"
        :aria-valuetext="`${label}: ${percentage}%`"
    >
        <slot name="label" :percentage="percentage" />
        <div class="smooth-progress__track">
            <div
                class="smooth-progress__fill"
                :style="{ transform: `scaleX(${displayed / 100})` }"
            />
        </div>
    </div>
</template>

<style scoped>
.smooth-progress__track {
    height: 0.375rem;
    overflow: hidden;
    border-radius: 999px;
    background: var(--fb-border);
}
.smooth-progress--small .smooth-progress__track {
    height: 0.25rem;
}
.smooth-progress__fill {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    transform-origin: left center;
    border-radius: inherit;
    background: var(--fb-brand-gradient);
    will-change: transform;
}
.smooth-progress__fill::after {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(
        105deg,
        transparent 20%,
        rgb(255 255 255 / 18%) 40%,
        rgb(255 255 255 / 75%) 50%,
        rgb(255 255 255 / 18%) 60%,
        transparent 80%
    );
    opacity: 0;
    transform: translateX(-100%);
    pointer-events: none;
}
.smooth-progress--shining .smooth-progress__fill::after {
    animation: progress-sheen 4.8s ease-in-out infinite;
}
@keyframes progress-sheen {
    0%,
    55% {
        opacity: 0;
        transform: translateX(-100%);
    }
    65% {
        opacity: 0.85;
    }
    85%,
    100% {
        opacity: 0;
        transform: translateX(100%);
    }
}
@media (prefers-reduced-motion: reduce) {
    .smooth-progress__fill::after {
        animation: none;
    }
}
</style>
