<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import AnimatedHeight from '../layout/AnimatedHeight.vue';
import Button from '../primitives/Button.vue';
import FilebeamIcon from '../primitives/FilebeamIcon.vue';
import FormField from '../primitives/FormField.vue';
import Input from '../primitives/Input.vue';

const props = defineProps<{
    passwordRequired: boolean;
    unlocking: boolean;
}>();
const emit = defineEmits<{
    unlock: [key: string, password: string];
    clearError: [];
}>();

const key = ref('');
const password = ref('');
const step = ref<'key' | 'password'>('key');
const loaded = ref(false);
const keyInput = ref<InstanceType<typeof Input>>();
const passwordInput = ref<InstanceType<typeof Input>>();
const validKey = computed(() => /^(?:v1\.)?[A-Za-z0-9_-]{43}$/.test(key.value.trim()));
const keyError = ref('');

function focusStep(): void {
    void nextTick(() => {
        (step.value === 'key' ? keyInput.value : passwordInput.value)?.$el?.focus();
    });
}

function continueWithKey(): void {
    if (props.unlocking) return;
    emit('clearError');
    if (!validKey.value) {
        keyError.value = 'Invalid decryption key - please enter the decryption key';
        return;
    }
    keyError.value = '';
    if (props.passwordRequired) step.value = 'password';
    else emit('unlock', key.value.trim(), '');
}

function unlockWithPassword(): void {
    if (!props.unlocking) emit('unlock', key.value.trim(), password.value);
}

function changeKey(): void {
    if (props.unlocking) return;
    key.value = '';
    password.value = '';
    keyError.value = '';
    step.value = 'key';
}

watch(step, () => {
    emit('clearError');
    focusStep();
});

onMounted(() => {
    const includedKey = new URLSearchParams(window.location.hash.slice(1)).get('k') ?? '';
    key.value = includedKey;
    if (props.passwordRequired && validKey.value) step.value = 'password';
    loaded.value = true;
    focusStep();
});

onBeforeUnmount(() => {
    key.value = '';
    password.value = '';
});
</script>

<template>
    <section v-if="loaded" class="mx-auto max-w-md py-12 text-center">
        <div
            class="mx-auto grid size-12 place-items-center rounded-2xl bg-[var(--fb-selected-surface)] text-[var(--fb-accent-text)]"
        >
            <FilebeamIcon name="lock" />
        </div>
        <h1 class="mt-5 text-3xl font-semibold">
            {{ step === 'password' ? 'Enter the password' : 'Unlock this transfer' }}
        </h1>
        <AnimatedHeight class="focus-safe-height mt-3">
            <Transition name="unlock-step" mode="out-in" @after-enter="focusStep">
                <form
                    v-if="step === 'key'"
                    key="key"
                    class="space-y-4 text-left"
                    @submit.prevent="continueWithKey"
                >
                    <p class="text-center text-[var(--fb-text-muted)]">
                        Use the generated key shared separately by the sender. It is never sent to
                        the server.
                    </p>
                    <FormField
                        id="transfer-key"
                        label="Generated key"
                        :error="keyError"
                        description="Paste the generated key shared by the sender."
                    >
                        <template #default="{ id, describedBy, invalid }">
                            <Input
                                :id="id"
                                ref="keyInput"
                                v-model="key"
                                class="fb-code"
                                placeholder="v1.xxxxxxxxx"
                                autocomplete="off"
                                :aria-describedby="describedBy"
                                :aria-invalid="invalid"
                                :disabled="unlocking"
                                @input="keyError = ''"
                            />
                        </template>
                    </FormField>
                    <Button class="w-full" type="submit" :disabled="unlocking">
                        <FilebeamIcon v-if="unlocking" name="loader" :size="17" />
                        {{ unlocking ? 'Unlocking' : passwordRequired ? 'Continue' : 'Unlock' }}
                    </Button>
                </form>
                <form
                    v-else
                    key="password"
                    class="space-y-4 text-left"
                    @submit.prevent="unlockWithPassword"
                >
                    <p class="text-center text-[var(--fb-text-muted)]">
                        This transfer also needs its password. It is never sent to the server.
                    </p>
                    <FormField id="transfer-password" label="Password">
                        <template #default="{ id, describedBy, invalid }">
                            <Input
                                :id="id"
                                ref="passwordInput"
                                v-model="password"
                                type="password"
                                autocomplete="current-password"
                                :aria-describedby="describedBy"
                                :aria-invalid="invalid"
                                :disabled="unlocking"
                            />
                        </template>
                    </FormField>
                    <div class="flex items-center justify-between gap-3">
                        <Button variant="ghost" @click="changeKey">Change key</Button>
                        <Button type="submit" :disabled="unlocking">
                            <FilebeamIcon v-if="unlocking" name="loader" :size="17" />
                            {{ unlocking ? 'Unlocking' : 'Unlock' }}
                        </Button>
                    </div>
                </form>
            </Transition>
        </AnimatedHeight>
    </section>
</template>

<style scoped>
.unlock-step-enter-active,
.unlock-step-leave-active {
    transition:
        opacity 160ms ease,
        transform 160ms ease;
}
.unlock-step-enter-from,
.unlock-step-leave-to {
    opacity: 0;
    transform: translateY(4px);
}
@media (prefers-reduced-motion: reduce) {
    .unlock-step-enter-active,
    .unlock-step-leave-active {
        transition: none;
    }
}
</style>
