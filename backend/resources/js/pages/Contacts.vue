<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import {
    CollapsibleContent,
    CollapsibleRoot,
    CollapsibleTrigger,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuPortal,
    DropdownMenuRoot,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
    TabsIndicator,
    TabsList,
    TabsContent,
    TabsRoot,
    TabsTrigger,
} from 'reka-ui';
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { RouteSurface } from '@filebeam/ui';
import {
    contactRequest,
    overrideValue,
    type Contact,
    type Contacts,
    type ReceivingDefaults,
} from '../../../../ui/src/lib/contacts';
import ReceivingDefaultsComponent from '../../../../ui/src/components/auth/ReceivingDefaults.vue';
import AccountPage from '../../../../ui/src/components/layout/AccountPage.vue';
import AnimatedReveal from '../../../../ui/src/components/layout/AnimatedReveal.vue';
import AnimatedHeight from '../../../../ui/src/components/layout/AnimatedHeight.vue';
import AppLink from '../../../../ui/src/components/primitives/AppLink.vue';
import BrandLogo from '../../../../ui/src/components/brand/BrandLogo.vue';
import ContactIdentity from '../../../../ui/src/components/contacts/ContactIdentity.vue';
import ContactConfirmation from '../../../../ui/src/components/contacts/ContactConfirmation.vue';
import Button from '../../../../ui/src/components/primitives/Button.vue';
import FormField from '../../../../ui/src/components/primitives/FormField.vue';
import Icon from '../../../../ui/src/components/primitives/Icon.vue';
import Input from '../../../../ui/src/components/primitives/Input.vue';
import Select from '../../../../ui/src/components/primitives/Select.vue';

defineOptions({ layout: RouteSurface });
type View = 'friends' | 'requests' | 'blocked';
type Region = {
    id: number;
    kind: 'preferences' | 'remove' | 'block' | 'unblock';
    returnId: string;
} | null;
type PreferenceDraft = Pick<Contact, 'canSend' | 'autoDownload'>;
const data = ref<Contacts>();
const loadError = ref('');
const view = ref<View>('friends');
const region = ref<Region>(null);
const query = ref('');
const addOpen = ref(false);
const username = ref('');
const addError = ref('');
const addSuccess = ref('');
const addPending = ref(false);
const pending = ref(new Set<number>());
const preferenceDrafts = ref<Record<number, PreferenceDraft>>({});
const rowErrors = ref<Record<number, string>>({});
const defaultsPending = ref(false);
const defaultsError = ref('');
const defaultsOpen = ref(false);
const announcement = ref('');
let alive = true;
let readEpoch = 0;
let writeEpoch = 0;
let queue = Promise.resolve();

