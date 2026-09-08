<script setup lang="ts">
import { Head, useForm } from '@inertiajs/vue3';
import { RouteSurface } from '@filebeam/ui';

defineOptions({ layout: RouteSurface });

const props = defineProps<{ token: string; email: string }>();

const form = useForm({
    username: '',
    name: '',
    email: props.email,
    password: '',
    password_confirmation: '',
});

function submit(): void {
    form.post(`/invitations/${props.token}`, {
        onFinish: () => form.reset('password', 'password_confirmation'),
    });
}
</script>

<template>
    <main class="mx-auto flex min-h-screen max-w-xl items-center px-5 py-12">
        <Head title="Accept invitation" />
        <section class="w-full rounded-2xl border border-white/10 bg-slate-950/90 p-7 shadow-2xl">
            <p class="text-sm font-medium text-cyan-300">Filebeam invitation</p>
            <h1 class="mt-2 text-2xl font-semibold text-white">Create your account</h1>
            <p class="mt-2 text-sm text-slate-300">
                This invitation is bound to
                <strong class="text-white">{{ email }}</strong
                >.
            </p>

            <form class="mt-7 space-y-4" @submit.prevent="submit">
                <label class="block text-sm text-slate-200">
                    Username
                    <input
                        v-model="form.username"
                        autocomplete="username"
                        class="mt-1 block w-full rounded-lg border border-slate-600 bg-slate-900 px-3 py-2 text-white"
                        :aria-invalid="Boolean(form.errors.username)"
                    />
                    <span v-if="form.errors.username" class="mt-1 block text-xs text-red-300">{{
                        form.errors.username
                    }}</span>
                </label>
                <label class="block text-sm text-slate-200">
                    Name (optional)
                    <input
                        v-model="form.name"
                        autocomplete="name"
                        class="mt-1 block w-full rounded-lg border border-slate-600 bg-slate-900 px-3 py-2 text-white"
                        :aria-invalid="Boolean(form.errors.name)"
                    />
                    <span v-if="form.errors.name" class="mt-1 block text-xs text-red-300">{{
                        form.errors.name
                    }}</span>
                </label>
                <label class="block text-sm text-slate-200">
                    Email
                    <input
                        v-model="form.email"
                        readonly
                        autocomplete="email"
                        class="mt-1 block w-full rounded-lg border border-slate-700 bg-slate-800 px-3 py-2 text-slate-300"
                        :aria-invalid="Boolean(form.errors.email)"
                    />
                    <span v-if="form.errors.email" class="mt-1 block text-xs text-red-300">{{
                        form.errors.email
                    }}</span>
                </label>
                <label class="block text-sm text-slate-200">
                    Password
                    <input
                        v-model="form.password"
                        type="password"
                        autocomplete="new-password"
                        class="mt-1 block w-full rounded-lg border border-slate-600 bg-slate-900 px-3 py-2 text-white"
                        :aria-invalid="Boolean(form.errors.password)"
                    />
                    <span v-if="form.errors.password" class="mt-1 block text-xs text-red-300">{{
                        form.errors.password
                    }}</span>
                </label>
                <label class="block text-sm text-slate-200">
                    Confirm password
                    <input
                        v-model="form.password_confirmation"
                        type="password"
                        autocomplete="new-password"
                        class="mt-1 block w-full rounded-lg border border-slate-600 bg-slate-900 px-3 py-2 text-white"
                    />
                </label>
                <button
                    type="submit"
                    class="w-full rounded-lg bg-cyan-400 px-4 py-2 font-semibold text-slate-950 disabled:opacity-50"
                    :disabled="form.processing"
                >
                    Create account
                </button>
            </form>
        </section>
    </main>
</template>
