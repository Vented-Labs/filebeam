<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue';

const outer = ref<HTMLElement>();
const inner = ref<HTMLElement>();
let observer: ResizeObserver | undefined;
let reducedMotion: MediaQueryList | undefined;
let initialized = false;
let frame = 0;

function updateHeight(): void {
    if (!outer.value || !inner.value) return;
    const height = inner.value.getBoundingClientRect().height;
    // out-in leaves one empty frame between panels; keep the previous height until the next panel mounts.
    if (initialized && height === 0) return;

    if (!initialized) {
        outer.value.style.height = `${height}px`;
        initialized = true;
        return;
    }

    if (reducedMotion?.matches) {
        outer.value.style.height = 'auto';
        return;
    }

    const currentHeight = outer.value.getBoundingClientRect().height;
    if (Math.abs(currentHeight - height) < 1) return;
    outer.value.style.height = `${currentHeight}px`;
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
        if (outer.value) outer.value.style.height = `${height}px`;
    });
}

function onTransitionEnd(event: TransitionEvent): void {
    if (
        event.target === outer.value &&
        event.propertyName === 'height' &&
        outer.value &&
        inner.value
    )
        outer.value.style.height = `${inner.value.getBoundingClientRect().height}px`;
}

function onMotionChange(): void {
    if (outer.value && inner.value)
        outer.value.style.height = `${inner.value.getBoundingClientRect().height}px`;
}

onMounted(async () => {
    reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
    reducedMotion.addEventListener('change', onMotionChange);
    await nextTick();
    updateHeight();
    observer = new ResizeObserver(updateHeight);
    if (inner.value) observer.observe(inner.value);
});

onBeforeUnmount(() => {
    cancelAnimationFrame(frame);
    observer?.disconnect();
    reducedMotion?.removeEventListener('change', onMotionChange);
});
</script>

<template>
    <div ref="outer" class="animated-height" @transitionend="onTransitionEnd">
        <div ref="inner"><slot /></div>
    </div>
</template>

<style scoped>
.animated-height {
    overflow: clip;
    transition: height 280ms cubic-bezier(0.2, 0.75, 0.25, 1);
}
@media (prefers-reduced-motion: reduce) {
    .animated-height {
        transition: none;
    }
}
</style>