const contacts = computed(() => data.value?.contacts ?? []);
const friends = computed(() => contacts.value.filter((contact) => contact.status === 'accepted'));
const incoming = computed(() => contacts.value.filter((contact) => contact.status === 'incoming'));
const outgoing = computed(() => contacts.value.filter((contact) => contact.status === 'outgoing'));
const filteredFriends = computed(() => {
    const needle = query.value.trim().toLowerCase();
    return needle
        ? friends.value.filter((contact) =>
              `${contact.name} ${contact.username}`.toLowerCase().includes(needle),
          )
        : friends.value;
});
const showSearch = computed(() => friends.value.length > 8);
const emptyFriends = computed(
    () => data.value && !friends.value.length && view.value === 'friends',
);
const showAddForm = computed(
    () => addOpen.value && !emptyFriends.value && view.value !== 'blocked',
);
const normalizedUsername = computed(() => username.value.trim().replace(/^@/, '').toLowerCase());
function display(contact: { name: string; username: string }): string {
    return contact.name || contact.username;
}
function initials(contact: { name: string; username: string }): string {
    return display(contact)
        .split(/\s+/)
        .filter(Boolean)
        .slice(0, 2)
        .map((part) => part[0])
        .join('')
        .toUpperCase();
}
function isCustom(contact: Contact): boolean {
    return contact.canSend !== null || contact.autoDownload !== null;
}
function override(value: boolean | null): string {
    return value === null ? 'inherit' : value ? 'allow' : 'deny';
}
function summary(settings: ReceivingDefaults): string {
    const policy = {
        anyone: 'Anyone can send',
        authenticated: 'Signed-in users can send',
        friends: 'Friends can send',
        nobody: 'Only explicitly allowed friends',
    }[settings.receivingPolicy];
    return `${policy} · Auto-download ${settings.autoDownloadFriends ? 'on' : 'off'}`;
}
function announce(message: string): void {
    announcement.value = '';
    void nextTick(() => (announcement.value = message));
}
function apply(next: Contacts): void {
    if (!alive || (data.value && next.settings.revision < data.value.settings.revision)) return;
    data.value = next;
}
async function load(): Promise<void> {
    const request = ++readEpoch;
    loadError.value = '';
    try {
        const next = await contactRequest<Contacts>('contacts');
        if (!alive || request !== readEpoch) return;
        apply(next);
    } catch (reason) {
        if (alive && request === readEpoch)
            loadError.value = reason instanceof Error ? reason.message : 'Could not load contacts.';
    }
}
function enqueue<T>(work: () => Promise<T>): Promise<T> {
    const result = queue.then(work, work);
    queue = result.then(
        () => undefined,
        () => undefined,
    );
    return result;
}
async function reconcile(): Promise<boolean> {
    try {
        const next = await contactRequest<Contacts>('contacts');
        if (!alive) return false;
        apply(next);
        return true;
    } catch {
        return false;
    }
}
function uncertain(reason: unknown): boolean {
    const message = reason instanceof Error ? reason.message : '';
    return /could not be completed|failed to fetch|network|timeout|abort/i.test(message);
}
function message(reason: unknown, fallback: string): string {
    return reason instanceof Error && reason.message ? reason.message : fallback;
}
async function mutate(
    contact: Contact,
    action: string,
    values?: Pick<Contact, 'canSend' | 'autoDownload'>,
): Promise<void> {
    if (pending.value.has(contact.id)) return;
    pending.value = new Set(pending.value).add(contact.id);
    rowErrors.value = { ...rowErrors.value, [contact.id]: '' };
    ++writeEpoch;
    readEpoch++;
    await enqueue(async () => {
        if (!alive) return;
        try {
            const next = await contactRequest<Contacts>(`contacts/${contact.username}`, 'POST', {
                action,
                canSend: values?.canSend ?? null,
                autoDownload: values?.autoDownload ?? null,
            });
            if (!alive) return;
            apply(next);
            if (action === 'accept') announce(`${display(contact)} is now a friend.`);
            else if (action === 'decline') announce(`Request from ${display(contact)} declined.`);
            else if (action === 'cancel') announce(`Request to @${contact.username} cancelled.`);
            else if (action === 'remove') announce(`${display(contact)} removed from friends.`);
            else if (action === 'block') announce(`@${contact.username} blocked.`);
            else if (action === 'unblock') announce(`@${contact.username} unblocked.`);
            else if (action === 'preferences')
                announce(`Receiving preferences for ${display(contact)} saved.`);
            if (action === 'preferences') {
                const nextDrafts = { ...preferenceDrafts.value };
                delete nextDrafts[contact.id];
                preferenceDrafts.value = nextDrafts;
            }
            if (action !== 'preferences' && region.value?.id === contact.id) {
                region.value = null;
                focus(
                    view.value === 'blocked'
                        ? 'blocked-accounts-link'
                        : `contacts-tab-${view.value}`,
                );
            }
        } catch (reason) {
            if (!alive) return;
            const refreshed = uncertain(reason) ? await reconcile() : false;
            rowErrors.value = {
                ...rowErrors.value,
                [contact.id]: uncertain(reason)
                    ? refreshed
                        ? "Couldn't confirm this change. Contacts were refreshed."
                        : "Couldn't confirm this change. Check your connection and try again."
                    : message(reason, 'Could not update contact.'),
            };
            if (action === 'preferences' && (!uncertain(reason) || refreshed)) {
                const nextDrafts = { ...preferenceDrafts.value };
                delete nextDrafts[contact.id];
                preferenceDrafts.value = nextDrafts;
            }
        } finally {
            if (alive)
                pending.value = new Set([...pending.value].filter((id) => id !== contact.id));
        }
    });
}
async function sendRequest(): Promise<void> {
    if (addPending.value || !alive) return;
    addError.value = '';
    addSuccess.value = '';
    const normalized = normalizedUsername.value;
    if (!/^[a-z0-9_]{3,24}$/.test(normalized)) {
        addError.value = 'Enter an exact username, such as @alice.';
        return;
    }
    const duplicate = contacts.value.find((contact) => contact.username === normalized);
    if (duplicate) {
        addError.value =
            duplicate.status === 'accepted'
                ? 'You are already friends with this account.'
                : duplicate.status === 'incoming'
                  ? 'This account has already requested to be your friend.'
                  : 'A request to this account is already pending.';
        return;
    }
    addPending.value = true;
    ++writeEpoch;
    readEpoch++;
    await enqueue(async () => {
        if (!alive) return;
        try {
            const next = await contactRequest<Contacts>(`contacts/${normalized}`, 'POST', {
                action: 'request',
                canSend: null,
                autoDownload: null,
            });
            if (!alive) return;
            apply(next);
            username.value = '';
            addSuccess.value = `Request sent to @${normalized}`;
            announce(addSuccess.value);
        } catch (reason) {
            if (!alive) return;
            const refreshed = uncertain(reason) ? await reconcile() : false;
            addError.value = uncertain(reason)
                ? refreshed
                    ? "Couldn't confirm this request. Contacts were refreshed."
                    : "Couldn't confirm this request. Check your connection and try again."
                : message(reason, 'Could not send request.');
        } finally {
            if (alive) addPending.value = false;
        }
    });
}
function setPreference(contact: Contact, field: 'canSend' | 'autoDownload', value: string): void {
    const draft = preferenceDrafts.value[contact.id] ?? {
        canSend: contact.canSend,
        autoDownload: contact.autoDownload,
    };
    const next = { ...draft, [field]: overrideValue(value) };
    preferenceDrafts.value = { ...preferenceDrafts.value, [contact.id]: next };
    void mutate(contact, 'preferences', {
        canSend: next.canSend,
        autoDownload: next.autoDownload,
    });
}
function resetPreferences(contact: Contact): void {
    preferenceDrafts.value = {
        ...preferenceDrafts.value,
        [contact.id]: { canSend: null, autoDownload: null },
    };
    void mutate(contact, 'preferences', { canSend: null, autoDownload: null });
}
function draftFor(contact: Contact): PreferenceDraft {
    return preferenceDrafts.value[contact.id] ?? contact;
}
function focus(id: string): void {
    void nextTick(() => document.getElementById(id)?.focus());
}
function openRegion(contact: Contact, kind: NonNullable<Region>['kind'], returnId?: string): void {
    region.value = { id: contact.id, kind, returnId: returnId ?? `contact-${contact.id}-identity` };
    rowErrors.value = { ...rowErrors.value, [contact.id]: '' };
    focus(
        kind === 'preferences' ? `contact-${contact.id}-identity` : `contact-${contact.id}-cancel`,
    );
}
function closeRegion(): void {
    if (region.value && pending.value.has(region.value.id)) return;
    const returnId = region.value?.returnId;
    region.value = null;
    if (returnId) focus(returnId);
}
function closeAdd(): void {
    addOpen.value = false;
    focus('add-friend-trigger');
}
function toggleAdd(): void {
    if (addOpen.value) closeAdd();
    else {
        addOpen.value = true;
        focus('add-friend');
    }
}
function onAddKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
        event.preventDefault();
        closeAdd();
    }
}
function profile(contact: Contact): void {
    const url = new URL(`/u/${contact.username}`, window.location.origin).href;
    void navigator.clipboard?.writeText(url).then(
        () => announce('Profile link copied.'),
        () => announce('Could not copy the profile link.'),
    );
}
async function saveDefaults(next: ReceivingDefaults): Promise<void> {
    if (defaultsPending.value || !alive) return;
    defaultsPending.value = true;
    defaultsError.value = '';
    ++writeEpoch;
    readEpoch++;
    await enqueue(async () => {
        if (!alive) return;
        try {
            const settings = await contactRequest<ReceivingDefaults>('account/receiving', 'PATCH', {
                receivingPolicy: next.receivingPolicy,
                autoDownloadFriends: next.autoDownloadFriends,
            });
            if (!alive) return;
            if (data.value && settings.revision >= data.value.settings.revision)
                data.value = { ...data.value, settings };
            const contacts = await contactRequest<Contacts>('contacts');
            if (!alive) return;
            apply(contacts);
            announce('Receiving preferences saved.');
        } catch (reason) {
            if (!alive) return;
            const refreshed = uncertain(reason) ? await reconcile() : false;
            defaultsError.value = uncertain(reason)
                ? refreshed
                    ? "Couldn't confirm this change. Contacts were refreshed."
                    : "Couldn't confirm this change. Check your connection and try again."
                : message(reason, 'Could not save receiving preferences.');
        } finally {
            if (alive) defaultsPending.value = false;
        }
    });
}
onMounted(load);
watch([view, emptyFriends], () => {
    if (view.value === 'blocked' || emptyFriends.value) addOpen.value = false;
});
onBeforeUnmount(() => {
    alive = false;
    readEpoch++;
    writeEpoch++;
});
</script>

