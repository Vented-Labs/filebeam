<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, useId, watch } from 'vue';
import { contactRequest, type Contacts, type ReceivingDefaults } from '../../lib/contacts';
import AppLink from '../primitives/AppLink.vue';
import Button from '../primitives/Button.vue';
import FormField from '../primitives/FormField.vue';
import Icon from '../primitives/Icon.vue';
import Select from '../primitives/Select.vue';
import Switch from '../primitives/Switch.vue';

const props = withDefaults(
    defineProps<{
        controlled?: boolean;
        modelValue?: ReceivingDefaults;
        pending?: boolean;
        error?: string;
        showContactsLink?: boolean;
    }>(),
    { controlled: false, pending: false, error: '', showContactsLink: true },
);
const emit = defineEmits<{ 'update:modelValue': [next: ReceivingDefaults] }>();

const saved = ref<ReceivingDefaults>();
const draft = ref<ReceivingDefaults>();
const busy = ref(false);
const loading = ref(false);
const loadError = ref('');
const idPrefix = useId();
const policyId = `${idPrefix}-receiving-policy`;
const autoDownloadId = `${idPrefix}-auto-download-friends`;
let disposed = false;
let readVersion = 0;

function copy(settings: ReceivingDefaults): ReceivingDefaults {
    return { ...settings };
}

function useControlledSettings(): void {
    if (!props.controlled || props.pending || !props.modelValue) return;
    draft.value = copy(props.modelValue);
}

watch(() => [props.controlled, props.modelValue, props.pending] as const, useControlledSettings, {
    immediate: true,
});

async function loadSettings(): Promise<boolean> {
    if (loading.value) return false;
    const version = ++readVersion;
    loading.value = true;
    loadError.value = '';
    try {
        const settings = (await contactRequest<Contacts>('contacts')).settings;
        if (disposed || version !== readVersion) return false;
        saved.value = copy(settings);
        draft.value = copy(settings);
        return true;
    } catch (reason) {
        if (disposed || version !== readVersion) return false;
        loadError.value =
            reason instanceof Error ? reason.message : 'Receiving settings unavailable.';
        return false;
    } finally {
        if (!disposed && version === readVersion) loading.value = false;
    }
}

onMounted(() => {
    if (!props.controlled) void loadSettings();
});

onBeforeUnmount(() => {
    disposed = true;
    readVersion++;
});

async function update(next: ReceivingDefaults): Promise<void> {
    if (props.controlled) {
        if (props.pending) return;
        draft.value = copy(next);
        emit('update:modelValue', copy(next));
        return;
    }

    if (busy.value || !saved.value) return;
    draft.value = copy(next);
    busy.value = true;
    loadError.value = '';
    try {
        const settings = await contactRequest<ReceivingDefaults>('account/receiving', 'PATCH', {
            receivingPolicy: next.receivingPolicy,
            autoDownloadFriends: next.autoDownloadFriends,
        });
        if (disposed) return;
        saved.value = copy(settings);
        draft.value = copy(settings);
    } catch {
        if (disposed) return;
        const refreshed = await loadSettings();
        if (disposed) return;
        loadError.value = refreshed
            ? "Couldn't confirm this change. Settings were refreshed."
            : "Couldn't confirm this change. Please try again.";
    } finally {
        if (!disposed) busy.value = false;
    }
}

function updatePolicy(receivingPolicy: string): void {
    if (!draft.value) return;
    void update({
        ...draft.value,
        receivingPolicy: receivingPolicy as ReceivingDefaults['receivingPolicy'],
    });
}

function updateAutoDownload(autoDownloadFriends: boolean): void {
    if (!draft.value) return;
    void update({ ...draft.value, autoDownloadFriends });
}

function retry(): void {
    if (!props.controlled && !busy.value) void loadSettings();
}
</script>

<template>
    <section class="receiving-defaults">
        <div v-if="draft" class="receiving-defaults__fields">
            <FormField
                :id="policyId"
                label="Who can send me files"
                description="The instance's restrictions and your inbox switch always apply."
            >
                <template #default="field">
                    <Select
                        :id="field.id"
                        label="Who can send me files"
                        :aria-describedby="field.describedBy"
                        :aria-invalid="field.invalid"
                        :model-value="draft.receivingPolicy"
                        :disabled="busy || pending"
                        icon="shield"
                        :options="[
                            { value: 'anyone', label: 'Anyone, including anonymous senders' },
                            { value: 'authenticated', label: 'Signed-in users' },
                            { value: 'friends', label: 'Friends only' },
                            { value: 'nobody', label: 'Nobody, unless explicitly allowed' },
                        ]"
                        @update:model-value="updatePolicy"
                    />
                </template>
            </FormField>
            <FormField
                :id="autoDownloadId"
                label="Automatically download from friends"
                description="Eligible friends' transfers are staged as ciphertext. Files are saved manually, and only on clients that opt in."
            >
                <template #default="field">
                    <div class="receiving-defaults__switch-row">
                        <Switch
                            :id="autoDownloadId"
                            :aria-describedby="field.describedBy"
                            :aria-invalid="field.invalid"
                            :model-value="draft.autoDownloadFriends"
                            :disabled="busy || pending"
                            @update:model-value="updateAutoDownload"
                        />
                    </div>
                </template>
            </FormField>
        </div>
        <AppLink v-if="draft && showContactsLink" href="/account/contacts" class="fb-text-link">
            <Icon name="users" :size="15" />Manage friends and per-contact overrides
        </AppLink>
        <div v-if="error || loadError" class="receiving-defaults__error" role="alert">
            <p>{{ error || loadError }}</p>
            <Button
                v-if="loadError && !controlled"
                variant="ghost"
                :disabled="busy || loading"
                @click="retry"
                >Retry</Button
            >
        </div>
    </section>
</template>

<style scoped>
.receiving-defaults {
    display: grid;
    gap: 0.75rem;
    padding-block: 0.25rem;
    font-size: 0.8125rem;
}
.receiving-defaults__fields {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 1rem;
}
.receiving-defaults__switch-row {
    display: flex;
    min-height: 2.75rem;
    align-items: center;
}
:deep(.account-select) {
    height: auto;
    min-height: 2.75rem;
}
:deep(.account-select__value) {
    overflow: visible;
    text-overflow: clip;
    white-space: normal;
}
.fb-text-link {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
}
.receiving-defaults__error {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    color: var(--fb-danger);
}
.receiving-defaults__error p {
    margin: 0;
}
@media (max-width: 560px) {
    .receiving-defaults__fields {
        grid-template-columns: 1fr;
    }
}
</style>
