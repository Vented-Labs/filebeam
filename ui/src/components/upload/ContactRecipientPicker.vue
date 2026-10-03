<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import {
    ComboboxRoot,
    ComboboxAnchor,
    ComboboxInput,
    ComboboxTrigger,
    ComboboxPortal,
    ComboboxContent,
    ComboboxItem,
} from 'reka-ui';
import { contactRequest, type Contact, type Contacts } from '../../lib/contacts';
import type { PublicRecipient } from '../../types';
import FormField from '../primitives/FormField.vue';
import Input from '../primitives/Input.vue';
import Button from '../primitives/Button.vue';
import Icon from '../primitives/Icon.vue';

const props = defineProps<{ disabled: boolean; initialRecipient?: PublicRecipient }>();
const emit = defineEmits<{
    choose: [recipient: PublicRecipient | undefined];
    blocked: [value: boolean];
}>();
const friends = ref<Contact[]>([]);
const username = ref(props.initialRecipient ? `@${props.initialRecipient.username}` : '');
const checking = ref(false);
const error = ref('');
const resolved = ref(props.initialRecipient?.username ?? '');
const open = ref(false);
const suggestions = computed(() =>
    friends.value.filter((contact) =>
        contact.username.includes(username.value.trim().replace(/^@/, '').toLowerCase()),
    ),
);
let generation = 0;
onMounted(async () => {
    try {
        friends.value = (await contactRequest<Contacts>('contacts')).contacts.filter(
            (contact) => contact.status === 'accepted',
        );
    } catch {
        /* Username addressing remains available on older instances. */
    }
});
function edit(value: string): void {
    generation++;
    username.value = value;
    resolved.value = '';
    error.value = '';
    checking.value = false;
    emit('choose', undefined);
    emit('blocked', true);
}
function clear(): void {
    edit('');
}
function select(value: unknown): void {
    if (typeof value === 'string')
        void resolve(friends.value.find((contact) => contact.username === value));
}
async function resolve(contact?: Contact): Promise<void> {
    if (props.disabled) return;
    if (contact) edit(`@${contact.username}`);
    const name = username.value.trim().replace(/^@/, '').toLowerCase();
    if (!name) return;
    if (!/^[a-z0-9_]{3,24}$/.test(name)) {
        error.value = 'Enter an exact username on this instance.';
        return;
    }
    const current = ++generation;
    checking.value = true;
    error.value = '';
    try {
        const recipient = await contactRequest<PublicRecipient>(`recipients/${name}`);
        if (current !== generation) return;
        if (contact && recipient.id !== contact.id)
            throw new Error('This contact changed. Refresh contacts before sending.');
        username.value = `@${recipient.username}`;
        resolved.value = recipient.username;
        open.value = false;
        emit('choose', recipient);
        emit('blocked', false);
    } catch (reason) {
        if (current === generation)
            error.value =
                reason instanceof Error ? reason.message : 'This receiving inbox is unavailable.';
    } finally {
        if (current === generation) checking.value = false;
    }
}
function enter(event: KeyboardEvent): void {
    if (!open.value) {
        event.preventDefault();
        void resolve();
    }
}
</script>
<template>
    <FormField
        id="transfer-recipient"
        label="Username"
        :error="error"
        :description="resolved ? `Files will go to @${resolved}’s inbox.` : undefined"
    >
        <template #default="field">
            <ComboboxRoot
                :model-value="resolved"
                v-model:open="open"
                :ignore-filter="true"
                :reset-search-term-on-blur="false"
                :reset-search-term-on-select="false"
                @update:model-value="select"
            >
                <ComboboxAnchor class="recipient-address">
                    <ComboboxInput
                        as-child
                        :model-value="username"
                        :disabled="disabled"
                        @update:model-value="edit"
                    >
                        <Input
                            v-bind="field"
                            :model-value="username"
                            placeholder="@username"
                            autocomplete="off"
                            :disabled="disabled"
                            @keydown.enter="enter"
                            @blur="username.trim() && !resolved && resolve()"
                        />
                    </ComboboxInput>
                    <ComboboxTrigger as-child
                        ><Button
                            variant="secondary"
                            :disabled="disabled || !friends.length"
                            aria-label="Choose contact"
                            ><Icon name="users" :size="17" /></Button
                    ></ComboboxTrigger>
                    <Button
                        v-if="username"
                        variant="ghost"
                        :disabled="disabled"
                        aria-label="Clear recipient"
                        @click="clear"
                        ><Icon name="x" :size="17"
                    /></Button>
                    <span v-if="checking" role="status" aria-label="Checking recipient"
                        ><Icon name="loader" :size="17"
                    /></span>
                </ComboboxAnchor>
                <ComboboxPortal
                    ><ComboboxContent
                        v-if="suggestions.length"
                        class="fb-select-content"
                        position="popper"
                        :side-offset="8"
                    >
                        <ComboboxItem
                            v-for="contact in suggestions"
                            :key="contact.id"
                            :value="contact.username"
                            class="fb-select-item"
                            ><Icon name="user" :size="16" /><span
                                >@{{ contact.username }}</span
                            ></ComboboxItem
                        >
                    </ComboboxContent></ComboboxPortal
                >
            </ComboboxRoot>
        </template>
    </FormField>
</template>
<style scoped>
.recipient-address {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
}
.recipient-address :deep(.fb-input) {
    flex: 1;
    min-width: 0;
}
.recipient-address > .fb-button {
    flex: none;
    padding-inline: 0.75rem;
}
.recipient-address > span {
    display: flex;
    color: var(--fb-text-subtle);
}
</style>