<template>
    <div>
        <Head title="Contacts" />
        <AccountPage title="Contacts" description="Your friends on this instance.">
            <template #actions
                ><AppLink href="/account/inbox" class="fb-button fb-button--secondary"
                    ><Icon name="folder" :size="16" />Open inbox</AppLink
                ></template
            >
            <section class="contacts-panel" aria-label="Contacts">
                <p v-if="loadError" class="contacts-error" role="alert">
                    <Icon name="alert" :size="17" />{{ loadError }}
                    <Button variant="ghost" @click="load">Retry</Button>
                </p>
                <template v-else-if="!data"
                    ><div class="contacts-loading" role="status">
                        <Icon name="loader" :size="20" />Loading contacts...
                    </div></template
                >
                <template v-else>
                    <TabsRoot v-model="view" class="contacts-tabs-root">
                        <header class="contacts-toolbar">
                            <template v-if="view === 'blocked'"
                                ><Button variant="ghost" class="back" @click="view = 'friends'"
                                    ><Icon name="arrow-left" :size="16" />Back</Button
                                ><strong>Blocked accounts</strong></template
                            >
                            <template v-else>
                                <div class="contacts-tabs">
                                    <TabsList
                                        class="contacts-tabs__list"
                                        aria-label="Contacts views"
                                        ><TabsIndicator
                                            class="contacts-tabs__indicator"
                                        /><TabsTrigger
                                            value="friends"
                                            id="contacts-tab-friends"
                                            class="contacts-tabs__trigger"
                                            >Friends
                                            <span v-if="friends.length" class="quiet-count">{{
                                                friends.length
                                            }}</span></TabsTrigger
                                        ><TabsTrigger
                                            value="requests"
                                            id="contacts-tab-requests"
                                            class="contacts-tabs__trigger"
                                            :aria-label="
                                                incoming.length
                                                    ? `Requests, ${incoming.length} incoming`
                                                    : 'Requests'
                                            "
                                            ><span>Requests</span
                                            ><span
                                                v-if="incoming.length"
                                                class="request-count"
                                                aria-hidden="true"
                                                >{{ incoming.length }}</span
                                            ></TabsTrigger
                                        ></TabsList
                                    >
                                </div>
                                <label
                                    v-if="showSearch && view === 'friends'"
                                    class="contacts-search"
                                    ><Icon name="search" :size="15" /><span class="sr-only"
                                        >Find a friend</span
                                    ><Input
                                        v-model="query"
                                        type="search"
                                        placeholder="Find a friend" /><button
                                        v-if="query"
                                        type="button"
                                        aria-label="Clear search"
                                        @click="query = ''"
                                    >
                                        <Icon name="x" :size="14" /></button
                                ></label>
                                <Button
                                    v-if="!emptyFriends"
                                    id="add-friend-trigger"
                                    :variant="addOpen ? 'secondary' : 'primary'"
                                    @click="toggleAdd"
                                    ><Icon name="plus" :size="16" />Add friend</Button
                                >
                            </template>
                        </header>
                        <AnimatedReveal :show="showAddForm">
                            <form
                                v-if="showAddForm"
                                class="add-form add-form--inline"
                                aria-label="Add friend toolbar"
                                @keydown="onAddKeydown"
                                @submit.prevent="sendRequest"
                            >
                                <FormField
                                    id="add-friend"
                                    label="Username"
                                    :error="addError"
                                    :description="
                                        addSuccess || 'Use their exact username on this instance.'
                                    "
                                    ><template #default="field"
                                        ><div class="add-form__row">
                                            <span class="at">@</span
                                            ><Input
                                                v-bind="field"
                                                v-model="username"
                                                placeholder="username"
                                                autocomplete="off"
                                                autocapitalize="none"
                                                :disabled="addPending"
                                            /><Button
                                                type="submit"
                                                :disabled="!username.trim() || addPending"
                                                ><Icon
                                                    :name="addPending ? 'loader' : 'plus'"
                                                    :size="16"
                                                />{{
                                                    addPending ? 'Sending...' : 'Send request'
                                                }}</Button
                                            ><Button
                                                variant="ghost"
                                                icon
                                                aria-label="Close add friend form"
                                                @click="closeAdd"
                                                ><Icon name="x" :size="16"
                                            /></Button></div></template></FormField
                                ><button
                                    v-if="addSuccess"
                                    type="button"
                                    class="inline-link"
                                    @click="
                                        view = 'requests';
                                        addOpen = false;
                                    "
                                >
                                    View requests
                                </button>
                            </form>
                        </AnimatedReveal>
                        <AnimatedHeight class="contacts-stage-height">
                            <div class="contacts-stage-stack">
                                <TabsContent
                                    value="friends"
                                    class="contacts-stage"
                                    :inert="view !== 'friends' ? '' : undefined"
                                    :aria-hidden="view !== 'friends'"
                                >
                                    <div v-if="!friends.length" class="contacts-empty">
                                        <BrandLogo compact />
                                        <h2>Add your first friend</h2>
                                        <p>Send files directly to people you know.</p>
                                        <form
                                            class="add-form"
                                            aria-label="Send request form"
                                            @submit.prevent="sendRequest"
                                        >
                                            <FormField
                                                id="first-friend"
                                                label="Username"
                                                :error="addError"
                                                description="Use their exact username on this instance."
                                                ><template #default="field"
                                                    ><div class="add-form__row">
                                                        <span class="at">@</span
                                                        ><Input
                                                            v-bind="field"
                                                            v-model="username"
                                                            placeholder="username"
                                                            autocomplete="off"
                                                            autocapitalize="none"
                                                            :disabled="addPending"
                                                        /><Button
                                                            type="submit"
                                                            :disabled="
                                                                !username.trim() || addPending
                                                            "
                                                            >{{
                                                                addPending
                                                                    ? 'Sending...'
                                                                    : 'Send request'
                                                            }}</Button
                                                        >
                                                    </div></template
                                                ></FormField
                                            >
                                            <p v-if="addSuccess" class="add-success" role="status">
                                                {{ addSuccess }}
                                            </p>
                                            <button
                                                v-if="addSuccess"
                                                type="button"
                                                class="inline-link"
                                                @click="view = 'requests'"
                                            >
                                                View requests
                                            </button>
                                        </form>
                                    </div>
                                    <template v-else
                                        ><div
                                            v-if="showSearch && !filteredFriends.length"
                                            class="contacts-empty contacts-empty--small"
                                        >
                                            <h2>No matching friends</h2>
                                            <Button variant="ghost" @click="query = ''"
                                                >Clear search</Button
                                            >
                                        </div>
                                        <TransitionGroup
                                            v-else
                                            name="contacts-row"
                                            tag="ul"
                                            class="contacts-list"
                                        >
                                            <li
                                                v-for="contact in filteredFriends"
                                                :key="contact.id"
                                                class="contact"
                                            >
                                                <div class="contact-row">
                                                    <ContactIdentity
                                                        :id="`contact-${contact.id}-identity`"
                                                        :name="contact.name"
                                                        :username="contact.username"
                                                        interactive
                                                        :expanded="
                                                            region?.id === contact.id &&
                                                            region.kind === 'preferences'
                                                        "
                                                        :status="
                                                            isCustom(contact)
                                                                ? contact.canSend === false
                                                                    ? 'Receiving off'
                                                                    : 'Custom preferences'
                                                                : ''
                                                        "
                                                        @activate="
                                                            openRegion(contact, 'preferences')
                                                        "
                                                    />
                                                    <AppLink
                                                        :href="`/u/${contact.username}`"
                                                        class="fb-button fb-button--secondary send-link"
                                                        ><Icon name="upload" :size="15" />Send
                                                        files</AppLink
                                                    ><DropdownMenuRoot :modal="false"
                                                        ><DropdownMenuTrigger as-child
                                                            ><Button
                                                                variant="ghost"
                                                                icon
                                                                :id="`contact-${contact.id}-actions`"
                                                                :aria-label="`${display(contact)} actions`"
                                                                ><Icon
                                                                    name="menu"
                                                                    :size="
                                                                        18
                                                                    " /></Button></DropdownMenuTrigger
                                                        ><DropdownMenuPortal
                                                            ><DropdownMenuContent
                                                                class="fb-select-content contacts-menu"
                                                                :side-offset="8"
                                                                align="end"
                                                                :collision-padding="8"
                                                                ><DropdownMenuItem
                                                                    class="fb-select-item"
                                                                    @select="
                                                                        openRegion(
                                                                            contact,
                                                                            'preferences',
                                                                            `contact-${contact.id}-actions`,
                                                                        )
                                                                    "
                                                                    >Receiving
                                                                    preferences</DropdownMenuItem
                                                                ><DropdownMenuItem
                                                                    class="fb-select-item"
                                                                    @select="profile(contact)"
                                                                    >Copy profile
                                                                    link</DropdownMenuItem
                                                                ><DropdownMenuSeparator
                                                                    class="contacts-menu__separator"
                                                                /><DropdownMenuItem
                                                                    class="fb-select-item"
                                                                    @select="
                                                                        openRegion(
                                                                            contact,
                                                                            'remove',
                                                                            `contact-${contact.id}-actions`,
                                                                        )
                                                                    "
                                                                    >Remove friend</DropdownMenuItem
                                                                ><DropdownMenuItem
                                                                    class="fb-select-item contacts-menu__danger"
                                                                    @select="
                                                                        openRegion(
                                                                            contact,
                                                                            'block',
                                                                            `contact-${contact.id}-actions`,
                                                                        )
                                                                    "
                                                                    >Block account</DropdownMenuItem
                                                                ></DropdownMenuContent
                                                            ></DropdownMenuPortal
                                                        ></DropdownMenuRoot
                                                    >
                                                </div>
                                                <div
                                                    v-if="
                                                        region?.id === contact.id &&
                                                        region.kind === 'preferences'
                                                    "
                                                    class="contact-region"
                                                    @keydown.escape.stop.prevent="closeRegion"
                                                >
                                                    <div class="region-heading">
                                                        <div>
                                                            <h3>
                                                                Receiving from
                                                                {{ display(contact).split(' ')[0] }}
                                                            </h3>
                                                            <p>
                                                                Only changes what @{{
                                                                    contact.username
                                                                }}
                                                                can send to you.
                                                            </p>
                                                        </div>
                                                        <Button
                                                            v-if="isCustom(contact)"
                                                            variant="ghost"
                                                            :disabled="pending.has(contact.id)"
                                                            @click="resetPreferences(contact)"
                                                            >Use defaults</Button
                                                        >
                                                    </div>
                                                    <div class="preference-fields">
                                                        <FormField
                                                            :id="`can-send-${contact.id}`"
                                                            label="Can send me files"
                                                            :description="
                                                                contact.effective.canSend
                                                                    ? 'Currently allowed.'
                                                                    : 'Currently not allowed.'
                                                            "
                                                            ><template #default="field"
                                                                ><Select
                                                                    v-bind="field"
                                                                    :model-value="
                                                                        override(
                                                                            draftFor(contact)
                                                                                .canSend,
                                                                        )
                                                                    "
                                                                    :disabled="
                                                                        pending.has(contact.id)
                                                                    "
                                                                    :options="[
                                                                        {
                                                                            value: 'inherit',
                                                                            label: 'Use account default',
                                                                        },
                                                                        {
                                                                            value: 'allow',
                                                                            label: 'Allow',
                                                                        },
                                                                        {
                                                                            value: 'deny',
                                                                            label: 'Don\'t allow',
                                                                        },
                                                                    ]"
                                                                    @update:model-value="
                                                                        setPreference(
                                                                            contact,
                                                                            'canSend',
                                                                            $event,
                                                                        )
                                                                    " /></template></FormField
                                                        ><FormField
                                                            :id="`auto-download-${contact.id}`"
                                                            label="Automatic download"
                                                            :description="
                                                                contact.effective.autoDownload
                                                                    ? 'Currently on.'
                                                                    : 'Currently off. Automatic receiving also requires an accepted friendship and a client that opts in.'
                                                            "
                                                            ><template #default="field"
                                                                ><Select
                                                                    v-bind="field"
                                                                    :model-value="
                                                                        override(
                                                                            draftFor(contact)
                                                                                .autoDownload,
                                                                        )
                                                                    "
                                                                    :disabled="
                                                                        pending.has(contact.id)
                                                                    "
                                                                    :options="[
                                                                        {
                                                                            value: 'inherit',
                                                                            label: 'Use account default',
                                                                        },
                                                                        {
                                                                            value: 'allow',
                                                                            label: 'On',
                                                                        },
                                                                        {
                                                                            value: 'deny',
                                                                            label: 'Off',
                                                                        },
                                                                    ]"
                                                                    @update:model-value="
                                                                        setPreference(
                                                                            contact,
                                                                            'autoDownload',
                                                                            $event,
                                                                        )
                                                                    " /></template
                                                        ></FormField>
                                                    </div>
                                                    <p v-if="pending.has(contact.id)" role="status">
                                                        Saving preferences...
                                                    </p>
                                                    <p v-if="rowErrors[contact.id]" role="alert">
                                                        {{ rowErrors[contact.id] }}
                                                    </p>
                                                </div>
                                                <ContactConfirmation
                                                    v-if="
                                                        region?.id === contact.id &&
                                                        (region.kind === 'remove' ||
                                                            region.kind === 'block')
                                                    "
                                                    class="contact-region confirmation"
                                                    :cancel-id="`contact-${contact.id}-cancel`"
                                                    :title="
                                                        region.kind === 'remove'
                                                            ? `Remove ${display(contact).split(' ')[0]} as a friend?`
                                                            : `Block @${contact.username}?`
                                                    "
                                                    :description="
                                                        region.kind === 'remove'
                                                            ? 'Becoming friends again requires a new request. This does not necessarily prevent transfers allowed by account policy.'
                                                            : 'This prevents new transfers and friend requests under the existing rules and removes the friendship.'
                                                    "
                                                    :action="
                                                        region.kind === 'remove'
                                                            ? 'Remove friend'
                                                            : 'Block account'
                                                    "
                                                    :pending="pending.has(contact.id)"
                                                    :error="rowErrors[contact.id]"
                                                    danger
                                                    @cancel="closeRegion"
                                                    @confirm="mutate(contact, region!.kind)"
                                                /></li></TransitionGroup
                                    ></template>
                                </TabsContent>
                                <TabsContent
                                    value="requests"
                                    class="contacts-stage"
                                    :inert="view !== 'requests' ? '' : undefined"
                                    :aria-hidden="view !== 'requests'"
                                >
                                    <div
                                        v-if="!incoming.length && !outgoing.length"
                                        class="contacts-empty contacts-empty--small"
                                    >
                                        <h2>No requests</h2>
                                        <p>Friend requests will appear here.</p>
                                    </div>
                                    <template v-else
                                        ><section v-if="incoming.length" class="request-group">
                                            <h2>
                                                Incoming <span>{{ incoming.length }}</span>
                                            </h2>
                                            <ul class="contacts-list">
                                                <li
                                                    v-for="contact in incoming"
                                                    :key="contact.id"
                                                    class="contact"
                                                >
                                                    <div class="contact-row">
                                                        <div class="contact-identity">
                                                            <span class="avatar">{{
                                                                initials(contact)
                                                            }}</span
                                                            ><span
                                                                ><strong>{{
                                                                    display(contact)
                                                                }}</strong
                                                                ><small
                                                                    >@{{ contact.username }}</small
                                                                ></span
                                                            >
                                                        </div>
                                                        <Button
                                                            :disabled="pending.has(contact.id)"
                                                            @click="mutate(contact, 'accept')"
                                                            >Accept</Button
                                                        ><Button
                                                            variant="secondary"
                                                            :disabled="pending.has(contact.id)"
                                                            @click="mutate(contact, 'decline')"
                                                            >Decline</Button
                                                        ><DropdownMenuRoot :modal="false"
                                                            ><DropdownMenuTrigger as-child
                                                                ><Button
                                                                    variant="ghost"
                                                                    icon
                                                                    :id="`contact-${contact.id}-actions`"
                                                                    :aria-label="`${display(contact)} actions`"
                                                                    ><Icon
                                                                        name="menu"
                                                                        :size="
                                                                            18
                                                                        " /></Button></DropdownMenuTrigger
                                                            ><DropdownMenuPortal
                                                                ><DropdownMenuContent
                                                                    class="fb-select-content contacts-menu"
                                                                    :side-offset="8"
                                                                    align="end"
                                                                    :collision-padding="8"
                                                                    ><DropdownMenuItem
                                                                        class="fb-select-item contacts-menu__danger"
                                                                        @select="
                                                                            openRegion(
                                                                                contact,
                                                                                'block',
                                                                                `contact-${contact.id}-actions`,
                                                                            )
                                                                        "
                                                                        >Block
                                                                        account</DropdownMenuItem
                                                                    ></DropdownMenuContent
                                                                ></DropdownMenuPortal
                                                            ></DropdownMenuRoot
                                                        >
                                                    </div>
                                                    <p
                                                        v-if="rowErrors[contact.id]"
                                                        class="row-error"
                                                        role="alert"
                                                    >
                                                        {{ rowErrors[contact.id] }}
                                                    </p>
                                                    <div
                                                        v-if="
                                                            region?.id === contact.id &&
                                                            region.kind === 'block'
                                                        "
                                                        class="contact-region confirmation"
                                                        @keydown.escape.stop.prevent="closeRegion"
                                                    >
                                                        <div>
                                                            <h3>Block @{{ contact.username }}?</h3>
                                                            <p>
                                                                This removes the pending request and
                                                                prevents new transfers and friend
                                                                requests.
                                                            </p>
                                                        </div>
                                                        <div>
                                                            <Button
                                                                :id="`contact-${contact.id}-cancel`"
                                                                variant="secondary"
                                                                :disabled="pending.has(contact.id)"
                                                                @click="closeRegion"
                                                                >Cancel</Button
                                                            ><Button
                                                                variant="danger"
                                                                :disabled="pending.has(contact.id)"
                                                                @click="mutate(contact, 'block')"
                                                                >Block account</Button
                                                            >
                                                        </div>
                                                    </div>
                                                </li>
                                            </ul>
                                        </section>
                                        <section v-if="outgoing.length" class="request-group">
                                            <h2>
                                                Sent <span>{{ outgoing.length }}</span>
                                            </h2>
                                            <ul class="contacts-list">
                                                <li
                                                    v-for="contact in outgoing"
                                                    :key="contact.id"
                                                    class="contact"
                                                >
                                                    <div class="contact-row">
                                                        <div class="contact-identity">
                                                            <span class="avatar">{{
                                                                initials(contact)
                                                            }}</span
                                                            ><span
                                                                ><strong>{{
                                                                    display(contact)
                                                                }}</strong
                                                                ><small
                                                                    >@{{ contact.username }}</small
                                                                ></span
                                                            >
                                                        </div>
                                                        <Button
                                                            variant="secondary"
                                                            :disabled="pending.has(contact.id)"
                                                            @click="mutate(contact, 'cancel')"
                                                            >Cancel request</Button
                                                        ><DropdownMenuRoot :modal="false"
                                                            ><DropdownMenuTrigger as-child
                                                                ><Button
                                                                    variant="ghost"
                                                                    icon
                                                                    :id="`contact-${contact.id}-actions`"
                                                                    :aria-label="`${display(contact)} actions`"
                                                                    ><Icon
                                                                        name="menu"
                                                                        :size="
                                                                            18
                                                                        " /></Button></DropdownMenuTrigger
                                                            ><DropdownMenuPortal
                                                                ><DropdownMenuContent
                                                                    class="fb-select-content contacts-menu"
                                                                    :side-offset="8"
                                                                    align="end"
                                                                    :collision-padding="8"
                                                                    ><DropdownMenuItem
                                                                        class="fb-select-item contacts-menu__danger"
                                                                        @select="
                                                                            openRegion(
                                                                                contact,
                                                                                'block',
                                                                                `contact-${contact.id}-actions`,
                                                                            )
                                                                        "
                                                                        >Block
                                                                        account</DropdownMenuItem
                                                                    ></DropdownMenuContent
                                                                ></DropdownMenuPortal
                                                            ></DropdownMenuRoot
                                                        >
                                                    </div>
                                                    <p
                                                        v-if="rowErrors[contact.id]"
                                                        class="row-error"
                                                        role="alert"
                                                    >
                                                        {{ rowErrors[contact.id] }}
                                                    </p>
                                                    <div
                                                        v-if="
                                                            region?.id === contact.id &&
                                                            region.kind === 'block'
                                                        "
                                                        class="contact-region confirmation"
                                                        @keydown.escape.stop.prevent="closeRegion"
                                                    >
                                                        <div>
                                                            <h3>Block @{{ contact.username }}?</h3>
                                                            <p>
                                                                This removes the pending request and
                                                                prevents new transfers and friend
                                                                requests.
                                                            </p>
                                                        </div>
                                                        <div>
                                                            <Button
                                                                :id="`contact-${contact.id}-cancel`"
                                                                variant="secondary"
                                                                :disabled="pending.has(contact.id)"
                                                                @click="closeRegion"
                                                                >Cancel</Button
                                                            ><Button
                                                                variant="danger"
                                                                :disabled="pending.has(contact.id)"
                                                                @click="mutate(contact, 'block')"
                                                                >Block account</Button
                                                            >
                                                        </div>
                                                    </div>
                                                </li>
                                            </ul>
                                        </section></template
                                    >
                                </TabsContent>
                                <Transition name="contacts-stage">
                                    <div
                                        v-if="view === 'blocked'"
                                        class="contacts-stage"
                                        aria-label="Blocked accounts"
                                    >
                                        <p class="blocked-copy">
                                            Blocking applies to new transfers and friend requests
                                            from these accounts under the existing rules.
                                        </p>
                                        <div
                                            v-if="!data.blocked.length"
                                            class="contacts-empty contacts-empty--small"
                                        >
                                            <h2>No blocked accounts</h2>
                                            <p>Block an account from its contact actions.</p>
                                        </div>
                                        <ul v-else class="contacts-list">
                                            <li
                                                v-for="contact in data.blocked"
                                                :key="contact.id"
                                                class="contact"
                                            >
                                                <div class="contact-row">
                                                    <div class="contact-identity">
                                                        <span class="avatar">{{
                                                            initials(contact)
                                                        }}</span
                                                        ><span
                                                            ><strong>{{ display(contact) }}</strong
                                                            ><small
                                                                >@{{ contact.username }}</small
                                                            ></span
                                                        >
                                                    </div>
                                                    <Button
                                                        variant="secondary"
                                                        :id="`contact-${contact.id}-unblock`"
                                                        :disabled="pending.has(contact.id)"
                                                        @click="
                                                            openRegion(
                                                                contact as Contact,
                                                                'unblock',
                                                                `contact-${contact.id}-unblock`,
                                                            )
                                                        "
                                                        >Unblock</Button
                                                    >
                                                </div>
                                                <p
                                                    v-if="rowErrors[contact.id]"
                                                    class="row-error"
                                                    role="alert"
                                                >
                                                    {{ rowErrors[contact.id] }}
                                                </p>
                                                <div
                                                    v-if="
                                                        region?.id === contact.id &&
                                                        region.kind === 'unblock'
                                                    "
                                                    class="contact-region confirmation"
                                                    @keydown.escape.stop.prevent="closeRegion"
                                                >
                                                    <div>
                                                        <h3>Unblock @{{ contact.username }}?</h3>
                                                        <p>
                                                            This removes the block only. It does not
                                                            recreate a friendship or restore old
                                                            overrides; your account policy may
                                                            permit receiving afterward.
                                                        </p>
                                                    </div>
                                                    <div>
                                                        <Button
                                                            :id="`contact-${contact.id}-cancel`"
                                                            variant="secondary"
                                                            :disabled="pending.has(contact.id)"
                                                            @click="closeRegion"
                                                            >Cancel</Button
                                                        ><Button
                                                            :disabled="pending.has(contact.id)"
                                                            @click="
                                                                mutate(
                                                                    contact as Contact,
                                                                    'unblock',
                                                                )
                                                            "
                                                            >Unblock account</Button
                                                        >
                                                    </div>
                                                </div>
                                            </li>
                                        </ul>
                                    </div>
                                </Transition>
                            </div>
                        </AnimatedHeight>
                        <CollapsibleRoot v-model:open="defaultsOpen" class="receiving">
                            <CollapsibleTrigger class="receiving-toggle">
                                <span><Icon name="shield" :size="17" />Receiving preferences</span
                                ><small>{{ summary(data.settings) }}</small
                                ><Icon
                                    :name="defaultsOpen ? 'chevron-up' : 'chevron-down'"
                                    :size="17"
                                />
                            </CollapsibleTrigger>
                            <CollapsibleContent
                                class="receiving-body"
                                :inert="!defaultsOpen ? '' : undefined"
                                :aria-hidden="!defaultsOpen"
                            >
                                <div class="receiving-fields">
                                    <ReceivingDefaultsComponent
                                        :controlled="true"
                                        :model-value="data.settings"
                                        :pending="defaultsPending"
                                        :error="defaultsError"
                                        :show-contacts-link="false"
                                        @update:model-value="
                                            (next: ReceivingDefaults) => saveDefaults(next)
                                        "
                                    />
                                </div>
                            </CollapsibleContent>
                        </CollapsibleRoot>
                    </TabsRoot>
                </template>
            </section>
            <div v-if="data" class="contacts-meta">
                <button id="blocked-accounts-link" type="button" @click="view = 'blocked'">
                    <Icon name="shield" :size="14" />Blocked accounts<span
                        v-if="data.blocked.length"
                    >
                        · {{ data.blocked.length }}</span
                    >
                </button>
            </div>
            <p class="sr-only" role="status" aria-live="polite">{{ announcement }}</p>
        </AccountPage>
    </div>
