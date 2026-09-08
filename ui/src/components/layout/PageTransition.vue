<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref } from 'vue';

defineProps<{ pageKey: string }>();
const surface = ref<HTMLElement>();
const changing = ref(false);
let targetHeight = 0;
let frame = 0;
let finishTimer: ReturnType<typeof setTimeout> | undefined;

function finish(): void {
    clearTimeout(finishTimer);
    if (surface.value) surface.value.style.height = 'auto';
    changing.value = false;
}

function beforeLeave(): void {
    clearTimeout(finishTimer);
    if (surface.value)
        surface.value.style.height = `${surface.value.getBoundingClientRect().height}px`;
    changing.value = true;
}

function enter(element: Element): void {
    void nextTick(() => {
        if (!surface.value) return;
        targetHeight = element.getBoundingClientRect().height;
        cancelAnimationFrame(frame);
        frame = requestAnimationFrame(() => {
            if (surface.value) surface.value.style.height = `${targetHeight}px`;
        });
        // Reduced motion and equal-height pages do not emit a height transition event.
        finishTimer = setTimeout(finish, 340);
    });
}

function transitionEnd(event: TransitionEvent): void {
    if (event.target === surface.value && event.propertyName === 'height') finish();
}

onBeforeUnmount(() => {
    cancelAnimationFrame(frame);
    clearTimeout(finishTimer);
});
</script>

<template>
    <div
        ref="surface"
        class="fb-page-transition"
        :data-changing="changing || undefined"
        @transitionend="transitionEnd"
    >
        <Transition name="fb-page" mode="out-in" @before-leave="beforeLeave" @enter="enter">
            <div :key="pageKey"><slot /></div>
        </Transition>
    </div>
</template>

<style scoped>
.fb-page-transition[data-changing] {
    overflow: clip;
    transition: height 280ms cubic-bezier(0.2, 0.75, 0.25, 1);
}
.fb-page-enter-active,
.fb-page-leave-active {
    transition:
        opacity 180ms ease,
        transform 180ms ease;
}
.fb-page-enter-from {
    opacity: 0;
    transform: translateY(6px);
}
.fb-page-leave-to {
    opacity: 0;
    transform: translateY(-4px);
}
@media (prefers-reduced-motion: reduce) {
    .fb-page-transition[data-changing],
    .fb-page-enter-active,
    .fb-page-leave-active {
        transition: none;
    }
    .fb-page-enter-from,
    .fb-page-leave-to {
        transform: none;
    }
}
</style>
