<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import { computed, onMounted, ref } from 'vue';
import { RouteSurface } from '@filebeam/ui';
import { contactRequest } from '../../../../ui/src/lib/contacts';
import AccountPage from '../../../../ui/src/components/layout/AccountPage.vue';
import Button from '../../../../ui/src/components/primitives/Button.vue';
import FormField from '../../../../ui/src/components/primitives/FormField.vue';
import Icon from '../../../../ui/src/components/primitives/Icon.vue';
import Input from '../../../../ui/src/components/primitives/Input.vue';

defineOptions({ layout: RouteSurface });

type Device = {
    id: string;
    display_name: string;
    platform: string;
    public_key: string;
    fingerprint: string;
    key_version: number;
    status: 'pending' | 'active' | 'revoked';
    generation: number;
    last_seen_at: string | null;
    presence_expires_at: string | null;
    revoked_at: string | null;
};
type Inventory = { revision: number; generation: number; devices: Device[] };
const inventory = ref<Inventory>();
const error = ref('');
const pending = ref('');
const editing = ref<Device>();
const name = ref('');
const revoke = ref<Device>();
const password = ref('');
const resetOpen = ref(false);
const active = computed(
    () => inventory.value?.devices.filter((device) => device.status === 'active') ?? [],
);
// The same SHA-256 key fingerprint native clients print, shortened for display.
function fingerprint(device: Device): string {
    return `${device.fingerprint.slice(0, 16).replace(/(.{4})(?!$)/g, '$1 ')}…`;
}
function state(device: Device): string {
    return device.status === 'revoked'
        ? `Revoked ${new Date(device.revoked_at!).toLocaleString()}`
        : device.status === 'pending'
          ? 'Pending approval'
          : device.presence_expires_at && new Date(device.presence_expires_at) > new Date()
            ? 'Ready'
            : 'Offline';
}
async function load(): Promise<void> {
    error.value = '';
    try {
        inventory.value = await contactRequest<Inventory>('devices');
    } catch (reason) {
        error.value = reason instanceof Error ? reason.message : 'Could not load devices.';
    }
}
function startRename(device: Device): void {
    editing.value = device;
    name.value = device.display_name;
}
async function saveRename(): Promise<void> {
    if (!editing.value || pending.value) return;
    pending.value = editing.value.id;
    try {
        await contactRequest(`devices/${editing.value.id}`, 'PATCH', { display_name: name.value });
        editing.value = undefined;
        await load();
    } catch (reason) {
        error.value = reason instanceof Error ? reason.message : 'Could not rename device.';
    } finally {
        pending.value = '';
    }
}
async function revokeDevice(): Promise<void> {
    if (!revoke.value || pending.value) return;
    pending.value = revoke.value.id;
    try {
        await contactRequest(`devices/${revoke.value.id}`, 'DELETE', {
            current_password: password.value,
        });
        revoke.value = undefined;
        password.value = '';
        await load();
    } catch (reason) {
        error.value = reason instanceof Error ? reason.message : 'Could not revoke device.';
    } finally {
        pending.value = '';
    }
}
async function reset(): Promise<void> {
    if (pending.value) return;
    pending.value = 'reset';
    try {
        await contactRequest('devices', 'DELETE', {
            current_password: password.value,
            confirmation: 'RESET',
        });
        resetOpen.value = false;
        password.value = '';
        await load();
    } catch (reason) {
        error.value = reason instanceof Error ? reason.message : 'Could not reset devices.';
    } finally {
        pending.value = '';
    }
}
onMounted(load);
</script>

