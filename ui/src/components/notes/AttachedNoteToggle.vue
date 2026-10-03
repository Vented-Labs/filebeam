<script setup lang="ts">
import Icon from '../primitives/Icon.vue';
import Switch from '../primitives/Switch.vue';

withDefaults(defineProps<{ disabled?: boolean; compact?: boolean }>(), {
    disabled: false,
    compact: false,
});
const enabled = defineModel<boolean>({ default: false });
</script>

<template>
    <label
        class="attached-note-toggle"
        :class="{ 'attached-note-toggle--compact': compact }"
        :data-enabled="enabled || undefined"
        :data-disabled="disabled || undefined"
    >
        <span class="attached-note-toggle__mark"><Icon name="note" :size="17" /></span>
        <span class="attached-note-toggle__copy">
            <span class="attached-note-toggle__title">Attach note</span>
            <span v-if="!compact" class="attached-note-toggle__description">
                {{
                    enabled
                        ? 'Encrypted with your files. Same password and expiry.'
                        : 'Add a message or instructions alongside your files.'
                }}
            </span>
        </span>
        <Switch v-model="enabled" :disabled="disabled" aria-label="Attach note" />
    </label>
</template>

<style scoped>
.attached-note-toggle {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.75rem;
    padding: 0.875rem 1.5rem;
    border-top: 1px solid var(--fb-note-border);
    background: var(--fb-surface);
    cursor: pointer;
}
.attached-note-toggle__mark {
    display: flex;
    width: 2rem;
    height: 2rem;
    flex: none;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--fb-note-border);
    border-radius: 0.625rem;
    color: var(--fb-accent-text);
    background: var(--fb-note-surface);
}
.attached-note-toggle__copy {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 0.1875rem;
}
.attached-note-toggle__title {
    color: var(--fb-text);
    font-size: 0.8125rem;
    font-weight: 600;
}
.attached-note-toggle__description {
    color: var(--fb-text-muted);
    font-size: 0.6875rem;
    line-height: 1.5;
}
.attached-note-toggle--compact {
    min-height: 2.75rem;
    box-sizing: border-box;
    gap: 0.625rem;
    padding: 0.5rem 0.875rem;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-sm);
    background: var(--fb-surface-raised);
}
.attached-note-toggle--compact .attached-note-toggle__mark {
    width: auto;
    height: auto;
    border: 0;
    background: transparent;
}
.attached-note-toggle--compact .attached-note-toggle__title {
    font-size: 0.75rem;
    white-space: nowrap;
}
.attached-note-toggle[data-enabled] {
    background: var(--fb-note-surface);
}
.attached-note-toggle--compact:hover:not([data-disabled]) {
    border-color: var(--fb-note-border);
    background: var(--fb-note-surface);
}
.attached-note-toggle[data-disabled] {
    cursor: default;
    opacity: 0.65;
}
@media (max-width: 730px) {
    .attached-note-toggle:not(.attached-note-toggle--compact) {
        padding-inline: 1rem;
    }
}
</style>
