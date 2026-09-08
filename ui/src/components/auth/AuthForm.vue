<script setup lang="ts">
import Checkbox from '../primitives/Checkbox.vue';
import AppLink from '../primitives/AppLink.vue';
import Button from '../primitives/Button.vue';
import FormField from '../primitives/FormField.vue';
import Input from '../primitives/Input.vue';

type Mode = 'login' | 'register' | 'forgot' | 'reset' | 'verify';
type Field = 'username' | 'name' | 'email' | 'password' | 'password_confirmation' | 'remember';

defineProps<{
    mode: Mode;
    values: Record<string, string | boolean>;
    errors?: Record<string, string>;
    processing?: boolean;
    status?: string;
    registrationEnabled?: boolean;
}>();

const emit = defineEmits<{
    submit: [];
    update: [field: Field, value: string | boolean];
}>();
</script>

<template>
    <form class="space-y-5" @submit.prevent="emit('submit')">
        <p
            v-if="status"
            class="rounded-lg border border-[var(--fb-border)] bg-[var(--fb-selected-surface)] px-3 py-2 text-sm text-[var(--fb-text)]"
            role="status"
        >
            {{ status }}
        </p>
        <template v-if="mode === 'register'">
            <FormField
                id="auth-username"
                label="Username"
                :error="errors?.username"
                description="3-24 lowercase letters, numbers, or underscores."
                v-slot="field"
                ><Input
                    :id="field.id"
                    name="username"
                    :value="String(values.username ?? '')"
                    autocomplete="username"
                    :aria-describedby="field.describedBy"
                    :aria-invalid="field.invalid"
                    :invalid="field.invalid"
                    :disabled="processing"
                    @input="emit('update', 'username', ($event.target as HTMLInputElement).value)"
            /></FormField>
        </template>
        <FormField
            v-if="mode !== 'verify'"
            id="auth-email"
            :label="mode === 'register' ? 'Email' : 'Email or username'"
            :error="errors?.email"
            v-slot="field"
            ><Input
                :id="field.id"
                name="email"
                :value="String(values.email ?? '')"
                :type="mode === 'register' ? 'email' : 'text'"
                :autocomplete="mode === 'register' ? 'email' : 'username'"
                required
                :aria-describedby="field.describedBy"
                :aria-invalid="field.invalid"
                :invalid="field.invalid"
                :disabled="processing"
                @input="emit('update', 'email', ($event.target as HTMLInputElement).value)"
        /></FormField>
        <FormField
            v-if="mode === 'login' || mode === 'register' || mode === 'reset'"
            id="auth-password"
            label="Password"
            :error="errors?.password"
        >
            <template v-if="mode === 'login'" #label-action>
                <AppLink
                    href="/forgot-password"
                    preserve-state
                    preserve-scroll
                    class="fb-text-link text-xs"
                    >Forgot password?</AppLink
                >
            </template>
            <template #default="field"
                ><Input
                    :id="field.id"
                    name="password"
                    :value="String(values.password ?? '')"
                    type="password"
                    :autocomplete="mode === 'login' ? 'current-password' : 'new-password'"
                    required
                    :aria-describedby="field.describedBy"
                    :aria-invalid="field.invalid"
                    :invalid="field.invalid"
                    :disabled="processing"
                    @input="emit('update', 'password', ($event.target as HTMLInputElement).value)"
            /></template>
        </FormField>
        <FormField
            v-if="mode === 'register' || mode === 'reset'"
            id="auth-password-confirmation"
            label="Confirm password"
            :error="errors?.password_confirmation"
            v-slot="field"
            ><Input
                :id="field.id"
                name="password_confirmation"
                :value="String(values.password_confirmation ?? '')"
                type="password"
                autocomplete="new-password"
                required
                :aria-describedby="field.describedBy"
                :aria-invalid="field.invalid"
                :invalid="field.invalid"
                :disabled="processing"
                @input="
                    emit(
                        'update',
                        'password_confirmation',
                        ($event.target as HTMLInputElement).value,
                    )
                "
        /></FormField>
        <label v-if="mode === 'login'" class="fb-check-label" :class="{ 'opacity-60': processing }"
            ><Checkbox
                :model-value="Boolean(values.remember)"
                :disabled="processing"
                @update:model-value="emit('update', 'remember', Boolean($event))"
            />
            Keep me signed in</label
        >
        <p v-if="mode === 'verify'" class="text-sm leading-6 text-[var(--fb-text)]">
            Check your inbox for a verification link. You can request another link if needed.
        </p>
        <Button class="w-full" :disabled="processing" type="submit">{{
            mode === 'login'
                ? 'Sign in'
                : mode === 'register'
                  ? 'Create account'
                  : mode === 'forgot'
                    ? 'Email reset link'
                    : mode === 'reset'
                      ? 'Reset password'
                      : 'Resend verification email'
        }}</Button>
    </form>
</template>
