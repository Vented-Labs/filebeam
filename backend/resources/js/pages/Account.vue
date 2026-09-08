<script setup lang="ts">
import { Head, router } from '@inertiajs/vue3';
import { AccountPanel, RouteSurface } from '@filebeam/ui';

defineOptions({ layout: RouteSurface });

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
</script>

<template>
    <div>
        <Head title="Account" />
        <AccountPanel
            :github-url="githubUrl"
            :copyright-holder="copyrightHolder"
            :user="user"
            @logout="router.post('/logout')"
        />
    </div>
</template>
