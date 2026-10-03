<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { contactRequest, type Contact, type Contacts } from '../../lib/contacts';
import type { PublicRecipient } from '../../types';
const props = defineProps<{ disabled: boolean }>();
const emit = defineEmits<{ choose: [recipient: PublicRecipient | undefined] }>();
const friends = ref<Contact[]>([]);
const selected = ref('');
const error = ref('');
onMounted(async () => {
    try {
        friends.value = (await contactRequest<Contacts>('contacts')).contacts.filter(
            (contact) => contact.status === 'accepted',
        );
    } catch {
        /* One-off receiving remains available on older instances. */
    }
});
async function choose(): Promise<void> {
    if (props.disabled) return;
    if (!selected.value) {
        emit('choose', undefined);
        return;
    }
    try {
        const recipient = await contactRequest<PublicRecipient>(`recipients/${selected.value}`);
        if (recipient.id !== friends.value.find((friend) => friend.username === selected.value)?.id)
            throw new Error('The saved contact account changed. Refresh contacts before sending.');
        emit('choose', recipient);
        error.value = '';
    } catch (reason) {
        error.value =
            reason instanceof Error
                ? reason.message
                : 'This receiving inbox is unavailable or does not permit you to send.';
    }
}
</script>
<template>
    <div>
        <label for="friend-recipient">Send to a friend</label>
        <select
            id="friend-recipient"
            v-model="selected"
            class="fb-input"
            :disabled="disabled"
            @change="choose"
        >
            <option value="">Share a link instead</option>
            <option v-for="friend in friends" :key="friend.id" :value="friend.username">
                @{{ friend.username }}
            </option>
        </select>
        <p v-if="error" role="alert">{{ error }}</p>
    </div>
</template>
<style scoped>
label {
    display: block;
    margin-bottom: 0.5rem;
    font-size: 0.8125rem;
    color: var(--fb-text-muted);
}
p {
    color: var(--fb-danger);
    font-size: 0.8125rem;
}
</style>
