<script setup lang="ts">
import { computed } from 'vue';
import Icon from '../primitives/Icon.vue';

const props = withDefaults(
    defineProps<{
        id?: string;
        name: string;
        username: string;
        interactive?: boolean;
        expanded?: boolean;
        status?: string;
    }>(),
    { id: undefined, interactive: false, expanded: false, status: '' },
);
const emit = defineEmits<{ activate: [] }>();
const label = computed(() => props.name || props.username);
const initials = computed(() =>
    label.value
        .split(/\s+/)
        .filter(Boolean)
        .slice(0, 2)
        .map((part) => part[0])
        .join('')
        .toUpperCase(),
);
</script>

<template>
    <button
        v-if="interactive"
        :id="id"
        class="contact-identity"
        :aria-expanded="expanded"
        @click="emit('activate')"
    >
        <span class="avatar">{{ initials }}</span>
        <span class="contact-identity__meta">
            <strong>{{ label }}</strong>
            <span class="contact-identity__secondary">
                <small>@{{ username }}</small>
                <em v-if="status"
                    ><Icon name="settings" :size="12" /><span class="status-label">{{
                        status
                    }}</span
                    ><span class="sr-only">{{ status }}</span></em
                >
            </span>
        </span>
    </button>
    <div v-else class="contact-identity">
        <span class="avatar">{{ initials }}</span>
        <span class="contact-identity__meta">
            <strong>{{ label }}</strong
            ><small>@{{ username }}</small>
        </span>
    </div>
</template>

<style scoped>
.contact-identity {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: 0.8rem;
    text-align: left;
}
button.contact-identity {
    cursor: pointer;
}
.contact-identity__meta {
    display: grid;
    min-width: 0;
    gap: 0.15rem;
}
strong {
    font-size: 0.875rem;
    overflow-wrap: break-word;
}
small {
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    overflow-wrap: anywhere;
}
.contact-identity__secondary {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.35rem;
}
.avatar {
    display: grid;
    width: 2.6rem;
    height: 2.6rem;
    flex: none;
    place-items: center;
    border: 1px solid var(--fb-card-border);
    border-radius: 0.8rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
    font-size: 0.75rem;
    font-weight: 650;
}
em {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    margin-left: 0.2rem;
    padding: 0.15rem 0.35rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.3rem;
    color: var(--fb-text-muted);
    font-size: 0.65rem;
    font-style: normal;
    white-space: nowrap;
}
.sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
}
@media (max-width: 640px) {
    .avatar {
        width: 2.25rem;
        height: 2.25rem;
        border-radius: 0.65rem;
    }
    em :deep(svg) {
        display: block;
    }
    em {
        margin: 0;
        padding: 0;
        border: 0;
    }
    .status-label {
        display: none;
    }
}
@media (min-width: 641px) {
    em :deep(svg) {
        display: none;
    }
}
</style>
