<script setup lang="ts">
defineProps<{ title: string; description: string }>();
</script>
<template>
    <section class="account-page">
        <header class="account-page__header">
            <div>
                <h1>{{ title }}</h1>
                <p>{{ description }}</p>
            </div>
            <div class="account-page__actions"><slot name="actions" /></div>
        </header>
        <slot />
    </section>
</template>
<style scoped>
.account-page {
    width: min(100% - 2rem, 72rem);
    margin-inline: auto;
    padding-block: 3.5rem 4.5rem;
    color: var(--fb-text);
}
.account-page__header {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 2rem;
    margin-bottom: 1.5rem;
}
h1 {
    margin: 0;
    font-size: 2.25rem;
    letter-spacing: -0.04em;
}
p {
    margin: 0.625rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
    line-height: 1.65;
}
.account-page__actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.625rem;
    flex-shrink: 0;
}
@media (max-width: 640px) {
    .account-page {
        padding-block: 2rem 3rem;
    }
    .account-page__header {
        align-items: start;
        flex-direction: column;
        gap: 1rem;
    }
    h1 {
        font-size: 2rem;
    }
}
</style>
