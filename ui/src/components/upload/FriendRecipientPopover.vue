<script setup lang="ts">
import { ref } from 'vue';
import { PopoverRoot, PopoverAnchor, PopoverTrigger, PopoverPortal, PopoverContent } from 'reka-ui';
import type { PublicRecipient } from '../../types';
import ContactRecipientPicker from './ContactRecipientPicker.vue';
import Button from '../primitives/Button.vue';
import Icon from '../primitives/Icon.vue';
const props = defineProps<{ disabled: boolean; value?: PublicRecipient }>();
const emit = defineEmits<{
    choose: [recipient: PublicRecipient | undefined];
    blocked: [value: boolean];
}>();
const open = ref(false);
const recipient = ref<PublicRecipient | undefined>(props.value);
function choose(value: PublicRecipient | undefined): void {
    recipient.value = value;
    emit('choose', value);
}
</script>
<template>
    <PopoverRoot v-model:open="open">
        <PopoverAnchor as-child
            ><PopoverTrigger as-child>
                <button
                    type="button"
                    class="password-trigger recipient-trigger"
                    :disabled="disabled"
                    aria-label="Set friend"
                >
                    <Icon name="user" :size="15" /><span>{{
                        recipient ? `@${recipient.username}` : 'Set Friend'
                    }}</span
                    ><Icon :name="recipient ? 'check' : 'plus'" :size="15" />
                </button> </PopoverTrigger
        ></PopoverAnchor>
        <PopoverPortal
            ><PopoverContent
                class="password-popover recipient-popover"
                :side-offset="9"
                :collision-padding="8"
                align="start"
            >
                <div class="password-popover__title">
                    <span>Send to Friend</span><Icon name="users" :size="15" />
                </div>
                <ContactRecipientPicker
                    :disabled="disabled"
                    :initial-recipient="value"
                    @choose="choose"
                    @blocked="emit('blocked', $event)"
                />
                <div class="password-popover__actions">
                    <Button variant="ghost" @click="open = false">Close</Button
                    ><Button :disabled="!recipient" @click="open = false"
                        >Done<Icon name="check" :size="14"
                    /></Button>
                </div> </PopoverContent
        ></PopoverPortal>
    </PopoverRoot>
</template>
<style>
.recipient-trigger {
    width: auto;
    min-width: 10rem;
    max-width: 18rem;
}
.recipient-popover {
    width: min(21rem, calc(100vw - 1rem));
}
</style>
