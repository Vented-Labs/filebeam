<script setup lang="ts">
import AppShell from '../layout/AppShell.vue';
import AppLink from '../primitives/AppLink.vue';
import Button from '../primitives/Button.vue';
import CopyButton from '../primitives/CopyButton.vue';
import Input from '../primitives/Input.vue';
import InboxSettings from './InboxSettings.vue';

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
    <AppShell :github-url="githubUrl" :copyright-holder="copyrightHolder" :user="user">
        <section class="mx-auto w-full max-w-3xl px-5 py-12 text-[var(--fb-text)] sm:px-8">
            <div class="flex flex-wrap items-center justify-between gap-4">
                <h1 class="text-3xl font-semibold">Account</h1>
                <Button variant="ghost" @click="emit('logout')">Sign out</Button>
            </div>
            <div
                class="mt-7 space-y-5 rounded-2xl border border-[var(--fb-border)] bg-[var(--fb-surface)] p-6"
            >
                <div>
                    <p class="text-sm text-[var(--fb-text-muted)]">Username</p>
                    <p class="mt-1 font-medium">{{ user.username ?? 'Not set' }}</p>
                </div>
                <div>
                    <p class="text-sm text-[var(--fb-text-muted)]">Email</p>
                    <p class="mt-1 font-medium">{{ user.email }}</p>
                    <p
                        class="mt-1 text-sm"
                        :class="
                            user.emailVerifiedAt
                                ? 'text-[var(--fb-success)]'
                                : 'text-[var(--fb-warning)]'
                        "
                    >
                        {{ user.emailVerifiedAt ? 'Verified' : 'Verification pending' }}
                    </p>
                    <AppLink
                        v-if="!user.emailVerifiedAt"
                        href="/verify-email"
                        class="fb-text-link mt-2 inline-block text-sm"
                        >Verify email</AppLink
                    >
                </div>
                <div>
                    <p class="text-sm text-[var(--fb-text-muted)]">Profile link preview</p>
                    <div class="mt-2 flex gap-2">
                        <Input
                            class="min-w-0 flex-1"
                            :value="user.profileUrl ?? 'Available after choosing a username'"
                            disabled
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
            </div>
            <InboxSettings :user="user" />
        </section>
    </AppShell>
</template>