<template>
    <div>
        <Head title="Devices" /><AccountPage
            title="My devices"
            description="Registered installations can connect only after native approval. File names, manifests, and receipts never appear here."
            ><template #actions
                ><a href="/account/contacts" class="fb-button fb-button--secondary"
                    ><Icon name="user" :size="16" />Contacts</a
                ></template
            >
            <section class="devices-panel" aria-label="Registered devices">
                <p v-if="error" class="devices-error" role="alert">
                    {{ error }} <Button variant="ghost" @click="load">Retry</Button>
                </p>
                <p v-else-if="!inventory" class="devices-loading" role="status">
                    <Icon name="loader" :size="18" />Loading devices...
                </p>
                <template v-else
                    ><header>
                        <div>
                            <strong
                                >{{ active.length }} active device{{
                                    active.length === 1 ? '' : 's'
                                }}</strong
                            >
                            <p>
                                Registry revision {{ inventory.revision }}. Online devices refresh
                                revocation status; offline devices use bounded cached authorization.
                            </p>
                        </div>
                        <Button variant="secondary" @click="resetOpen = true"
                            >Reset all devices</Button
                        >
                    </header>
                    <ul class="devices-list">
                        <li v-for="device in inventory.devices" :key="device.id">
                            <div class="device-main">
                                <Icon
                                    :name="device.status === 'active' ? 'monitor' : 'clock'"
                                    :size="20"
                                />
                                <div>
                                    <strong>{{ device.display_name }}</strong
                                    ><small
                                        >{{ device.platform }} · key v{{ device.key_version }} ·
                                        <span :title="device.fingerprint">{{
                                            fingerprint(device)
                                        }}</span></small
                                    ><small>{{ state(device) }}</small>
                                </div>
                            </div>
                            <div class="device-actions">
                                <Button
                                    v-if="device.status !== 'revoked'"
                                    variant="ghost"
                                    :disabled="!!pending"
                                    @click="startRename(device)"
                                    >Rename</Button
                                ><Button
                                    v-if="device.status !== 'revoked'"
                                    variant="secondary"
                                    :disabled="!!pending"
                                    @click="revoke = device"
                                    >Revoke</Button
                                >
                            </div>
                            <form
                                v-if="editing?.id === device.id"
                                class="device-form"
                                @submit.prevent="saveRename"
                            >
                                <FormField id="device-name" label="Device name"
                                    ><template #default="field"
                                        ><Input
                                            v-bind="field"
                                            v-model="name"
                                            maxlength="80"
                                            :disabled="!!pending" /></template></FormField
                                ><Button type="submit" :disabled="!name.trim() || !!pending"
                                    >Save</Button
                                ><Button type="button" variant="ghost" @click="editing = undefined"
                                    >Cancel</Button
                                >
                            </form>
                        </li>
                    </ul>
                    <form
                        v-if="revoke || resetOpen"
                        class="device-confirm"
                        @submit.prevent="revoke ? revokeDevice() : reset()"
                    >
                        <h2>
                            {{
                                revoke
                                    ? `Revoke ${revoke.display_name}?`
                                    : 'Reset all registered devices?'
                            }}
                        </h2>
                        <p>
                            {{
                                revoke
                                    ? 'This permanently denies its credential and rendezvous access. A new identity must be enrolled to use it again.'
                                    : 'This revokes every registered device and advances your device generation. Existing peers must be verified again.'
                            }}
                        </p>
                        <FormField id="device-password" label="Current password"
                            ><template #default="field"
                                ><Input
                                    v-bind="field"
                                    v-model="password"
                                    type="password"
                                    autocomplete="current-password"
                                    :disabled="!!pending" /></template></FormField
                        ><Button type="submit" :disabled="!password || !!pending">{{
                            pending ? 'Confirming...' : revoke ? 'Revoke device' : 'Reset devices'
                        }}</Button
                        ><Button
                            type="button"
                            variant="ghost"
                            @click="
                                revoke = undefined;
                                resetOpen = false;
                                password = '';
                            "
                            >Cancel</Button
                        >
                    </form>
                </template>
            </section>
        </AccountPage>
    </div>
</template>

<style scoped>
.devices-panel {
    display: grid;
    gap: 1rem;
}
.devices-panel > header,
.device-main,
.device-actions,
.device-form {
    display: flex;
    align-items: center;
    gap: 0.75rem;
}
.devices-panel > header {
    justify-content: space-between;
}
.devices-panel p,
small {
    color: var(--fb-text-muted);
}
.devices-list {
    display: grid;
    gap: 0.75rem;
    padding: 0;
    list-style: none;
}
.devices-list li,
.device-confirm {
    padding: 1rem;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
}
.devices-list li {
    display: grid;
    gap: 0.8rem;
}
.device-main {
    min-width: 0;
}
.device-main > div {
    display: grid;
    gap: 0.15rem;
}
.device-actions {
    margin-left: auto;
}
.device-form,
.device-confirm {
    flex-wrap: wrap;
}
.device-form :deep(.fb-form-field),
.device-confirm :deep(.fb-form-field) {
    flex: 1 1 16rem;
}
.devices-error {
    color: var(--fb-danger);
}
@media (max-width: 640px) {
    .devices-panel > header {
        align-items: flex-start;
        flex-direction: column;
    }
    .device-actions {
        margin-left: 0;
    }
}
</style>
