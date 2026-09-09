<script setup lang="ts">
import AppLink from '../primitives/AppLink.vue';
import Button from '../primitives/Button.vue';
import CopyButton from '../primitives/CopyButton.vue';
import Input from '../primitives/Input.vue';
import InboxSettings from './InboxSettings.vue';
import Icon from '../primitives/Icon.vue';

defineProps<{
    githubUrl: string;
    copyrightHolder: string;
    user: {
        id: number;
        name: string;
        username: string | null;
        email: string;
        emailVerifiedAt: string | null;
        profileUrl: string | null;
        inboxEnabled: boolean;
        usernameRoutingEnabled: boolean;
        notificationChannel: 'mail' | 'database';
    };
}>();

const emit = defineEmits<{ logout: [] }>();
</script>

<template>
    <section class="account-panel">
        <header class="account-panel__header">
            <div>
                <h1>Account</h1>
                <p>Manage your identity, receiving link, and browser-held encryption keys.</p>
            </div>
            <Button variant="secondary" @click="emit('logout')">
                <Icon name="logout" :size="16" />Sign out
            </Button>
        </header>
        <div class="account-panel__grid">
            <aside class="account-panel__identity">
                <div class="account-panel__avatar"><Icon name="user" :size="26" /></div>
                <h2>{{ user.name }}</h2>
                <p v-if="user.username">@{{ user.username }}</p>
                <dl>
                    <div>
                        <dt>Email</dt>
                        <dd>{{ user.email }}</dd>
                    </div>
                    <div>
                        <dt>Status</dt>
                        <dd
                            :class="
                                user.emailVerifiedAt
                                    ? 'text-[var(--fb-success)]'
                                    : 'text-[var(--fb-warning)]'
                            "
                        >
                            {{ user.emailVerifiedAt ? 'Verified' : 'Verification pending' }}
                        </dd>
                    </div>
                </dl>
                <AppLink
                    v-if="!user.emailVerifiedAt"
                    href="/verify-email"
                    class="fb-text-link text-sm"
                    >Verify email</AppLink
                >
                <div class="account-panel__profile-link">
                    <label for="account-profile-url">Profile link preview</label>
                    <div>
                        <Input
                            id="account-profile-url"
                            :value="user.profileUrl ?? 'Available after choosing a username'"
                            readonly
                        />
                        <CopyButton
                            :value="user.profileUrl ?? ''"
                            label="Copy profile link"
                            icon-only
                            variant="secondary"
                            :disabled="!user.profileUrl"
                        />
                    </div>
                </div>
            </aside>
            <InboxSettings :user="user" />
        </div>
    </section>
</template>

<style scoped>
.account-panel {
    width: min(100% - 2rem, 72rem);
    margin-inline: auto;
    padding-block: 3.5rem 4.5rem;
    color: var(--fb-text);
}
.account-panel__header {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 2rem;
    margin-bottom: 1.5rem;
}
.account-panel__header h1 {
    margin: 0;
    font-size: 2.25rem;
    letter-spacing: -0.04em;
}
.account-panel__header div > p:last-child {
    margin: 0.625rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
}
.account-panel__grid {
    display: grid;
    grid-template-columns: minmax(15rem, 0.72fr) minmax(0, 1.4fr);
    overflow: hidden;
    align-items: stretch;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
    box-shadow: var(--fb-shadow-panel);
}
.account-panel__identity {
    padding: 1.75rem;
    border-right: 1px solid #ffffff0a;
    background: #17131e;
}
.account-panel__avatar {
    display: grid;
    width: 3.5rem;
    height: 3.5rem;
    place-items: center;
    border: 1px solid #78598666;
    border-radius: 1rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.account-panel__identity h2 {
    margin: 1rem 0 0;
    font-size: 1.25rem;
}
.account-panel__identity > p {
    margin: 0.25rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
}
.account-panel__identity dl {
    display: grid;
    gap: 0.875rem;
    margin: 1.5rem 0;
    padding-block: 1.25rem;
    border-block: 1px solid #ffffff08;
}
.account-panel__identity dt,
.account-panel__profile-link label {
    color: var(--fb-text-subtle);
    font-size: 0.6875rem;
    letter-spacing: 0.07em;
    text-transform: uppercase;
}
.account-panel__identity dd {
    overflow-wrap: anywhere;
    margin: 0.25rem 0 0;
    font-size: 0.8125rem;
}
.account-panel__profile-link {
    margin-top: 1.5rem;
}
.account-panel__profile-link > div {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.5rem;
}
.account-panel__profile-link .fb-input {
    min-width: 0;
    flex: 1;
    font-size: 0.75rem;
}
.account-panel__grid :deep(.inbox-settings) {
    margin-top: 0;
    border: 0;
    border-radius: 0;
    background: var(--fb-settings-surface);
}
@media (max-width: 760px) {
    .account-panel {
        padding-block: 2rem 3rem;
    }
    .account-panel__header {
        align-items: start;
    }
    .account-panel__grid {
        grid-template-columns: 1fr;
    }
    .account-panel__identity {
        border-right: 0;
        border-bottom: 1px solid #ffffff0a;
    }
}
@media (max-width: 460px) {
    .account-panel__header {
        gap: 1rem;
    }
    .account-panel__header div > p:last-child {
        max-width: 15rem;
    }
    .account-panel__header .fb-button {
        padding-inline: 0.75rem;
    }
}
</style>
