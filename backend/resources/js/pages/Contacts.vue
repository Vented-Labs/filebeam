<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import { onMounted, ref } from 'vue';
import { RouteSurface } from '@filebeam/ui';
import {
    contactRequest,
    overrideValue,
    type Contacts,
    type Contact,
} from '../../../../ui/src/lib/contacts';
import ReceivingDefaults from '../../../../ui/src/components/auth/ReceivingDefaults.vue';
import AccountPage from '../../../../ui/src/components/layout/AccountPage.vue';
import AccountSection from '../../../../ui/src/components/layout/AccountSection.vue';
import AnimatedReveal from '../../../../ui/src/components/layout/AnimatedReveal.vue';
import AppLink from '../../../../ui/src/components/primitives/AppLink.vue';
import Button from '../../../../ui/src/components/primitives/Button.vue';
import FormField from '../../../../ui/src/components/primitives/FormField.vue';
import Icon from '../../../../ui/src/components/primitives/Icon.vue';
import Input from '../../../../ui/src/components/primitives/Input.vue';
import Select from '../../../../ui/src/components/primitives/Select.vue';

defineOptions({ layout: RouteSurface });
const data = ref<Contacts>();
const username = ref('');
const busy = ref(false);
const error = ref('');
const groups = [
    {
        status: 'incoming',
        title: 'Incoming requests',
        description: 'Choose who you want to add to your friends.',
        icon: 'users',
    },
    {
        status: 'accepted',
        title: 'Friends',
        description: 'Your permissions apply to files coming from each friend.',
        icon: 'users',
    },
    {
        status: 'outgoing',
        title: 'Sent requests',
        description: 'Waiting for these accounts to accept your request.',
        icon: 'clock',
    },
] as const;
async function load(): Promise<void> {
    try {
        data.value = await contactRequest<Contacts>('contacts');
    } catch (reason) {
        error.value = reason instanceof Error ? reason.message : 'Could not load contacts.';
    }
}
async function action(target: string, operation: string, contact?: Contact): Promise<void> {
    if (busy.value) return;
    const normalized = target.trim().replace(/^@/, '').toLowerCase();
    if (!/^[a-z0-9_]{3,24}$/.test(normalized)) {
        error.value = 'Enter an exact username, such as @alice.';
        return;
    }
    busy.value = true;
    error.value = '';
    try {
        data.value = await contactRequest<Contacts>(`contacts/${normalized}`, 'POST', {
            action: operation,
            canSend: contact?.canSend ?? null,
            autoDownload: contact?.autoDownload ?? null,
        });
        if (operation === 'request') username.value = '';
    } catch (reason) {
        error.value = reason instanceof Error ? reason.message : 'Could not update contact.';
        await load();
    } finally {
        busy.value = false;
    }
}
function preference(contact: Contact, field: 'canSend' | 'autoDownload', value: string): void {
    contact[field] = overrideValue(value);
    void action(contact.username, 'preferences', contact);
}
function override(value: boolean | null): string {
    return value === null ? 'inherit' : value ? 'allow' : 'deny';
}
onMounted(load);
</script>
<template>
    <div>
        <Head title="Contacts" />
        <AccountPage
            title="Contacts"
            description="Friends on this instance, with receiving preferences you control."
        >
            <template #actions
                ><AppLink href="/account/inbox" class="fb-button fb-button--secondary"
                    ><Icon name="folder" :size="16" />Open inbox</AppLink
                ></template
            >
            <AnimatedReveal :show="Boolean(error)"
                ><p role="alert" class="contacts-feedback">
                    <Icon name="alert" :size="17" />{{ error }}
                </p></AnimatedReveal
            >
            <div class="contacts-grid">
                <aside class="contacts-sidebar">
                    <AccountSection
                        title="Add a friend"
                        description="Use their exact username on this instance."
                        icon="plus"
                    >
                        <form
                            class="contacts-request"
                            @submit.prevent="action(username, 'request')"
                        >
                            <FormField
                                id="contact-username"
                                label="Add a friend by exact username"
                                description="They will receive a request to accept."
                            >
                                <template #default="field"
                                    ><Input
                                        v-bind="field"
                                        v-model="username"
                                        placeholder="@username"
                                        autocomplete="off"
                                        :disabled="busy"
                                /></template>
                            </FormField>
                            <Button type="submit" :disabled="busy || !username.trim()"
                                ><Icon :name="busy ? 'loader' : 'plus'" :size="16" />Send friend
                                request</Button
                            >
                        </form>
                    </AccountSection>
                    <AccountSection
                        title="Receiving defaults"
                        description="Used whenever a contact's setting is Inherit."
                        icon="shield"
                        ><ReceivingDefaults
                    /></AccountSection>
                </aside>
                <div class="contacts-directory">
                    <div v-if="!data" class="contacts-loading" role="status">
                        <Icon name="loader" :size="22" />Loading contacts…
                    </div>
                    <template v-else>
                        <AccountSection
                            v-for="group in groups"
                            :key="group.status"
                            :title="group.title"
                            :description="group.description"
                            :icon="group.icon"
                        >
                            <template #actions
                                ><span class="contacts-count">{{
                                    data.contacts.filter(
                                        (contact) => contact.status === group.status,
                                    ).length
                                }}</span></template
                            >
                            <div
                                v-if="
                                    !data.contacts.some(
                                        (contact) => contact.status === group.status,
                                    )
                                "
                                class="contacts-empty"
                            >
                                <Icon :name="group.icon" :size="24" />
                                <div>
                                    <strong>{{
                                        group.status === 'accepted'
                                            ? 'No friends yet'
                                            : group.status === 'incoming'
                                              ? 'No incoming requests'
                                              : 'No sent requests'
                                    }}</strong>
                                    <p>
                                        {{
                                            group.status === 'accepted'
                                                ? 'Add a username to get started.'
                                                : 'Requests will appear here.'
                                        }}
                                    </p>
                                </div>
                            </div>
                            <div v-else class="contacts-list">
                                <article
                                    v-for="contact in data.contacts.filter(
                                        (contact) => contact.status === group.status,
                                    )"
                                    :key="contact.id"
                                    class="contact-card"
                                >
                                    <header class="contact-card__header">
                                        <span class="contact-avatar"
                                            ><Icon name="user" :size="20"
                                        /></span>
                                        <div>
                                            <h3>{{ contact.name || contact.username }}</h3>
                                            <p>@{{ contact.username }}</p>
                                        </div>
                                        <span class="contact-status"
                                            ><Icon
                                                :name="
                                                    group.status === 'accepted' ? 'check' : 'clock'
                                                "
                                                :size="13"
                                            />{{
                                                group.status === 'accepted' ? 'Friend' : 'Pending'
                                            }}</span
                                        >
                                    </header>
                                    <div
                                        v-if="group.status === 'accepted'"
                                        class="contact-preferences"
                                    >
                                        <FormField
                                            :id="`contact-send-${contact.id}`"
                                            label="Can send me files"
                                            :description="
                                                contact.effective.canSend
                                                    ? 'Incoming files are allowed.'
                                                    : 'Incoming files are denied.'
                                            "
                                            ><template #default="field"
                                                ><Select
                                                    v-bind="field"
                                                    :model-value="override(contact.canSend)"
                                                    :disabled="busy"
                                                    icon="shield"
                                                    :options="[
                                                        {
                                                            value: 'inherit',
                                                            label: 'Inherit account default',
                                                        },
                                                        { value: 'allow', label: 'Allow' },
                                                        { value: 'deny', label: 'Deny' },
                                                    ]"
                                                    @update:model-value="
                                                        preference(contact, 'canSend', $event)
                                                    " /></template
                                        ></FormField>
                                        <FormField
                                            :id="`contact-auto-${contact.id}`"
                                            label="Automatic download"
                                            :description="
                                                contact.effective.autoDownload
                                                    ? 'Eligible clients will stage files privately.'
                                                    : 'Files wait for a manual download.'
                                            "
                                            ><template #default="field"
                                                ><Select
                                                    v-bind="field"
                                                    :model-value="override(contact.autoDownload)"
                                                    :disabled="busy"
                                                    icon="download"
                                                    :options="[
                                                        {
                                                            value: 'inherit',
                                                            label: 'Inherit account default',
                                                        },
                                                        { value: 'allow', label: 'On' },
                                                        { value: 'deny', label: 'Off' },
                                                    ]"
                                                    @update:model-value="
                                                        preference(contact, 'autoDownload', $event)
                                                    " /></template
                                        ></FormField>
                                    </div>
                                    <div class="contact-actions">
                                        <template v-if="group.status === 'incoming'"
                                            ><Button
                                                :disabled="busy"
                                                @click="action(contact.username, 'accept')"
                                                ><Icon name="check" :size="15" />Accept</Button
                                            ><Button
                                                variant="secondary"
                                                :disabled="busy"
                                                @click="action(contact.username, 'decline')"
                                                >Decline</Button
                                            ></template
                                        >
                                        <Button
                                            v-else-if="group.status === 'outgoing'"
                                            variant="secondary"
                                            :disabled="busy"
                                            @click="action(contact.username, 'cancel')"
                                            >Cancel request</Button
                                        >
                                        <template v-else
                                            ><AppLink
                                                :href="`/u/${contact.username}`"
                                                class="fb-button fb-button--secondary"
                                                ><Icon name="upload" :size="15" />Send
                                                files</AppLink
                                            ><Button
                                                variant="ghost"
                                                :disabled="busy"
                                                @click="action(contact.username, 'remove')"
                                                >Remove friend</Button
                                            ></template
                                        >
                                        <Button
                                            variant="ghost"
                                            class="contact-actions__block"
                                            :disabled="busy"
                                            @click="action(contact.username, 'block')"
                                            ><Icon name="shield" :size="15" />Block</Button
                                        >
                                    </div>
                                </article>
                            </div>
                        </AccountSection>
                        <AccountSection
                            title="Blocked accounts"
                            description="New deliveries and friend requests from these accounts are blocked."
                            icon="shield"
                        >
                            <template #actions
                                ><span class="contacts-count">{{
                                    data.blocked.length
                                }}</span></template
                            >
                            <div v-if="!data.blocked.length" class="contacts-empty">
                                <Icon name="shield" :size="24" />
                                <div>
                                    <strong>No blocked accounts</strong>
                                    <p>You can block an account from its contact card.</p>
                                </div>
                            </div>
                            <div v-else class="contacts-list">
                                <article
                                    v-for="contact in data.blocked"
                                    :key="contact.id"
                                    class="contact-card contact-card--blocked"
                                >
                                    <span class="contact-avatar"
                                        ><Icon name="user" :size="20"
                                    /></span>
                                    <div class="contact-card__identity">
                                        <h3>{{ contact.name || contact.username }}</h3>
                                        <p>@{{ contact.username }}</p>
                                    </div>
                                    <Button
                                        variant="secondary"
                                        :disabled="busy"
                                        @click="action(contact.username, 'unblock')"
                                        >Unblock</Button
                                    >
                                </article>
                            </div>
                        </AccountSection>
                    </template>
                </div>
            </div>
        </AccountPage>
    </div>
