<script setup lang="ts">
import { Head, useForm, usePage } from '@inertiajs/vue3';
import { nextTick, watch } from 'vue';
import { AuthScreen as AuthScreenView, RouteSurface, type FilebeamConfig } from '@filebeam/ui';

defineOptions({ layout: RouteSurface });

type Mode = 'login' | 'register' | 'forgot' | 'reset' | 'verify';
type AuthFields = 'username' | 'name' | 'email' | 'password' | 'password_confirmation' | 'remember';
type HomeUser = { name: string; username?: string | null };
type SharedPageProps = {
    auth: { user: HomeUser | null };
    filebeam: FilebeamConfig;
    flash: { status?: string };
};

const props = withDefaults(
    defineProps<{
        mode: Mode;
        token?: string;
        email?: string;
        githubUrl: string;
        copyrightHolder: string;
        registrationEnabled?: boolean;
    }>(),
    { token: '', email: '', registrationEnabled: false },
);

const page = usePage<SharedPageProps>();
const form = useForm({
    username: '',
    name: '',
    email: props.email,
    password: '',
    password_confirmation: '',
    remember: false,
    token: props.token,
});

watch(
    () => props.mode,
    () => {
        form.clearErrors();
        form.reset('password', 'password_confirmation');
        form.token = props.token;
    },
);

function update(field: AuthFields, value: string | boolean): void {
    form.clearErrors(field);
    if (field === 'remember') {
        form.remember = Boolean(value);
    } else if (typeof value === 'string') {
        form[field] = value;
    }
}

function submit(): void {
    const path = {
        login: '/login',
        register: '/register',
        forgot: '/forgot-password',
        reset: '/reset-password',
        verify: '/email/verification-notification',
    }[props.mode];

    form.post(path, {
        onError: () => {
            void nextTick(() => {
                document.querySelector<HTMLElement>('.auth-drawer [aria-invalid="true"]')?.focus();
            });
        },
        onFinish: () => form.reset('password', 'password_confirmation'),
    });
}
</script>

<template>
    <div>
        <Head
            :title="
                mode === 'login'
                    ? 'Sign in'
                    : mode === 'register'
                      ? 'Create account'
                      : mode === 'forgot'
                        ? 'Reset password'
                        : mode === 'reset'
                          ? 'Choose a new password'
                          : 'Verify email'
            "
        />
        <AuthScreenView
            :mode="mode"
            :values="form.data()"
            :errors="form.errors"
            :processing="form.processing"
            :status="page.props.flash?.status"
            :registration-enabled="registrationEnabled"
            :config="page.props.filebeam"
            :user="page.props.auth.user"
            @submit="submit"
            @update="update"
        />
    </div>
</template>
