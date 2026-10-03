<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { contactRequest, type Contacts, type ReceivingDefaults } from '../../lib/contacts';
import Switch from '../primitives/Switch.vue';
import AppLink from '../primitives/AppLink.vue';
import FormField from '../primitives/FormField.vue';
import Icon from '../primitives/Icon.vue';
import Select from '../primitives/Select.vue';

const settings = ref<ReceivingDefaults>();
const busy = ref(false);
const error = ref('');
onMounted(async () => {
    try {
        settings.value = (await contactRequest<Contacts>('contacts')).settings;
    } catch (reason) {
        error.value = reason instanceof Error ? reason.message : 'Receiving settings unavailable.';
    }
});
async function save(autoDownload?: boolean): Promise<void> {
    if (!settings.value || busy.value) return;
    busy.value = true;
    error.value = '';
    try {
        settings.value = await contactRequest<ReceivingDefaults>('account/receiving', 'PATCH', {
            receivingPolicy: settings.value.receivingPolicy,
            autoDownloadFriends: autoDownload ?? settings.value.autoDownloadFriends,
        });
    } catch (reason) {
        error.value =
            reason instanceof Error ? reason.message : 'Could not save receiving settings.';
    } finally {
        busy.value = false;
    }
}
</script>

<template>
    <section class="receiving-defaults">
        <template v-if="settings">
            <FormField
                id="receiving-policy"
                label="Who can send me files"
                description="The instance's restrictions and your inbox switch always apply."
            >
                <template #default="field"
                    ><Select
                        v-bind="field"
                        v-model="settings.receivingPolicy"
                        :disabled="busy"
                        icon="shield"
                        :options="[
                            { value: 'anyone', label: 'Anyone, including anonymous senders' },
                            { value: 'authenticated', label: 'Signed-in users' },
                            { value: 'friends', label: 'Friends only' },
                            { value: 'nobody', label: 'Nobody, unless explicitly allowed' },
                        ]"
                        @update:model-value="save()"
                /></template>
            </FormField>
            <label class="receiving-defaults__switch">
                <span
                    ><strong>Automatically download from friends</strong
                    ><small
                        >Eligible clients stage encrypted files privately. Saving remains your
                        choice.</small
                    ></span
                >
                <Switch
                    :model-value="settings.autoDownloadFriends"
                    :disabled="busy"
                    aria-label="Automatically download from friends"
                    @update:model-value="save"
                />
            </label>
            <AppLink href="/account/contacts" class="fb-text-link"
                ><Icon name="users" :size="15" />Manage friends and per-contact overrides</AppLink
            >
        </template>
        <p v-if="error" role="alert">{{ error }}</p>
    </section>
</template>

<style scoped>
.receiving-defaults {
    display: grid;
    gap: 0.75rem;
    padding-block: 0.25rem;
    font-size: 0.8125rem;
}
.receiving-defaults__switch {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding-block: 1rem;
    border-bottom: 1px solid var(--fb-line-soft);
}
strong {
    display: block;
    font-size: 0.8125rem;
    font-weight: 600;
}
.fb-text-link {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
}
small {
    display: block;
    margin-top: 0.25rem;
    color: var(--fb-text-muted);
    line-height: 1.5;
    font-size: 0.75rem;
}
p {
    color: var(--fb-danger);
}
</style>
