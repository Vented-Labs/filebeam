<script setup lang="ts">
defineProps<{ variant: 'home' | 'receive' | 'transfer' }>();

const copy = {
    home: { lines: ['Share files', 'privately'] },
    receive: { lines: ['Send files.', 'Securely.'] },
    transfer: { lines: ['Encrypted', 'transfer.'] },
};
</script>

<template>
    <article class="og-card" :aria-label="copy[variant].lines.join(' ')">
        <div class="og-orbit og-orbit-outer" aria-hidden="true"></div>
        <div class="og-orbit og-orbit-inner" aria-hidden="true"></div>
        <img class="og-logo" src="/brand/filebeam-logo-header.svg" alt="Filebeam" />
        <div class="og-copy">
            <h1 :class="{ 'og-headline-single': variant === 'home' }">
                <span v-for="line in copy[variant].lines" :key="line">{{ line }}</span>
            </h1>
            <p class="og-description">Securely send files and notes to others</p>
        </div>
        <img class="og-mark" src="/brand/filebeam-mark.svg" alt="" />
        <footer class="og-footer">
            <span class="og-lock" aria-hidden="true">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
                    <rect x="5" y="10" width="14" height="11" rx="3" />
                    <path d="M8 10V7a4 4 0 0 1 8 0v3M12 14v3" />
                </svg>
            </span>
            <span>End-to-end encrypted.</span>
        </footer>
    </article>
</template>

<style scoped>
.og-card {
    position: relative;
    box-sizing: border-box;
    width: 1200px;
    height: 630px;
    overflow: hidden;
    background:
        radial-gradient(ellipse at 88% 43%, rgb(139 53 255 / 24%), transparent 48%), var(--fb-bg);
    color: var(--fb-text);
    font-family: var(--fb-font-ui);
    -webkit-font-smoothing: antialiased;
}
.og-logo {
    position: absolute;
    top: 54px;
    left: 64px;
    width: 198px;
    height: 40px;
}
.og-copy {
    position: absolute;
    top: 193px;
    left: 64px;
    z-index: 1;
}
h1 {
    margin: 0;
    font-size: 64px;
    line-height: 1.1;
    font-weight: 750;
    letter-spacing: -3px;
}
h1 span {
    display: block;
}
h1 span + span {
    color: var(--fb-brand-fold);
}
.og-headline-single {
    display: flex;
    gap: 0.25em;
}
.og-description {
    margin: 27px 0 0;
    color: var(--fb-text-muted);
    font-size: 23px;
    line-height: 1.5;
    letter-spacing: -0.3px;
}
.og-mark {
    position: absolute;
    top: 154px;
    right: 78px;
    width: 260px;
    height: 330px;
    object-fit: contain;
    filter: drop-shadow(0 22px 48px rgb(139 53 255 / 30%));
}
.og-orbit {
    position: absolute;
    border: 1px solid rgb(199 91 250 / 12%);
    border-radius: 50%;
    transform: rotate(-25deg);
}
.og-orbit-outer {
    width: 570px;
    height: 760px;
    right: -106px;
    top: -64px;
}
.og-orbit-inner {
    width: 430px;
    height: 590px;
    right: -33px;
    top: 25px;
}
.og-footer {
    position: absolute;
    bottom: 49px;
    left: 64px;
    right: 64px;
    display: flex;
    align-items: center;
    gap: 12px;
    padding-top: 25px;
    border-top: 1px solid var(--fb-border);
    color: var(--fb-text-muted);
    font-size: 16px;
}
.og-lock {
    display: flex;
    color: var(--fb-accent-text);
}
.og-lock svg {
    width: 22px;
    height: 22px;
}
</style>
