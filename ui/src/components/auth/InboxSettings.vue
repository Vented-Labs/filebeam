<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
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
import AnimatedHeight from '../layout/AnimatedHeight.vue';
import AnimatedReveal from '../layout/AnimatedReveal.vue';

const props = defineProps<{
    user: {
        id: number;
        inboxEnabled: boolean;
        usernameRoutingEnabled: boolean;
        notificationChannel: 'mail' | 'database';
    };
}>();

const bundles = ref<AccountKeyBundle[]>([]);
const loadingBundles = ref(props.user.usernameRoutingEnabled);
const animateSettingsState = ref(false);
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
const settingsState = computed(() => {
    if (!props.user.usernameRoutingEnabled) return 'unavailable';
    if (loadingBundles.value) return 'loading';
    return activeBundle.value ? 'configured' : 'setup';
});
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
        await nextTick();
        if (!disposed) animateSettingsState.value = true;
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

function makeOutgoingInert(element: Element): void {
    const pane = element as HTMLElement;
    pane.inert = true;
    pane.setAttribute('aria-hidden', 'true');
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
    <section class="inbox-settings">
        <header class="inbox-settings__header">
            <span class="inbox-settings__header-icon"><Icon name="shield" :size="21" /></span>
            <div class="inbox-settings__heading">
                <h2>Secure inbox</h2>
                <p>Receive encrypted transfers with a key only you can unlock.</p>
            </div>
            <AppLink href="/account/inbox" class="fb-button fb-button--secondary">
                <Icon name="folder" :size="16" />Open inbox
            </AppLink>
        </header>

        <AnimatedHeight class="inbox-settings__state-height">
            <Transition
                name="inbox-settings__state"
                :css="animateSettingsState"
                @before-leave="makeOutgoingInert"
            >
                <div :key="settingsState" class="inbox-settings__state">
                    <p v-if="!user.usernameRoutingEnabled" class="inbox-settings__notice">
                        New username inbox setup is unavailable on this instance. Existing inbox
                        transfers remain available above.
                    </p>
                    <div v-else-if="loadingBundles" class="inbox-settings__loading" role="status">
                        <span><Icon name="loader" :size="20" /></span>
                        <div>
                            <strong>Loading account keys</strong>
                            <p>Checking this browser's secure-inbox configuration.</p>
                        </div>
                    </div>
                    <template v-else-if="activeBundle && !wizardOpen">
                        <div class="inbox-settings__key-status">
                            <span class="inbox-settings__status-icon"
                                ><Icon name="key" :size="18"
                            /></span>
                            <div>
                                <strong>{{
                                    inboxEnabled ? 'Secure inbox active' : 'Receiving paused'
                                }}</strong>
                                <span class="fb-code"
                                    >Key version {{ activeBundle.version }}:
                                    {{ activeBundle.fingerprint }}</span
                                >
                            </div>
                        </div>
                        <label class="inbox-settings__setting-row">
                            <span>
                                <strong>Accept incoming transfers</strong>
                                <small>People can send encrypted files to your inbox.</small>
                            </span>
                            <Switch
                                :model-value="inboxEnabled"
                                :disabled="savingInbox"
                                @update:model-value="updateInbox"
                            />
                        </label>
                        <div
                            class="inbox-settings__setting-row inbox-settings__setting-row--select"
                        >
                            <label for="notification-channel">New transfer notifications</label>
                            <select
                                id="notification-channel"
                                v-model="notificationChannel"
                                class="fb-input inbox-settings__select"
                                :disabled="savingNotification"
                                @change="updateNotification"
                            >
                                <option value="mail">Email</option>
                                <option value="database">In-app</option>
                            </select>
                        </div>
                        <Button
                            variant="ghost"
                            class="inbox-settings__replace"
                            @click="openWizard(true)"
                        >
                            <Icon name="redo" :size="15" />Replace lost key
                        </Button>
                    </template>
                    <template v-else>
                        <div v-if="!wizardOpen" class="inbox-settings__activation">
                            <span><Icon name="key" :size="27" /></span>
                            <div>
                                <h3>Create your receiving key</h3>
                                <p>
                                    Set up an account key before enabling incoming transfers. Key
                                    creation and protection happen in this browser.
                                </p>
                            </div>
                            <Button @click="openWizard(false)">
                                <Icon name="plus" :size="16" />Activate secure inbox
                            </Button>
                        </div>
                        <div v-else class="inbox-settings__wizard">
                            <AnimatedReveal :show="replacement">
                                <div class="inbox-settings__warning">
                                    <Icon name="alert" :size="18" />
                                    <p>
                                        <strong
                                            >Replacing a lost key permanently retires the current
                                            key.</strong
                                        >
                                        Transfers encrypted for the old key cannot be recovered
                                        without its password or private export. Old key records
                                        remain retained.
                                    </p>
                                </div>
                            </AnimatedReveal>
                            <div class="inbox-settings__wizard-heading">
                                <span>Step 1</span>
                                <h3>Choose where your private key lives</h3>
                            </div>
                            <div
                                class="inbox-settings__custody"
                                role="radiogroup"
                                aria-label="Account key custody"
                            >
                                <label
                                    class="inbox-settings__custody-card"
                                    :class="{
                                        'inbox-settings__custody-card--selected':
                                            custody === 'password',
                                    }"
                                >
                                    <input
                                        v-model="custody"
                                        class="sr-only"
                                        type="radio"
                                        name="account-custody"
                                        value="password"
                                        :disabled="savingKey || !!generated"
                                    />
                                    <span class="inbox-settings__custody-icon"
                                        ><Icon name="lock" :size="20"
                                    /></span>
                                    <span>
                                        <strong
                                            >Protect with password
                                            <small>Recommended</small></strong
                                        >
                                        <span
                                            >Your browser encrypts your private key before Filebeam
                                            stores it. Keep the password used for this key.</span
                                        >
                                    </span>
                                    <span class="inbox-settings__radio" />
                                </label>
                                <label
                                    class="inbox-settings__custody-card"
                                    :class="{
                                        'inbox-settings__custody-card--selected':
                                            custody === 'self',
                                    }"
                                >
                                    <input
                                        v-model="custody"
                                        class="sr-only"
                                        type="radio"
                                        name="account-custody"
                                        value="self"
                                        :disabled="savingKey || !!generated"
                                    />
                                    <span class="inbox-settings__custody-icon"
                                        ><Icon name="key" :size="20"
                                    /></span>
                                    <span>
                                        <strong>Keep the key yourself</strong>
                                        <span
                                            >Filebeam stores only your public key. A lost private
                                            export cannot be recovered.</span
                                        >
                                    </span>
                                    <span class="inbox-settings__radio" />
                                </label>
                            </div>
                            <AnimatedReveal :show="replacement">
                                <label class="inbox-settings__acknowledgement">
                                    <input
                                        v-model="replacementAcknowledged"
                                        type="checkbox"
                                        :disabled="savingKey"
                                    />
                                    <span
                                        >I understand this replacement cannot recover transfers
                                        encrypted for the old key.</span
                                    >
                                </label>
                            </AnimatedReveal>
                            <div v-if="!generated" class="inbox-settings__wizard-actions">
                                <Button
                                    :disabled="
                                        savingKey || (replacement && !replacementAcknowledged)
                                    "
                                    @click="generate"
                                >
                                    Generate {{ replacement ? 'replacement' : 'account' }} key
                                </Button>
                                <Button variant="ghost" :disabled="savingKey" @click="closeWizard"
                                    >Cancel</Button
                                >
                            </div>
                            <form v-else class="inbox-settings__key-form" @submit.prevent="saveKey">
                                <div class="inbox-settings__wizard-heading">
                                    <span>Step 2</span>
                                    <h3>Secure and confirm your key</h3>
                                </div>
                                <template v-if="custody === 'password'">
                                    <label for="account-current-password">Current password</label>
                                    <Input
                                        id="account-current-password"
                                        v-model="password"
                                        type="password"
                                        autocomplete="current-password"
                                        :disabled="savingKey"
                                    />
                                    <p>
                                        A password reset does not unlock old transfers. Keep the
                                        password that protected this key.
                                    </p>
                                </template>
                                <template v-else>
                                    <div class="inbox-settings__warning">
                                        <Icon name="alert" :size="18" />
                                        <p>
                                            Save this export somewhere secure. Losing it permanently
                                            prevents access to transfers for this key.
                                        </p>
                                    </div>
                                    <div class="inbox-settings__export-actions">
                                        <CopyButton
                                            :value="exportPrivateKey(generated.privateKey)"
                                            label="Copy private key"
                                        />
                                        <Button
                                            variant="secondary"
                                            :disabled="savingKey"
                                            @click="downloadExport"
                                            >Download private key</Button
                                        >
                                    </div>
                                    <label for="account-key-confirmation"
                                        >Paste the exported private key to confirm</label
                                    >
                                    <Input
                                        id="account-key-confirmation"
                                        v-model="confirmation"
                                        class="fb-code"
                                        autocomplete="off"
                                        spellcheck="false"
                                        :disabled="savingKey"
                                    />
                                </template>
                                <div class="inbox-settings__wizard-actions">
                                    <Button
                                        type="submit"
                                        :disabled="
                                            savingKey || (replacement && !replacementAcknowledged)
                                        "
                                        >Save account key</Button
                                    >
                                    <Button
                                        variant="ghost"
                                        :disabled="savingKey"
                                        @click="closeWizard"
                                        >Cancel</Button
                                    >
                                </div>
                            </form>
                        </div>
                    </template>
                </div>
            </Transition>
        </AnimatedHeight>
        <AnimatedReveal :show="Boolean(success)">
            <p class="inbox-settings__feedback inbox-settings__feedback--success" role="status">
                {{ success }}
            </p>
        </AnimatedReveal>
        <AnimatedReveal :show="Boolean(error)">
            <p class="inbox-settings__feedback inbox-settings__feedback--error" role="alert">
                {{ error }}
            </p>
        </AnimatedReveal>
    </section>
