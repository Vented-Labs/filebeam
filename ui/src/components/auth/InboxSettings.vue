<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import {
    cancelAccountCrypto,
    createPasswordEnvelope,
    exportPrivateKey,
    generateAccountKey,
    validatePrivateKey,
    type AccountKeyBundle,
} from '../../lib/account-crypto';
import { csrfHeaders } from '../../lib/csrf';
import AppLink from '../primitives/AppLink.vue';
import Button from '../primitives/Button.vue';
import CopyButton from '../primitives/CopyButton.vue';
import Icon from '../primitives/Icon.vue';
import Input from '../primitives/Input.vue';
import Switch from '../primitives/Switch.vue';

const props = defineProps<{
    user: {
        id: number;
        inboxEnabled: boolean;
        usernameRoutingEnabled: boolean;
        notificationChannel: 'mail' | 'database';
    };
}>();

const bundles = ref<AccountKeyBundle[]>([]);
const loadingBundles = ref(false);
const wizardOpen = ref(false);
const replacement = ref(false);
const savingKey = ref(false);
const savingInbox = ref(false);
const savingNotification = ref(false);
const error = ref('');
const success = ref('');
const custody = ref<'password' | 'self'>('password');
const password = ref('');
const confirmation = ref('');
const replacementAcknowledged = ref(false);
const generated = ref<{ privateKey: string; publicKey: string; fingerprint: string }>();
const inboxEnabled = ref(props.user.inboxEnabled);
const notificationChannel = ref(props.user.notificationChannel);
const savedNotificationChannel = ref(props.user.notificationChannel);
const activeBundle = computed(() => bundles.value.find((bundle) => bundle.is_active));
const requests = new AbortController();
let disposed = false;

async function jsonRequest(
    url: string,
    method: 'GET' | 'POST' | 'PATCH',
    body?: unknown,
): Promise<unknown> {
    let response: Response;
    try {
        response = await fetch(url, {
            method,
            headers: csrfHeaders(true),
            credentials: 'same-origin',
            signal: requests.signal,
            redirect: 'error',
            ...(body === undefined ? {} : { body: JSON.stringify(body) }),
        });
    } catch {
        throw new Error('Request could not be completed. Please refresh and try again.');
    }
    const payload = (await response.json().catch(() => null)) as {
        message?: string;
        errors?: Record<string, string[]>;
    } | null;
    if (!response.ok || !payload) {
        throw new Error(
            payload?.errors
                ? Object.values(payload.errors).flat()[0]
                : (payload?.message ?? 'Request failed. Please refresh and try again.'),
        );
    }
    return payload;
}

async function loadBundles(): Promise<void> {
    if (!props.user.usernameRoutingEnabled) return;
    loadingBundles.value = true;
    try {
        const payload = (await jsonRequest('/account/keys', 'GET')) as { data: AccountKeyBundle[] };
        if (disposed) return;
        bundles.value = payload.data;
    } catch (reason) {
        error.value = reason instanceof Error ? reason.message : 'Could not load account keys.';
    } finally {
        loadingBundles.value = false;
    }
}

function openWizard(isReplacement: boolean): void {
    error.value = '';
    success.value = '';
    replacement.value = isReplacement;
    replacementAcknowledged.value = false;
    custody.value = 'password';
    wizardOpen.value = true;
}

function clearGenerated(): void {
    if (generated.value) generated.value.privateKey = '';
    generated.value = undefined;
    password.value = '';
    confirmation.value = '';
}

function closeWizard(): void {
    if (savingKey.value) return;
    clearGenerated();
    replacementAcknowledged.value = false;
    wizardOpen.value = false;
}

async function generate(): Promise<void> {
    savingKey.value = true;
    error.value = '';
    try {
        generated.value = await generateAccountKey();
        confirmation.value = '';
    } catch (reason) {
        error.value =
            reason instanceof Error ? reason.message : 'Could not generate an account key.';
    } finally {
        savingKey.value = false;
    }
}

