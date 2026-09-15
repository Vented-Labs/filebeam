<script setup lang="ts">
import { computed } from 'vue';
import Icon from '../primitives/Icon.vue';

const props = defineProps<{ destination?: string }>();
// A platform-neutral install page owns platform/capability/installed-state resolution.
// Layout labels must never select a binary based on viewport width.
const destination = computed(() => {
    const configured = props.destination ?? import.meta.env.VITE_APP_INSTALL_URL;
    if (!configured) return undefined;
    try {
        const url = new URL(configured, window.location.origin);
        return url.protocol === 'https:' ||
            (url.origin === window.location.origin && url.protocol === 'http:')
            ? url.href
            : undefined;
    } catch {
        return undefined;
    }
});
</script>

<template>
    <component
        :is="destination ? 'a' : 'button'"
        :href="destination"
        :type="destination ? undefined : 'button'"
        :aria-disabled="destination ? undefined : true"
        :title="destination ? undefined : 'App installation is not available yet'"
        :target="destination ? '_blank' : undefined"
        :rel="destination ? 'noopener noreferrer' : undefined"
        class="fb-button fb-button--ghost app-install-entry"
        data-app-install-entry
    >
        <Icon class="app-install-entry__desktop" name="monitor" :size="17" />
        <Icon class="app-install-entry__mobile" name="mobile" :size="17" />
        <span class="app-install-entry__desktop">Install Desktop App</span>
        <span class="app-install-entry__mobile">Install Mobile App</span>
    </component>
</template>

<style scoped>
.app-install-entry {
    flex: none;
    white-space: nowrap;
    border: 1px solid var(--fb-border);
    background: var(--fb-surface);
    font-size: 0.75rem;
}
.app-install-entry :deep(.fb-icon) {
    color: var(--fb-accent);
}
.app-install-entry[aria-disabled='true'] {
    cursor: not-allowed;
}
.app-install-entry__mobile {
    display: none;
}
@media (max-width: 900px) {
    .app-install-entry__desktop {
        display: none;
    }
    .app-install-entry__mobile {
        display: inline;
    }
}
@media (max-width: 560px) {
    .app-install-entry {
        padding-inline: 0.5rem;
        font-size: 0.6875rem;
        gap: 0.35rem;
    }
}
@media (max-width: 380px) {
    .app-install-entry {
        font-size: 0.625rem;
    }
}
</style>