</template>

<style scoped>
.inbox-settings {
    min-width: 0;
    padding: 1.75rem;
}
.inbox-settings__header {
    display: grid;
    grid-template-columns: 2.75rem minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.875rem;
    padding-bottom: 1.375rem;
    border-bottom: 1px solid #ffffff0a;
}
.inbox-settings__header-icon,
.inbox-settings__status-icon,
.inbox-settings__activation > span {
    display: grid;
    place-items: center;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.inbox-settings__header-icon {
    width: 2.75rem;
    height: 2.75rem;
    border: 1px solid #78598666;
    border-radius: 0.75rem;
}
.inbox-settings__heading h2 {
    margin: 0;
    font-size: 1.125rem;
    letter-spacing: -0.02em;
}
.inbox-settings__heading p {
    margin: 0.25rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
}
.inbox-settings__state-height {
    position: relative;
    margin-top: 1.375rem;
    overflow-clip-margin: 0.25rem;
}
.inbox-settings__state {
    min-height: 13rem;
}
.inbox-settings__state-enter-active,
.inbox-settings__state-leave-active {
    transition:
        opacity var(--fb-duration-pane) var(--fb-ease),
        transform var(--fb-duration-pane) var(--fb-ease),
        filter var(--fb-duration-pane) var(--fb-ease);
}
.inbox-settings__state-leave-active {
    position: absolute;
    inset: 0;
    width: 100%;
    pointer-events: none;
}
.inbox-settings__state-enter-from {
    opacity: 0;
    filter: blur(3px);
    transform: scale(0.995);
}
.inbox-settings__state-leave-to {
    opacity: 0;
    filter: blur(2px);
    transform: scale(0.995);
}
.inbox-settings__notice,
.inbox-settings__warning {
    border: 1px solid var(--fb-border);
    border-radius: 0.75rem;
    color: var(--fb-text-muted);
    background: var(--fb-surface-raised);
    font-size: 0.8125rem;
    line-height: 1.6;
}
.inbox-settings__notice {
    margin: 0;
    padding: 1rem;
}
.inbox-settings__loading {
    display: flex;
    min-height: 12rem;
    align-items: center;
    justify-content: center;
    gap: 0.875rem;
    color: var(--fb-text-muted);
}
.inbox-settings__loading > span {
    display: grid;
    width: 2.5rem;
    height: 2.5rem;
    place-items: center;
    border-radius: 0.75rem;
    background: var(--fb-selected-surface);
}
.inbox-settings__loading strong {
    display: block;
    color: var(--fb-text);
    font-size: 0.875rem;
}
.inbox-settings__loading p {
    margin: 0.25rem 0 0;
    font-size: 0.75rem;
}
.inbox-settings__key-status {
    display: grid;
    grid-template-columns: 2.5rem minmax(0, 1fr);
    gap: 0.75rem;
    padding: 0.875rem;
    border: 1px solid #6d518044;
    border-radius: 0.75rem;
    background: #251b2d;
}
.inbox-settings__status-icon {
    width: 2.5rem;
    height: 2.5rem;
    border-radius: 0.625rem;
}
.inbox-settings__key-status strong,
.inbox-settings__key-status .fb-code {
    display: block;
}
.inbox-settings__key-status strong {
    font-size: 0.875rem;
}
.inbox-settings__key-status .fb-code {
    overflow-wrap: anywhere;
    margin-top: 0.25rem;
    color: var(--fb-text-subtle);
    font-size: 0.6875rem;
}
.inbox-settings__setting-row {
    display: flex;
    min-height: 4.25rem;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding-block: 0.875rem;
    border-bottom: 1px solid #ffffff08;
}
.inbox-settings__setting-row strong,
.inbox-settings__setting-row small {
    display: block;
}
.inbox-settings__setting-row strong,
.inbox-settings__setting-row > label {
    color: var(--fb-text);
    font-size: 0.8125rem;
    font-weight: 600;
}
.inbox-settings__setting-row small {
    margin-top: 0.25rem;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
}
.inbox-settings__setting-row--select {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 9rem;
}
.inbox-settings__select {
    min-height: 2.5rem;
    padding-block: 0.4rem;
    font-size: 0.8125rem;
}
.inbox-settings__replace {
    margin-top: 0.75rem;
}
.inbox-settings__activation {
    display: grid;
    min-height: 13rem;
    grid-template-columns: 3rem minmax(0, 1fr);
    align-content: center;
    align-items: center;
    gap: 0.875rem 1rem;
    padding: 1.25rem;
    border: 1px dashed #685276;
    border-radius: 0.875rem;
    background: #ffffff02;
}
.inbox-settings__activation > span {
    width: 3rem;
    height: 3rem;
    border-radius: 0.75rem;
}
.inbox-settings__activation h3 {
    margin: 0;
    font-size: 1rem;
}
.inbox-settings__activation p {
    margin: 0.375rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
    line-height: 1.55;
}
.inbox-settings__activation .fb-button {
    width: max-content;
    grid-column: 2;
    margin-top: 0.25rem;
}
.inbox-settings__wizard {
    display: grid;
    gap: 1rem;
}
.inbox-settings__warning {
    display: flex;
    gap: 0.75rem;
    margin-bottom: 1rem;
    padding: 0.875rem;
    border-color: color-mix(in srgb, var(--fb-warning) 38%, transparent);
}
.inbox-settings__warning > .fb-icon {
    margin-top: 0.125rem;
    flex: none;
    color: var(--fb-warning);
}
.inbox-settings__warning p {
    margin: 0;
}
.inbox-settings__warning strong {
    display: block;
    margin-bottom: 0.25rem;
    color: var(--fb-text);
}
.inbox-settings__wizard-heading > span {
    color: var(--fb-accent-text);
    font-size: 0.6875rem;
    font-weight: 650;
    letter-spacing: 0.1em;
    text-transform: uppercase;
}
.inbox-settings__wizard-heading h3 {
    margin: 0.25rem 0 0;
    font-size: 1rem;
}
.inbox-settings__custody {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.625rem;
}
.inbox-settings__custody-card {
    position: relative;
    display: grid;
    grid-template-columns: 2.25rem minmax(0, 1fr);
    gap: 0.75rem;
    padding: 0.875rem 2rem 0.875rem 0.875rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.75rem;
    background: var(--fb-surface-sunken);
    cursor: pointer;
    transition:
        border-color var(--fb-duration-control) ease,
        background var(--fb-duration-control) ease,
        transform var(--fb-duration-control) var(--fb-ease);
}
.inbox-settings__custody-card input.sr-only {
    position: absolute;
    z-index: 2;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    clip: auto;
    clip-path: none;
    opacity: 0;
    cursor: pointer;
}
.inbox-settings__custody-card:hover {
    border-color: #725b80;
    transform: translateY(-1px);
}
.inbox-settings__custody-card:focus-within {
    outline: 2px solid var(--fb-focus);
    outline-offset: 2px;
}
.inbox-settings__custody-card--selected {
    border-color: #806199;
    background: var(--fb-selected-surface);
}
.inbox-settings__custody-icon {
    display: grid;
    width: 2.25rem;
    height: 2.25rem;
    place-items: center;
    border-radius: 0.625rem;
    color: var(--fb-accent-text);
    background: #ffffff07;
}
.inbox-settings__custody-card strong,
.inbox-settings__custody-card strong + span {
    display: block;
}
.inbox-settings__custody-card strong {
    font-size: 0.8125rem;
}
.inbox-settings__custody-card strong small {
    display: inline;
    margin-left: 0.25rem;
    color: var(--fb-success);
    font-size: 0.6875rem;
    font-weight: 500;
}
.inbox-settings__custody-card strong + span {
    margin-top: 0.375rem;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    line-height: 1.5;
}
.inbox-settings__radio {
    position: absolute;
    top: 1rem;
    right: 0.75rem;
    width: 0.75rem;
    height: 0.75rem;
    border: 1px solid var(--fb-control-border);
    border-radius: 999px;
    box-shadow: inset 0 0 0 2px var(--fb-surface-sunken);
    transition:
        border-color var(--fb-duration-control) ease,
        background var(--fb-duration-control) ease;
}
.inbox-settings__custody-card--selected .inbox-settings__radio {
    border-color: var(--fb-accent-text);
    background: var(--fb-accent-text);
}
.inbox-settings__acknowledgement {
    display: flex;
    align-items: start;
    gap: 0.625rem;
    padding-block: 0.25rem;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
}
.inbox-settings__acknowledgement input {
    margin-top: 0.2rem;
    accent-color: var(--fb-brand-bright);
}
.inbox-settings__wizard-actions,
.inbox-settings__export-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.625rem;
}
.inbox-settings__key-form {
    display: grid;
    gap: 0.75rem;
    padding-top: 1rem;
    border-top: 1px solid #ffffff0a;
}
.inbox-settings__key-form > label {
    color: var(--fb-text);
    font-size: 0.8125rem;
    font-weight: 600;
}
.inbox-settings__key-form > p {
    margin: 0;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
}
.inbox-settings__feedback {
    margin: 1rem 0 0;
    padding: 0.75rem;
    border: 1px solid;
    border-radius: 0.625rem;
    font-size: 0.8125rem;
}
.inbox-settings__feedback--success {
    border-color: color-mix(in srgb, var(--fb-success) 35%, transparent);
    color: var(--fb-success);
    background: color-mix(in srgb, var(--fb-success) 7%, transparent);
}
.inbox-settings__feedback--error {
    border-color: color-mix(in srgb, var(--fb-danger) 35%, transparent);
    color: var(--fb-danger);
    background: color-mix(in srgb, var(--fb-danger) 7%, transparent);
}
@media (max-width: 900px) {
    .inbox-settings__custody {
        grid-template-columns: 1fr;
    }
}
@media (max-width: 560px) {
    .inbox-settings {
        padding: 1.375rem;
    }
    .inbox-settings__header {
        grid-template-columns: 2.5rem minmax(0, 1fr);
    }
    .inbox-settings__header .fb-button {
        width: 100%;
        grid-column: 1 / -1;
    }
    .inbox-settings__setting-row--select {
        grid-template-columns: 1fr;
    }
    .inbox-settings__activation {
        grid-template-columns: 1fr;
        text-align: left;
    }
    .inbox-settings__activation .fb-button {
        width: 100%;
        grid-column: 1;
    }
    .inbox-settings__export-actions .fb-button,
    .inbox-settings__export-actions :deep(.copy-button),
    .inbox-settings__export-actions :deep(.copy-button .fb-button) {
        width: 100%;
    }
}
@media (prefers-reduced-motion: reduce) {
    .inbox-settings__state-enter-active,
    .inbox-settings__state-leave-active,
    .inbox-settings__custody-card,
    .inbox-settings__radio {
        transition: none;
    }
}
</style>
