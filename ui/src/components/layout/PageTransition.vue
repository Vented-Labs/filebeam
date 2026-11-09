<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref } from 'vue';

defineProps<{ pageKey: string }>();
const surface = ref<HTMLElement>();
const changing = ref(false);
const tall = ref(false);
let frame = 0;
let finishTimer: ReturnType<typeof setTimeout> | undefined;
let transitionId = 0;

function finish(id = transitionId): void {
    if (id !== transitionId) return;
    clearTimeout(finishTimer);
    if (surface.value) surface.value.style.height = 'auto';
    changing.value = false;
    tall.value = false;
}

function beforeLeave(element: Element): void {
    transitionId++;
    clearTimeout(finishTimer);
    cancelAnimationFrame(frame);
    if (surface.value)
        surface.value.style.height = `${surface.value.getBoundingClientRect().height}px`;
    const pane = element as HTMLElement;
    pane.inert = true;
    pane.setAttribute('aria-hidden', 'true');
    changing.value = true;
}

function enter(element: Element): void {
    const id = transitionId;
    void nextTick(() => {
        if (!surface.value || id !== transitionId) return;
        const targetHeight = element.getBoundingClientRect().height;
        const startHeight = surface.value.getBoundingClientRect().height;
        // Animating height across long pages relayouts the whole document every frame and
        // drags the footer through the viewport; long pages only cross-fade.
        if (Math.max(targetHeight, startHeight) > window.innerHeight) {
            tall.value = true;
            surface.value.style.height = 'auto';
            finishTimer = setTimeout(() => finish(id), 320);
            return;
        }
        cancelAnimationFrame(frame);
        frame = requestAnimationFrame(() => {
            if (surface.value && id === transitionId)
                surface.value.style.height = `${targetHeight}px`;
        });
        finishTimer = setTimeout(() => finish(id), 520);
    });
}

function entered(): void {
    finish();
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
        :data-tall="tall || undefined"
        @transitionend="transitionEnd"
    >
        <Transition
            name="fb-page"
            @before-leave="beforeLeave"
            @enter="enter"
            @after-enter="entered"
        >
            <div :key="pageKey" class="fb-page-pane"><slot /></div>
        </Transition>
    </div>
</template>

<style scoped>
.fb-page-transition[data-changing] {
    position: relative;
    isolation: isolate;
    overflow: clip;
    transition: height var(--fb-duration-pane) var(--fb-ease);
}
.fb-page-pane {
    width: 100%;
}
.fb-page-enter-active,
.fb-page-leave-active {
    transition:
        opacity var(--fb-duration-pane) var(--fb-ease),
        transform var(--fb-duration-pane) var(--fb-ease);
}
.fb-page-enter-active {
    position: relative;
    z-index: 2;
}
.fb-page-leave-active {
    position: absolute;
    z-index: 0;
    inset: 0 0 auto;
    width: 100%;
    pointer-events: none;
    visibility: hidden;
}
.fb-page-enter-from {
    opacity: 0;
    transform: translateY(16px);
}
.fb-page-leave-to {
    opacity: 0;
    transform: translateY(-10px);
}
/* Long content pages: a short, composited fade with a slight rise and no height animation. */
.fb-page-transition[data-tall] {
    overflow: visible;
    transition: none;
}
.fb-page-transition[data-tall] .fb-page-enter-active {
    transition:
        opacity 300ms cubic-bezier(0.25, 1, 0.5, 1),
        transform 300ms cubic-bezier(0.25, 1, 0.5, 1);
    will-change: opacity, transform;
}
.fb-page-transition[data-tall] .fb-page-enter-from {
    transform: translateY(6px);
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