</template>
<style scoped>
.contacts-grid {
    display: grid;
    grid-template-columns: minmax(18rem, 0.8fr) minmax(0, 1.6fr);
    align-items: start;
    gap: 1.25rem;
}
.contacts-sidebar,
.contacts-directory,
.contacts-list,
.contacts-request {
    display: grid;
    gap: 1.25rem;
    min-width: 0;
}
.contacts-request .fb-button {
    width: 100%;
}
.contacts-count {
    padding: 0.25rem 0.625rem;
    border: 1px solid var(--fb-card-border);
    border-radius: 0.625rem;
    background: var(--fb-selected-surface);
    color: var(--fb-accent-text);
    font-size: 0.75rem;
    font-weight: 650;
}
.contacts-empty,
.contacts-loading {
    display: flex;
    align-items: center;
    gap: 0.875rem;
    padding: 1rem 0;
    color: var(--fb-text-subtle);
}
.contacts-empty strong {
    color: var(--fb-text);
    font-size: 0.875rem;
    font-weight: 600;
}
.contacts-empty p,
.contact-card p {
    margin: 0.25rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    line-height: 1.6;
}
.contact-card {
    min-width: 0;
    padding: 1.125rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.875rem;
    background: var(--fb-surface-sunken);
}
.contact-card__header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
}
.contact-card__header > div,
.contact-card__identity {
    min-width: 0;
    flex: 1;
}
h3 {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 650;
    overflow-wrap: anywhere;
}
.contact-avatar {
    display: grid;
    width: 2.5rem;
    height: 2.5rem;
    flex: none;
    place-items: center;
    border: 1px solid var(--fb-card-border);
    border-radius: 0.75rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.contact-status {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    color: var(--fb-text-subtle);
    font-size: 0.6875rem;
}
.contact-preferences {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.875rem;
    margin-block: 1.125rem;
}
.contact-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    padding-top: 0.875rem;
    margin-top: 0.875rem;
    border-top: 1px solid var(--fb-line-soft);
}
.contact-actions__block {
    margin-left: auto;
}
.contact-card--blocked {
    display: flex;
    align-items: center;
    gap: 0.75rem;
}
.contacts-feedback {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    margin: 0 0 1.25rem;
    padding: 0.875rem 1rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.75rem;
    background: var(--fb-surface);
    color: var(--fb-danger);
    font-size: 0.8125rem;
}
@media (max-width: 900px) {
    .contacts-grid {
        grid-template-columns: 1fr;
    }
    .contacts-sidebar {
        grid-template-columns: repeat(2, minmax(0, 1fr));
    }
}
@media (max-width: 640px) {
    .contacts-sidebar,
    .contact-preferences {
        grid-template-columns: 1fr;
    }
    .contact-status {
        display: none;
    }
    .contact-actions__block {
        margin-left: 0;
    }
}
</style>