</template>

<style scoped>
:deep(.account-page) {
    width: min(100% - 2rem, 65rem);
}
:deep(.account-page h1) {
    font-size: 2rem;
}
.contacts-panel {
    overflow: hidden;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
    box-shadow: var(--fb-shadow-popover);
}
.contacts-toolbar {
    display: flex;
    min-height: 5.25rem;
    align-items: center;
    gap: 1rem;
    padding: 1.15rem 1.5rem;
    border-bottom: 1px solid var(--fb-line-faint);
    background-color: var(--fb-surface);
    background-image: var(--fb-ambient-glow);
}
.contacts-toolbar > .fb-button:last-child {
    margin-left: auto;
}
.back {
    margin-right: 0.25rem;
}
.contacts-tabs {
    min-width: 0;
}
.contacts-tabs__list {
    position: relative;
    display: flex;
    gap: 0.2rem;
    padding: 0.25rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.75rem;
    background: var(--fb-surface-sunken);
}
.contacts-tabs__indicator {
    position: absolute;
    top: 0.25rem;
    left: var(--reka-tabs-indicator-position);
    z-index: 0;
    width: var(--reka-tabs-indicator-size);
    height: calc(100% - 0.5rem);
    border: 1px solid var(--fb-card-border);
    border-radius: 0.5rem;
    background: var(--fb-selected-surface);
    transition:
        left var(--fb-duration-switch) var(--fb-ease),
        width var(--fb-duration-switch) var(--fb-ease);
}
.contacts-tabs__trigger {
    z-index: 1;
    display: inline-flex;
    min-height: 2.25rem;
    align-items: center;
    gap: 0.45rem;
    padding: 0.45rem 0.75rem;
    border-radius: 0.5rem;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
}
.contacts-tabs__trigger[data-state='active'] {
    color: var(--fb-text);
}
.quiet-count,
.request-count {
    font-size: 0.6875rem;
    font-variant-numeric: tabular-nums;
}
.request-count {
    padding: 0.1rem 0.35rem;
    border-radius: 0.3rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.contacts-search {
    display: flex;
    min-width: 10rem;
    max-width: 13rem;
    align-items: center;
    gap: 0.35rem;
    padding: 0 0.55rem;
    border: 1px solid var(--fb-control-border);
    border-radius: 0.55rem;
    background: var(--fb-surface-sunken);
    color: var(--fb-text-muted);
}
.contacts-search :deep(.fb-input) {
    min-width: 0;
    border: 0;
    background: transparent;
    padding-inline: 0;
}
.contacts-search button {
    color: inherit;
}
.contacts-loading,
.contacts-error {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.6rem;
    min-height: 16rem;
    padding: 2rem;
    color: var(--fb-text-muted);
}
.contacts-error {
    min-height: 12rem;
    color: var(--fb-danger);
}
.add-form {
    width: min(100%, 28rem);
}
.add-form--inline {
    width: auto;
    padding: 1.15rem 1.5rem;
    border-bottom: 1px solid var(--fb-line-faint);
    background: var(--fb-surface-sunken);
}
.add-form__row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 0.5rem;
}
.add-form__row :deep(.fb-input) {
    min-width: 0;
    flex: 1;
    padding-left: 1.75rem;
}
.add-form__row :deep(.fb-button) {
    flex: none;
    white-space: nowrap;
}
.at {
    position: absolute;
    z-index: 1;
    left: 0.75rem;
    color: var(--fb-text-muted);
}
.inline-link {
    margin-top: 0.5rem;
    color: var(--fb-accent-text);
    text-decoration: underline;
    text-underline-offset: 0.2rem;
}
.add-success {
    margin: 0.5rem 0 0;
    color: var(--fb-success);
    font-size: 0.75rem;
}
.contacts-stage-stack {
    position: relative;
}
.contacts-stage[data-state='active'],
.contacts-stage-enter-active {
    animation: contacts-stage-in var(--fb-duration-pane) var(--fb-ease);
}
.contacts-stage[data-state='inactive'] {
    position: absolute;
    inset: 0;
    width: 100%;
    pointer-events: none;
    animation: contacts-stage-out var(--fb-duration-pane) var(--fb-ease);
}
.contacts-stage-leave-active {
    position: absolute;
    inset: 0;
    width: 100%;
    pointer-events: none;
    animation: contacts-stage-out var(--fb-duration-pane) var(--fb-ease);
}
@keyframes contacts-stage-in {
    from {
        opacity: 0;
        transform: translateX(8px);
    }
    to {
        opacity: 1;
        transform: translateX(0);
    }
}
@keyframes contacts-stage-out {
    from {
        opacity: 1;
        transform: translateX(0);
    }
    to {
        opacity: 0;
        transform: translateX(-8px);
    }
}
.contacts-list {
    margin: 0;
    padding: 0 1.5rem;
    list-style: none;
}
.contact {
    border-bottom: 1px solid var(--fb-line-faint);
}
.contact:last-child {
    border-bottom: 0;
}
.contact-row {
    display: flex;
    min-height: 5.3rem;
    align-items: center;
    gap: 0.55rem;
}
.contact-identity {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: 0.8rem;
    text-align: left;
}
button.contact-identity {
    cursor: pointer;
}
.contact-identity > span:not(.avatar) {
    display: grid;
    min-width: 0;
    gap: 0.15rem;
}
.contact-identity strong,
.contact-identity small {
    overflow-wrap: anywhere;
}
.contact-identity strong {
    font-size: 0.875rem;
}
.contact-identity small {
    color: var(--fb-text-muted);
    font-size: 0.75rem;
}
.contact-identity em {
    margin-left: 0.2rem;
    padding: 0.15rem 0.35rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.3rem;
    color: var(--fb-text-muted);
    font-size: 0.65rem;
    font-style: normal;
    white-space: nowrap;
}
.contact-identity em :deep(svg) {
    display: none;
}
.avatar {
    display: grid;
    width: 2.6rem;
    height: 2.6rem;
    flex: none;
    place-items: center;
    border: 1px solid var(--fb-card-border);
    border-radius: 0.8rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
    font-size: 0.75rem;
    font-weight: 650;
}
.send-link {
    min-height: 2.25rem;
    padding: 0.45rem 0.7rem;
    font-size: 0.75rem;
}
.contacts-menu {
    min-width: 12rem;
}
.contacts-menu__separator {
    height: 1px;
    margin: 0.25rem;
    background: var(--fb-border);
}
.contacts-menu__danger {
    color: var(--fb-danger);
}
.contact-region {
    margin: 0 -1.5rem;
    padding: 1.25rem 1.5rem;
    border-top: 1px solid var(--fb-line-faint);
    background-color: var(--fb-surface-sunken);
    background-image: var(--fb-ambient-glow);
    animation: contacts-region-in var(--fb-duration-switch) var(--fb-ease);
}
@keyframes contacts-region-in {
    from {
        opacity: 0;
        transform: translateY(-4px);
    }
    to {
        opacity: 1;
        transform: translateY(0);
    }
}
.region-heading,
.confirmation {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
}
.region-heading h3,
.confirmation h3 {
    margin: 0 0 0.3rem;
    font-size: 0.875rem;
}
.region-heading p,
.confirmation p,
.contact-region > p {
    margin: 0;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    line-height: 1.55;
}
.contact-region [role='alert'] {
    margin-top: 0.65rem;
    color: var(--fb-danger);
}
.preference-fields {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 1rem;
    margin-top: 1rem;
}
.confirmation > div:last-child {
    display: flex;
    align-items: center;
    gap: 0.5rem;
}
.request-group {
    padding: 0.5rem 0;
}
.request-group + .request-group {
    border-top: 1px solid var(--fb-line-faint);
}
.request-group h2 {
    margin: 0.75rem 1.5rem 0.1rem;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    font-weight: 600;
}
.request-group h2 span {
    font-weight: 400;
}
.blocked-copy {
    margin: 0;
    padding: 1.25rem 1.5rem 0.25rem;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
}
.row-error {
    margin: -0.35rem 0 0.75rem;
    color: var(--fb-danger);
    font-size: 0.75rem;
}
.contacts-row-enter-active,
.contacts-row-leave-active,
.contacts-row-move {
    transition:
        opacity var(--fb-duration-switch) var(--fb-ease),
        transform var(--fb-duration-switch) var(--fb-ease);
}
.contacts-row-enter-from,
.contacts-row-leave-to {
    opacity: 0;
    transform: translateY(4px);
}
@media (prefers-reduced-motion: reduce) {
    .contacts-row-enter-active,
    .contacts-row-leave-active,
    .contacts-row-move {
        transition: none;
    }
}
.contacts-empty {
    display: grid;
    min-height: 22rem;
    place-items: center;
    align-content: center;
    gap: 0.6rem;
    padding: 2rem;
    text-align: center;
    background-color: var(--fb-surface);
    background-image: var(--fb-ambient-glow);
}
.contacts-empty :deep(.fb-brand) {
    width: 3.5rem;
    height: 4.5rem;
    margin-bottom: 0.5rem;
}
.contacts-empty h2 {
    margin: 0;
    font-size: 1.35rem;
}
.contacts-empty > p {
    margin: 0 0 0.5rem;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
}
.contacts-empty--small {
    min-height: 16rem;
}
.receiving {
    border-top: 1px solid var(--fb-line-faint);
    background: var(--fb-surface-sunken);
}
.receiving-toggle {
    display: flex;
    width: 100%;
    min-height: 4.4rem;
    align-items: center;
    gap: 0.75rem;
    padding: 1rem 1.5rem;
    text-align: left;
}
.receiving-toggle span {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8125rem;
    font-weight: 600;
}
.receiving-toggle small {
    margin-left: auto;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
}
.receiving-body {
    overflow: hidden;
}
.receiving-fields {
    padding: 0 1.5rem 1.5rem;
}
.receiving-body[data-state='open'] {
    animation: contacts-disclosure-in var(--fb-duration-switch) var(--fb-ease);
}
.receiving-body[data-state='closed'] {
    animation: contacts-disclosure-out var(--fb-duration-switch) var(--fb-ease);
    pointer-events: none;
}
@keyframes contacts-disclosure-in {
    from {
        height: 0;
        opacity: 0;
    }
    to {
        height: var(--reka-collapsible-content-height);
        opacity: 1;
    }
}
@keyframes contacts-disclosure-out {
    from {
        height: var(--reka-collapsible-content-height);
        opacity: 1;
    }
    to {
        height: 0;
        opacity: 0;
    }
}
.contacts-meta {
    display: flex;
    justify-content: flex-end;
    margin-top: 0.65rem;
}
.contacts-meta button {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
}
.sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
}
@media (max-width: 640px) {
    .contacts-toolbar {
        flex-wrap: wrap;
        min-height: auto;
        padding: 0.9rem 1rem;
    }
    .contacts-search {
        order: 3;
        width: 100%;
        max-width: none;
        flex: 1 0 100%;
    }
    .contacts-list {
        padding-inline: 1rem;
    }
    .contact-row {
        gap: 0.3rem;
    }
    .avatar {
        width: 2.25rem;
        height: 2.25rem;
        border-radius: 0.65rem;
    }
    .contact-identity {
        gap: 0.55rem;
    }
    .contact-identity em :deep(svg) {
        display: block;
    }
    .contact-identity em .status-label {
        display: none;
    }
    .send-link {
        padding-inline: 0.5rem;
    }
    .send-link :deep(svg) {
        display: none;
    }
    .contact-region {
        margin-inline: -1rem;
        padding-inline: 1rem;
    }
    .preference-fields {
        grid-template-columns: 1fr;
    }
    .region-heading,
    .confirmation {
        flex-direction: column;
    }
    .confirmation > div:last-child {
        align-self: flex-end;
    }
    .receiving-toggle {
        align-items: flex-start;
        flex-wrap: wrap;
        padding-inline: 1rem;
    }
    .receiving-toggle small {
        width: calc(100% - 2rem);
        margin-left: 0;
    }
    .receiving-fields {
        padding-inline: 1rem;
    }
    .add-form--inline {
        padding-inline: 1rem;
    }
    .add-form__row {
        flex-wrap: wrap;
    }
    .add-form__row :deep(.fb-input) {
        flex: 1;
        min-width: 10rem;
    }
    .contacts-empty {
        min-height: 20rem;
        padding-inline: 1rem;
    }
}
@media (prefers-reduced-motion: reduce) {
    .contacts-tabs__indicator,
    .contacts-stage[data-state='active'],
    .contacts-stage[data-state='inactive'],
    .contacts-stage-enter-active,
    .contacts-stage-leave-active {
        animation: none;
        transition: none;
    }
    .contact-region {
        animation: none;
    }
    .receiving-body[data-state='open'],
    .receiving-body[data-state='closed'] {
        animation: none;
    }
}
</style>