function downloadExport(): void {
    if (!generated.value) return;
    const url = URL.createObjectURL(
        new Blob([exportPrivateKey(generated.value.privateKey)], { type: 'text/plain' }),
    );
    const link = document.createElement('a');
    link.href = url;
    link.download = 'filebeam-account-key.txt';
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 0);
}

async function saveKey(): Promise<void> {
    if (
        savingKey.value ||
        !generated.value ||
        (replacement.value && !replacementAcknowledged.value)
    )
        return;
    const selectedCustody = custody.value;
    savingKey.value = true;
    error.value = '';
    try {
        let encryptedPrivateKey: string | null = null;
        if (selectedCustody === 'password') {
            if (!password.value)
                throw new Error('Enter your current password to protect this key.');
            encryptedPrivateKey = await createPasswordEnvelope(
                generated.value.privateKey,
                generated.value.publicKey,
                props.user.id,
                password.value,
            );
        } else {
            await validatePrivateKey(confirmation.value, generated.value.publicKey);
        }
        if (disposed) return;
        const payload = (await jsonRequest('/account/keys', 'POST', {
            public_key: generated.value.publicKey,
            fingerprint: generated.value.fingerprint,
            custody_mode: selectedCustody,
            encrypted_private_key: encryptedPrivateKey,
            ...(selectedCustody === 'password' ? { current_password: password.value } : {}),
            ...(replacement.value ? { replace: true } : {}),
        })) as { data: AccountKeyBundle; inbox_enabled?: boolean };
        if (disposed) return;
        bundles.value = [
            ...bundles.value.map((bundle) => ({ ...bundle, is_active: false })),
            payload.data,
        ];
        inboxEnabled.value = payload.inbox_enabled ?? true;
        clearGenerated();
        wizardOpen.value = false;
        success.value =
            'Secure inbox is active. Your profile link can now receive encrypted transfers.';
    } catch (reason) {
        error.value = reason instanceof Error ? reason.message : 'Could not save account key.';
    } finally {
        savingKey.value = false;
    }
}

async function updateInbox(enabled: boolean): Promise<void> {
    if (savingInbox.value) return;
    error.value = '';
    const previous = inboxEnabled.value;
    inboxEnabled.value = enabled;
    savingInbox.value = true;
    try {
        await jsonRequest('/account/inbox', 'PATCH', { enabled });
        success.value = enabled
            ? 'Incoming transfers are enabled.'
            : 'Incoming transfers are disabled.';
    } catch (reason) {
        inboxEnabled.value = previous;
        error.value = reason instanceof Error ? reason.message : 'Could not update inbox.';
    } finally {
        savingInbox.value = false;
    }
}

async function updateNotification(): Promise<void> {
    if (savingNotification.value) return;
    error.value = '';
    savingNotification.value = true;
    try {
        await jsonRequest('/account/notifications', 'PATCH', {
            channel: notificationChannel.value,
        });
        savedNotificationChannel.value = notificationChannel.value;
        success.value = 'Notification preference saved.';
    } catch (reason) {
        notificationChannel.value = savedNotificationChannel.value;
        error.value = reason instanceof Error ? reason.message : 'Could not update notifications.';
    } finally {
        savingNotification.value = false;
    }
}

onMounted(loadBundles);
onBeforeUnmount(() => {
    disposed = true;
    requests.abort();
    cancelAccountCrypto();
    clearGenerated();
});
</script>

