<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { contactRequest, type Contacts, type ReceivingDefaults } from '../../lib/contacts';
import Switch from '../primitives/Switch.vue';
import AppLink from '../primitives/AppLink.vue';

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
            <label for="receiving-policy">Who can send me files</label>
            <select
                id="receiving-policy"
                v-model="settings.receivingPolicy"
                class="fb-input"
                :disabled="busy"
                @change="save()"
            >
                <option value="anyone">Anyone, including anonymous senders</option>
                <option value="authenticated">Signed-in users</option>
                <option value="friends">Friends only</option>
                <option value="nobody">Nobody, unless explicitly allowed</option>
            </select>
            <label class="receiving-defaults__switch">
                <span
                    >Automatically download from friends<small
                        >Eligible clients stage encrypted files privately. Saving remains your
                        choice.</small
                    ></span
                >
                <Switch
                    :model-value="settings.autoDownloadFriends"
                    :disabled="busy"
                    @update:model-value="save"
                />
            </label>
            <AppLink href="/account/contacts" class="fb-text-link"
                >Manage friends and per-contact overrides</AppLink
            >
        </template>
        <p v-if="error" role="alert">{{ error }}</p>
    </section>
</template>

<style scoped>
.receiving-defaults {
    display: grid;
    gap: 0.75rem;
    padding-block: 1rem;
    font-size: 0.8125rem;
}
.receiving-defaults__switch {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
}
small {
    display: block;
    margin-top: 0.25rem;
    color: var(--fb-text-muted);
    line-height: 1.5;
}
p {
    color: var(--fb-danger);
}
</style>
