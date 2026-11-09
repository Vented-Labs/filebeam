<script setup lang="ts">
import BrandLogo from '../../brand/BrandLogo.vue';

defineProps<{ title: string; caption: string; description: string; tall?: boolean }>();
</script>

<template>
    <figure
        class="release-art"
        :class="{ 'release-art--tall': tall }"
        role="img"
        :aria-label="description"
    >
        <div class="release-art__copy" aria-hidden="true">
            <BrandLogo compact label="" />
            <strong>{{ title }}</strong>
            <span>{{ caption }}</span>
        </div>
        <div class="release-art__stage" aria-hidden="true"><slot /></div>
    </figure>
</template>

<style scoped>
/* Static editorial illustration; nothing inside is interactive. */
.release-art {
    position: relative;
    display: flex;
    height: 176px;
    align-items: center;
    margin: 25px 0 29px;
    overflow: hidden;
    border: 1px solid var(--cl-line);
    border-radius: 12px;
    background:
        radial-gradient(ellipse at 62% 60%, var(--cl-glow), transparent 73%),
        var(--fb-surface-sunken);
    container-type: inline-size;
    isolation: isolate;
}
.release-art--tall {
    height: 236px;
}
.release-art::before,
.release-art::after {
    content: '';
    position: absolute;
    border: 1px solid var(--cl-line);
    border-radius: 50%;
    transform: rotate(-24deg);
    pointer-events: none;
}
.release-art::before {
    top: -37px;
    right: -21px;
    width: 450px;
    height: 205px;
    opacity: 0.25;
}
.release-art::after {
    z-index: -1;
    top: -64px;
    right: -65px;
    width: 540px;
    height: 261px;
    opacity: 0.13;
}
.release-art__copy {
    z-index: 2;
    max-width: 222px;
    align-self: center;
    padding-left: 27px;
}
.release-art__copy :deep(.fb-brand) {
    padding: 0 0 13px;
}
.release-art__copy :deep(.fb-brand__glyph) {
    width: 26px;
    height: 32px;
}
.release-art__copy strong {
    display: block;
    color: var(--fb-text);
    font: 600 21px/1.3 var(--fb-font-brand);
    letter-spacing: -0.55px;
}
.release-art__copy span {
    display: block;
    max-width: 160px;
    margin-top: 5px;
    color: var(--fb-text-muted);
    font-size: 10px;
    line-height: 1.6;
}
.release-art__stage {
    position: absolute;
    inset: 0;
}
@container (max-width: 760px) {
    .release-art__copy {
        max-width: 181px;
        padding-left: 21px;
    }
    .release-art__copy strong {
        font-size: 19px;
    }
}
@container (max-width: 640px) {
    .release-art__copy {
        display: none;
    }
}
@media (max-width: 650px) {
    .release-art {
        margin: 20px -1px 24px;
    }
}
</style>
