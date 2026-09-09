<script setup lang="ts">
defineProps<{ show: boolean }>();

function hideOutgoing(element: Element): void {
    const reveal = element as HTMLElement;
    reveal.inert = true;
    reveal.setAttribute('aria-hidden', 'true');
}
</script>

<template>
    <Transition name="fb-reveal" @before-leave="hideOutgoing">
        <div v-if="show" class="fb-reveal">
            <div class="fb-reveal__inner"><slot /></div>
        </div>
    </Transition>
</template>

<style scoped>
.fb-reveal {
    display: grid;
    grid-template-rows: 1fr;
    opacity: 1;
    transform: translateY(0);
    transition:
        grid-template-rows var(--fb-duration-switch) var(--fb-ease),
        opacity var(--fb-duration-control) ease,
        transform var(--fb-duration-switch) var(--fb-ease);
}
.fb-reveal__inner {
    min-height: 0;
    overflow: hidden;
}
.fb-reveal-enter-from,
.fb-reveal-leave-to {
    grid-template-rows: 0fr;
    opacity: 0;
    transform: translateY(-5px);
}
@media (prefers-reduced-motion: reduce) {
    .fb-reveal {
        transition: none;
    }
}
</style>
