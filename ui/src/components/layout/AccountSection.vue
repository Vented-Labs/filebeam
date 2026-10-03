<script setup lang="ts">
import Icon, { type IconName } from '../primitives/Icon.vue';
defineProps<{ title: string; description: string; icon: IconName }>();
</script>
<template>
    <section class="account-section">
        <header class="account-section__header">
            <span class="account-section__icon"><Icon :name="icon" :size="21" /></span>
            <div>
                <h2>{{ title }}</h2>
                <p>{{ description }}</p>
            </div>
            <slot name="actions" />
        </header>
        <div class="account-section__body"><slot /></div>
    </section>
</template>
<style scoped>
.account-section {
    min-width: 0;
    overflow: hidden;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
    box-shadow: var(--fb-shadow-panel);
}
.account-section__header {
    display: flex;
    align-items: center;
    gap: 0.875rem;
    margin-inline: 1.75rem;
    padding-block: 1.5rem 1.375rem;
    border-bottom: 1px solid var(--fb-line-strong);
}
.account-section__header > div {
    min-width: 0;
    flex: 1;
}
.account-section__icon {
    display: grid;
    width: 2.75rem;
    height: 2.75rem;
    flex: none;
    place-items: center;
    border: 1px solid var(--fb-card-border);
    border-radius: 0.75rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
h2 {
    margin: 0;
    font-size: 1.125rem;
    letter-spacing: -0.02em;
}
p {
    margin: 0.25rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
    line-height: 1.6;
}
.account-section__body {
    padding: 1.375rem 1.75rem 1.75rem;
}
@media (max-width: 560px) {
    .account-section__header {
        margin-inline: 1.375rem;
    }
    .account-section__body {
        padding-inline: 1.375rem;
    }
}
</style>
