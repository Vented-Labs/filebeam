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
import AppLink from '../../../../ui/src/components/primitives/AppLink.vue';
import Button from '../../../../ui/src/components/primitives/Button.vue';
import Input from '../../../../ui/src/components/primitives/Input.vue';

defineOptions({ layout: RouteSurface });
const data = ref<Contacts>();
const username = ref('');
const busy = ref(false);
const error = ref('');
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
function preference(contact: Contact, field: 'canSend' | 'autoDownload', event: Event): void {
    contact[field] = overrideValue((event.target as HTMLSelectElement).value);
    void action(contact.username, 'preferences', contact);
}
onMounted(load);
</script>

<template>
    <div class="contacts-page">
        <Head title="Contacts" />
        <header>
            <h1>Contacts</h1>
            <AppLink href="/account/inbox" class="fb-text-link">Open inbox</AppLink>
        </header>
        <p>
            Your friends are stored on this instance. Each of you controls your own incoming
            permissions.
        </p>
        <section class="contacts-card">
            <h2>Account receiving defaults</h2>
            <ReceivingDefaults />
        </section>
        <form class="contacts-request" @submit.prevent="action(username, 'request')">
            <label for="contact-username">Add a friend by exact username</label>
            <Input
                id="contact-username"
                v-model="username"
                placeholder="@username"
                autocomplete="off"
                :disabled="busy"
            />
            <Button type="submit" :disabled="busy || !username.trim()">Send friend request</Button>
        </form>
        <p v-if="error" role="alert" class="contacts-error">{{ error }}</p>
        <p v-if="!data" role="status">Loading contacts…</p>
        <template v-else>
            <section
                v-for="status in ['incoming', 'outgoing', 'accepted']"
                :key="status"
                class="contacts-group"
            >
                <h2>
                    {{
                        status === 'accepted'
                            ? 'Friends'
                            : status === 'incoming'
                              ? 'Incoming requests'
                              : 'Sent requests'
                    }}
                </h2>
                <p v-if="!data.contacts.some((contact) => contact.status === status)">None yet.</p>
                <article
                    v-for="contact in data.contacts.filter((contact) => contact.status === status)"
                    :key="contact.id"
                    class="contacts-card"
                >
                    <header>
                        <strong>@{{ contact.username }}</strong
                        ><span>{{ contact.name }}</span>
                    </header>
                    <div v-if="status === 'accepted'" class="contacts-preferences">
                        <label
                            >Can send me files
                            <select
                                class="fb-input"
                                :value="
                                    contact.canSend === null
                                        ? 'inherit'
                                        : contact.canSend
                                          ? 'allow'
                                          : 'deny'
                                "
                                :disabled="busy"
                                @change="preference(contact, 'canSend', $event)"
                            >
                                <option value="inherit">
                                    Inherit — {{ contact.effective.canSend ? 'allowed' : 'denied' }}
                                </option>
                                <option value="allow">Allow</option>
                                <option value="deny">Deny</option>
                            </select>
                        </label>
                        <label
                            >Automatic download
                            <select
                                class="fb-input"
                                :value="
                                    contact.autoDownload === null
                                        ? 'inherit'
                                        : contact.autoDownload
                                          ? 'allow'
                                          : 'deny'
                                "
                                :disabled="busy"
                                @change="preference(contact, 'autoDownload', $event)"
                            >
                                <option value="inherit">
                                    Inherit — {{ contact.effective.autoDownload ? 'on' : 'off' }}
                                </option>
                                <option value="allow">On</option>
                                <option value="deny">Off</option>
                            </select>
                        </label>
                    </div>
                    <div class="contacts-actions">
                        <template v-if="status === 'incoming'"
                            ><Button :disabled="busy" @click="action(contact.username, 'accept')"
                                >Accept</Button
                            ><Button
                                variant="secondary"
                                :disabled="busy"
                                @click="action(contact.username, 'decline')"
                                >Decline</Button
                            ></template
                        >
                        <Button
                            v-else-if="status === 'outgoing'"
                            variant="secondary"
                            :disabled="busy"
                            @click="action(contact.username, 'cancel')"
                            >Cancel request</Button
                        >
                        <template v-else
                            ><AppLink
                                :href="`/u/${contact.username}`"
                                class="fb-button fb-button--secondary"
                                >Send files</AppLink
                            ><Button
                                variant="ghost"
                                :disabled="busy"
                                @click="action(contact.username, 'remove')"
                                >Remove friend</Button
                            ></template
                        >
                        <Button
                            variant="ghost"
                            :disabled="busy"
                            @click="action(contact.username, 'block')"
                            >Block</Button
                        >
                    </div>
                </article>
            </section>
            <section class="contacts-group">
                <h2>Blocked accounts</h2>
                <p v-if="!data.blocked.length">None.</p>
                <article v-for="contact in data.blocked" :key="contact.id" class="contacts-card">
                    <header>
                        <strong>@{{ contact.username }}</strong
                        ><Button
                            variant="ghost"
                            :disabled="busy"
                            @click="action(contact.username, 'unblock')"
                            >Unblock</Button
                        >
                    </header>
                </article>
            </section>
        </template>
    </div>
</template>

<style scoped>
.contacts-page {
    width: min(100% - 2rem, 58rem);
    margin-inline: auto;
    padding-block: 3rem;
}
header,
.contacts-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
}
h1 {
    margin: 0;
}
h2 {
    font-size: 1.125rem;
}
p,
header > span {
    color: var(--fb-text-muted);
    line-height: 1.6;
}
.contacts-card {
    margin-block: 1rem;
    padding: 1.25rem;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
}
.contacts-request {
    display: grid;
    gap: 0.75rem;
    margin-top: 2rem;
}
.contacts-preferences {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 1rem;
    margin-block: 1rem;
}
label {
    display: grid;
    gap: 0.5rem;
    font-size: 0.875rem;
}
.contacts-actions {
    justify-content: start;
    margin-top: 1rem;
}
.contacts-group {
    margin-top: 2rem;
}
.contacts-error {
    color: var(--fb-danger);
}
@media (max-width: 560px) {
    .contacts-preferences {
        grid-template-columns: 1fr;
    }
}
</style>
