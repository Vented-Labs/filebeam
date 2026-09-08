<script setup lang="ts">
import type { FilebeamConfig } from '../../types';
import AppLink from '../primitives/AppLink.vue';
import AuthDrawer from './AuthDrawer.vue';
import AuthForm from './AuthForm.vue';

type Mode = 'login' | 'register' | 'forgot' | 'reset' | 'verify';

withDefaults(
    defineProps<{
        mode: Mode;
        values: Record<string, string | boolean>;
        errors?: Record<string, string>;
        processing?: boolean;
        status?: string;
        registrationEnabled?: boolean;
        config: FilebeamConfig;
        user?: { name: string; username?: string | null } | null;
    }>(),
    { registrationEnabled: true },
);

const emit = defineEmits<{
    submit: [];
    update: [
        field: 'username' | 'name' | 'email' | 'password' | 'password_confirmation' | 'remember',
        value: string | boolean,
    ];
}>();

const headings: Record<Mode, string> = {
    login: 'Welcome back',
    register: 'Create your account',
    forgot: 'Reset your password',
    reset: 'Choose a new password',
    verify: 'Verify your email',
};

const descriptions: Record<Mode, string> = {
    login: 'Sign in to manage your account.',
    register: 'Your email verification keeps account recovery available.',
    forgot: 'We will email you a link to reset your password.',
    reset: 'Set a new password for your account.',
    verify: 'Confirm your email address to finish setting up your account.',
};

function update(
    field: 'username' | 'name' | 'email' | 'password' | 'password_confirmation' | 'remember',
    value: string | boolean,
): void {
    emit('update', field, value);
}
</script>

<template>
    <AuthDrawer :title="headings[mode]" :description="descriptions[mode]">
        <AuthForm
            class="mt-7"
            :mode="mode"
            :values="values"
            :errors="errors"
            :processing="processing"
            :status="status"
            :registration-enabled="registrationEnabled"
            @submit="emit('submit')"
            @update="update"
        />
        <nav
            class="mt-6 border-t border-[var(--fb-border)] pt-5 text-center text-sm text-[var(--fb-text-muted)]"
            aria-label="Authentication links"
        >
            <template v-if="mode === 'login' && registrationEnabled">
                New here?
                <AppLink href="/register" preserve-state preserve-scroll class="fb-text-link ml-1"
                    >Create account</AppLink
                >
            </template>
            <span v-if="mode === 'register'">Already have an account? </span>
            <AppLink
                v-if="mode !== 'login' && mode !== 'verify'"
                href="/login"
                preserve-state
                preserve-scroll
                class="fb-text-link ml-1"
                >Sign in</AppLink
            >
        </nav>
    </AuthDrawer>
</template>