<template>
    <section
        class="mt-7 space-y-5 rounded-2xl border border-[var(--fb-border)] bg-[var(--fb-surface)] p-6"
    >
        <div class="flex flex-wrap items-start justify-between gap-3">
            <div>
                <h2 class="text-lg font-semibold">Secure inbox</h2>
                <p class="mt-1 text-sm text-[var(--fb-text-muted)]">
                    Receive encrypted transfers with a key only you can unlock.
                </p>
            </div>
            <AppLink href="/account/inbox" class="fb-text-link text-sm">Open inbox</AppLink>
        </div>

        <p
            v-if="!user.usernameRoutingEnabled"
            class="rounded-lg border border-[var(--fb-border)] bg-[var(--fb-surface-raised)] p-4 text-sm text-[var(--fb-text-muted)]"
        >
            New username inbox setup is unavailable on this instance. Existing inbox transfers
            remain available above.
        </p>
        <div v-else-if="loadingBundles" class="text-sm text-[var(--fb-text-muted)]">
            Loading account keys...
        </div>
        <template v-else-if="activeBundle && !wizardOpen">
            <div
                class="rounded-lg border border-[var(--fb-border)] bg-[var(--fb-surface-raised)] p-4 text-sm"
            >
                <strong class="block">{{
                    inboxEnabled ? 'Secure inbox active' : 'Receiving paused'
                }}</strong
                ><span class="mt-1 block break-all font-mono text-xs text-[var(--fb-text-muted)]"
                    >Key version {{ activeBundle.version }}: {{ activeBundle.fingerprint }}</span
                >
            </div>
            <label class="flex items-center justify-between gap-4"
                ><span
                    ><strong class="block text-sm">Accept incoming transfers</strong
                    ><span class="text-sm text-[var(--fb-text-muted)]"
                        >People can send encrypted files to your inbox.</span
                    ></span
                ><Switch
                    :model-value="inboxEnabled"
                    :disabled="savingInbox"
                    @update:model-value="updateInbox"
            /></label>
            <div>
                <label class="block text-sm font-medium" for="notification-channel"
                    >New transfer notifications</label
                ><select
                    id="notification-channel"
                    v-model="notificationChannel"
                    class="mt-2 w-full rounded-lg border border-[var(--fb-border)] bg-[var(--fb-surface-raised)] px-3 py-2 text-sm disabled:opacity-60"
                    :disabled="savingNotification"
                    @change="updateNotification"
                >
                    <option value="mail">Email</option>
                    <option value="database">In-app</option>
                </select>
            </div>
            <Button variant="secondary" @click="openWizard(true)">Replace lost key</Button>
        </template>
        <template v-else>
            <template v-if="!wizardOpen"
                ><p class="text-sm text-[var(--fb-text-muted)]">
                    Set up an account key before enabling incoming transfers.
                </p>
                <Button @click="openWizard(false)">Activate secure inbox</Button></template
            >
            <div v-else class="space-y-4 rounded-xl border border-[var(--fb-border)] p-4">
                <div
                    v-if="replacement"
                    class="rounded-lg border border-[var(--fb-warning)] p-4 text-sm text-[var(--fb-text-muted)]"
                >
                    <strong class="block text-[var(--fb-text)]"
                        >Replacing a lost key permanently retires the current key.</strong
                    >
                    Transfers encrypted for the old key cannot be recovered without its password or
                    private export. Old key records remain retained.
                </div>
                <div class="space-y-3" role="radiogroup" aria-label="Account key custody">
                    <label
                        class="block cursor-pointer rounded-xl border p-4 transition focus-within:ring-2 focus-within:ring-[var(--fb-action)]"
                        :class="
                            custody === 'password'
                                ? 'border-[var(--fb-action)] bg-[var(--fb-surface-raised)]'
                                : 'border-[var(--fb-border)]'
                        "
                    >
                        <input
                            v-model="custody"
                            class="sr-only"
                            type="radio"
                            name="account-custody"
                            value="password"
                            :disabled="savingKey || !!generated"
                        />
                        <span class="flex gap-3"
                            ><Icon name="lock" :size="24" /><span
                                ><strong class="block"
                                    >Protect with password
                                    <span class="text-xs font-normal text-[var(--fb-success)]"
                                        >Recommended</span
                                    ></strong
                                ><span class="mt-1 block text-sm text-[var(--fb-text-muted)]"
                                    >Your browser encrypts your private key with your account
                                    password before Filebeam stores it. If you forget that password,
                                    Filebeam cannot recover the key or old files.</span
                                ></span
                            ></span
                        >
                    </label>
                    <label
                        class="block cursor-pointer rounded-xl border p-4 transition focus-within:ring-2 focus-within:ring-[var(--fb-action)]"
                        :class="
                            custody === 'self'
                                ? 'border-[var(--fb-action)] bg-[var(--fb-surface-raised)]'
                                : 'border-[var(--fb-border)]'
                        "
                    >
                        <input
                            v-model="custody"
                            class="sr-only"
                            type="radio"
                            name="account-custody"
                            value="self"
                            :disabled="savingKey || !!generated"
                        />
                        <span class="flex gap-3"
                            ><Icon name="key" :size="24" /><span
                                ><strong class="block">Keep the key yourself</strong
                                ><span class="mt-1 block text-sm text-[var(--fb-text-muted)]"
                                    >Filebeam stores only your public key. If the private export is
                                    lost, transfers for this key cannot be recovered.</span
                                ></span
                            ></span
                        >
                    </label>
                </div>
                <label v-if="replacement" class="flex items-start gap-3 text-sm"
                    ><input
                        v-model="replacementAcknowledged"
                        type="checkbox"
                        :disabled="savingKey"
                    /><span
                        >I understand this replacement cannot recover transfers encrypted for the
                        old key.</span
                    ></label
                >
                <template v-if="!generated">
                    <Button
                        :disabled="savingKey || (replacement && !replacementAcknowledged)"
                        @click="generate"
                        >Generate {{ replacement ? 'replacement' : 'account' }} key</Button
                    >
                    <Button variant="ghost" :disabled="savingKey" @click="closeWizard"
                        >Cancel</Button
                    >
                </template>
                <form
                    v-else
                    class="space-y-4 border-t border-[var(--fb-border)] pt-4"
                    @submit.prevent="saveKey"
                >
                    <template v-if="custody === 'password'"
                        ><label class="block text-sm font-medium" for="account-current-password"
                            >Current password</label
                        ><Input
                            id="account-current-password"
                            v-model="password"
                            class="mt-2"
                            type="password"
                            autocomplete="current-password"
                            :disabled="savingKey"
                        />
                        <p class="text-sm text-[var(--fb-text-muted)]">
                            A password reset does not unlock old transfers. Keep the password that
                            protected this key.
                        </p></template
                    >
                    <template v-else
                        ><p
                            class="rounded-lg border border-[var(--fb-warning)] p-3 text-sm text-[var(--fb-text-muted)]"
                        >
                            Save this export somewhere secure. Losing it permanently prevents access
                            to transfers for this key.
                        </p>
                        <div class="flex flex-wrap gap-2">
                            <CopyButton
                                :value="exportPrivateKey(generated.privateKey)"
                                label="Copy private key"
                            /><Button
                                variant="secondary"
                                :disabled="savingKey"
                                @click="downloadExport"
                                >Download private key</Button
                            >
                        </div>
                        <label class="block text-sm font-medium" for="account-key-confirmation"
                            >Paste the exported private key to confirm</label
                        ><Input
                            id="account-key-confirmation"
                            v-model="confirmation"
                            class="mt-2 font-mono"
                            autocomplete="off"
                            spellcheck="false"
                            :disabled="savingKey"
                    /></template>
                    <div class="flex flex-wrap gap-2">
                        <Button
                            type="submit"
                            :disabled="savingKey || (replacement && !replacementAcknowledged)"
                            >Save account key</Button
                        ><Button variant="ghost" :disabled="savingKey" @click="closeWizard"
                            >Cancel</Button
                        >
                    </div>
                </form>
            </div>
        </template>
        <p v-if="success" class="text-sm text-[var(--fb-success)]" role="status">{{ success }}</p>
        <p v-if="error" class="text-sm text-[var(--fb-danger)]" role="alert">{{ error }}</p>
    </section>
</template>
