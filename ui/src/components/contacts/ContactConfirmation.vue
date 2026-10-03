<script setup lang="ts">
import Button from '../primitives/Button.vue';

const props = defineProps<{
    cancelId: string;
    title: string;
    description: string;
    action: string;
    pending?: boolean;
    error?: string;
    danger?: boolean;
}>();
const emit = defineEmits<{ cancel: []; confirm: [] }>();
function cancel(): void {
    if (!props.pending) emit('cancel');
}
</script>

<template>
    <div class="contact-confirmation" @keydown.escape.stop.prevent="cancel">
        <div>
            <h3>{{ title }}</h3>
            <p>{{ description }}</p>
            <p v-if="error" role="alert">{{ error }}</p>
        </div>
        <div class="contact-confirmation__actions">
            <Button :id="cancelId" variant="secondary" :disabled="pending" @click="cancel"
                >Cancel</Button
            ><Button
                :variant="danger ? 'danger' : 'primary'"
                :disabled="pending"
                @click="emit('confirm')"
                >{{ action }}</Button
            >
        </div>
    </div>
</template>

<style scoped>
.contact-confirmation {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    margin: 0 -1.5rem;
    padding: 1.25rem 1.5rem;
    border-top: 1px solid var(--fb-line-faint);
    background: var(--fb-surface-sunken);
}
h3 {
    margin: 0 0 0.3rem;
    font-size: 0.875rem;
}
p {
    margin: 0;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    line-height: 1.55;
}
[role='alert'] {
    margin-top: 0.65rem;
    color: var(--fb-danger);
}
.contact-confirmation__actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
}
@media (max-width: 640px) {
    .contact-confirmation {
        flex-direction: column;
        margin-inline: -1rem;
        padding-inline: 1rem;
    }
    .contact-confirmation__actions {
        align-self: flex-end;
    }
}
</style>
