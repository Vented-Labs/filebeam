<script setup lang="ts">
import { Head, useForm } from '@inertiajs/vue3';
import { AnimatedReveal, Icon, Input, RouteSurface } from '@filebeam/ui';

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
    <main class="invitation-page">
        <Head title="Accept invitation" />
        <section class="invitation-card">
            <div class="invitation-card__emblem"><Icon name="user" :size="24" /></div>
            <p class="invitation-card__eyebrow">Filebeam invitation</p>
            <h1>Create your account</h1>
            <p class="invitation-card__intro">
                This invitation is bound to <strong>{{ email }}</strong
                >.
            </p>

            <form @submit.prevent="submit">
                <label>
                    <span>Username</span>
                    <Input
                        v-model="form.username"
                        autocomplete="username"
                        :invalid="Boolean(form.errors.username)"
                    />
                    <AnimatedReveal :show="Boolean(form.errors.username)">
                        <small>{{ form.errors.username }}</small>
                    </AnimatedReveal>
                </label>
                <label>
                    <span>Name (optional)</span>
                    <Input
                        v-model="form.name"
                        autocomplete="name"
                        :invalid="Boolean(form.errors.name)"
                    />
                    <AnimatedReveal :show="Boolean(form.errors.name)">
                        <small>{{ form.errors.name }}</small>
                    </AnimatedReveal>
                </label>
                <label>
                    <span>Email</span>
                    <Input
                        v-model="form.email"
                        readonly
                        autocomplete="email"
                        :invalid="Boolean(form.errors.email)"
                    />
                    <AnimatedReveal :show="Boolean(form.errors.email)">
                        <small>{{ form.errors.email }}</small>
                    </AnimatedReveal>
                </label>
                <label>
                    <span>Password</span>
                    <Input
                        v-model="form.password"
                        type="password"
                        autocomplete="new-password"
                        :invalid="Boolean(form.errors.password)"
                    />
                    <AnimatedReveal :show="Boolean(form.errors.password)">
                        <small>{{ form.errors.password }}</small>
                    </AnimatedReveal>
                </label>
                <label>
                    <span>Confirm password</span>
                    <Input
                        v-model="form.password_confirmation"
                        type="password"
                        autocomplete="new-password"
                    />
                </label>
                <button
                    type="submit"
                    class="fb-button fb-button--primary invitation-card__submit"
                    :disabled="form.processing"
                >
                    <Icon name="arrow-right" :size="16" />Create account
                </button>
            </form>
        </section>
    </main>
</template>

<style scoped>
.invitation-page {
    display: grid;
    width: min(100% - 2rem, 36rem);
    min-height: calc(100svh - 10rem);
    place-items: center;
    margin-inline: auto;
    padding-block: 3rem;
}
.invitation-card {
    width: 100%;
    box-sizing: border-box;
    padding: 2rem;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
    box-shadow: var(--fb-shadow-panel);
}
.invitation-card__emblem {
    display: grid;
    width: 3.25rem;
    height: 3.25rem;
    place-items: center;
    border: 1px solid #78598666;
    border-radius: 0.875rem;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.invitation-card__eyebrow {
    margin: 1.25rem 0 0;
    color: var(--fb-accent-text);
    font-size: 0.6875rem;
    font-weight: 650;
    letter-spacing: 0.14em;
    text-transform: uppercase;
}
.invitation-card h1 {
    margin: 0.5rem 0 0;
    color: var(--fb-text);
    font-size: 1.75rem;
    letter-spacing: -0.04em;
}
.invitation-card__intro {
    margin: 0.625rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.875rem;
}
.invitation-card__intro strong {
    color: var(--fb-text);
}
.invitation-card form {
    display: grid;
    gap: 1rem;
    margin-top: 1.75rem;
}
.invitation-card label {
    display: grid;
    gap: 0.5rem;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
    font-weight: 550;
}
.invitation-card small {
    color: var(--fb-danger);
    font-size: 0.75rem;
}
.invitation-card__submit {
    width: 100%;
    margin-top: 0.5rem;
}
@media (max-width: 520px) {
    .invitation-card {
        padding: 1.5rem;
    }
}
</style>
