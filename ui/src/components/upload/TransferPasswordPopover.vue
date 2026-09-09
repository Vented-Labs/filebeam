<script setup lang="ts">
import { computed, nextTick, ref } from 'vue';
import { PopoverAnchor, PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from 'reka-ui';
import Button from '../primitives/Button.vue';
import Icon from '../primitives/Icon.vue';
import Input from '../primitives/Input.vue';
import AnimatedReveal from '../layout/AnimatedReveal.vue';

const props = withDefaults(
    defineProps<{ disabled?: boolean; invalid?: boolean; labelledBy?: string }>(),
    {
        disabled: false,
        invalid: false,
        labelledBy: undefined,
    },
);
const password = defineModel<string>({ required: true });
const open = ref(false);
const revealed = ref(false);
const input = ref<InstanceType<typeof Input>>();
const locallyTouched = ref(false);
const generationError = ref('');
const tooShort = computed(() => password.value.length > 0 && Array.from(password.value).length < 8);
const showError = computed(() => (props.invalid || locallyTouched.value) && tooShort.value);
const triggerLabelledBy = computed(() =>
    props.labelledBy ? `${props.labelledBy} password-trigger-state` : 'password-trigger-state',
);

function opened(value: boolean): void {
    open.value = value;
    if (value) void nextTick(() => input.value?.focus({ preventScroll: true }));
}
function done(): void {
    locallyTouched.value = true;
    if (tooShort.value) {
        input.value?.focus();
        return;
    }
    open.value = false;
}
function remove(): void {
    password.value = '';
    locallyTouched.value = false;
    revealed.value = false;
    open.value = false;
}
function generatePassword(): void {
    if (props.disabled) return;
    generationError.value = '';
    try {
        const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';
        // 64 equally likely symbols per byte: 24 characters provide 144 bits of entropy.
        const bytes = crypto.getRandomValues(new Uint8Array(24));
        password.value = Array.from(bytes, (byte) => alphabet[byte & 63]).join('');
        locallyTouched.value = false;
        void nextTick(() => input.value?.focus({ preventScroll: true }));
    } catch {
        generationError.value =
            'Secure password generation is unavailable. Enter a password manually.';
    }
}
</script>

<template>
    <PopoverRoot :open="open" @update:open="opened">
        <PopoverAnchor as-child>
            <PopoverTrigger as-child>
                <button
                    type="button"
                    class="password-trigger"
                    data-testid="prism-password-trigger"
                    :class="{ 'password-trigger--invalid': showError }"
                    :disabled="disabled"
                    :aria-labelledby="triggerLabelledBy"
                    :aria-invalid="showError || undefined"
                    :aria-describedby="showError ? 'password-trigger-error' : undefined"
                >
                    <Icon name="key" :size="15" />
                    <span id="password-trigger-state">{{
                        password ? 'Password set' : 'At least 8 characters'
                    }}</span>
                    <Icon :name="password && !showError ? 'check' : 'plus'" :size="15" />
                </button>
            </PopoverTrigger>
        </PopoverAnchor>
        <PopoverPortal>
            <PopoverContent
                data-testid="prism-password-popover"
                class="password-popover"
                :side-offset="9"
                :collision-padding="8"
                align="start"
            >
                <div class="password-popover__title">
                    <span>Password <small>optional</small></span
                    ><Icon name="key" :size="15" />
                </div>
                <div class="password-popover__input">
                    <Input
                        id="transfer-password"
                        ref="input"
                        v-model="password"
                        :type="revealed ? 'text' : 'password'"
                        :invalid="showError"
                        autocomplete="new-password"
                        minlength="8"
                        maxlength="128"
                        spellcheck="false"
                        placeholder="At least 8 characters"
                        :aria-label="labelledBy ? undefined : 'Password'"
                        :aria-labelledby="labelledBy"
                        :aria-describedby="'password-help'"
                        @blur="locallyTouched = true"
                    />
                    <button
                        type="button"
                        class="password-popover__reveal"
                        :aria-label="revealed ? 'Hide password' : 'Show password'"
                        :aria-pressed="revealed"
                        @click="revealed = !revealed"
                    >
                        <Icon :name="revealed ? 'eye-off' : 'eye'" :size="16" />
                    </button>
                </div>
                <p
                    id="password-help"
                    class="password-popover__help"
                    :class="{ 'password-popover__help--error': showError }"
                    :role="showError ? 'alert' : undefined"
                >
                    <Icon :name="showError ? 'alert' : password ? 'check' : 'lock'" :size="13" />
                    {{
                        showError
                            ? 'Use at least 8 characters.'
                            : password
                              ? 'Password protection is on.'
                              : 'Optional. Share it separately.'
                    }}
                </p>
                <Button
                    class="password-popover__generate"
                    variant="secondary"
                    :disabled="disabled"
                    @click="generatePassword"
                >
                    <Icon name="redo" :size="14" />Generate secure password
                </Button>
                <p
                    v-if="generationError"
                    class="password-popover__help password-popover__help--error"
                    role="alert"
                >
                    {{ generationError }}
                </p>
                <div class="password-popover__actions">
                    <Button variant="ghost" @click="remove">Remove</Button>
                    <Button @click="done">Done <Icon name="check" :size="14" /></Button>
                </div>
            </PopoverContent>
        </PopoverPortal>
    </PopoverRoot>
    <AnimatedReveal :show="showError">
        <p id="password-trigger-error" class="password-trigger__error">
            Use at least 8 characters.
        </p>
    </AnimatedReveal>
</template>

<style>
.password-trigger {
    display: flex;
    width: 100%;
    height: 2.625rem;
    align-items: center;
    gap: 0.5625rem;
    padding: 0 0.6875rem;
    border: 1px solid #44374f;
    border-radius: var(--fb-radius-control);
    color: var(--fb-text-muted);
    background: var(--fb-surface-sunken);
    font: inherit;
    font-size: 0.8125rem;
    text-align: left;
    cursor: pointer;
    transition:
        border-color var(--fb-duration-control) ease,
        background var(--fb-duration-control) ease,
        transform var(--fb-duration-control) var(--fb-ease);
}
.password-trigger span {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
.password-trigger:hover:not(:disabled) {
    border-color: #7f668e;
    background: #1d1727;
}
.password-trigger[data-state='open'] {
    border-color: #ab8ac4;
    background: #201828;
    outline: 2px solid #b391d4;
    outline-offset: 2px;
}
.password-trigger--invalid {
    border-color: var(--fb-danger);
    color: var(--fb-danger);
}
.password-trigger:disabled {
    cursor: not-allowed;
    opacity: 0.48;
}
.password-trigger__error {
    margin: 0.5rem 0 0;
    color: var(--fb-danger);
    font-size: 0.6875rem;
    line-height: 1.5;
}
.password-popover {
    z-index: 80;
    width: min(19.375rem, calc(100vw - 1rem));
    box-sizing: border-box;
    padding: 1.0625rem;
    border: 1px solid #5b496c;
    border-radius: 1rem;
    color: var(--fb-text);
    background: #25202f;
    box-shadow: var(--fb-shadow-popover);
    transform-origin: var(--reka-popover-content-transform-origin);
    animation: fb-menu-in var(--fb-duration-menu-in) var(--fb-ease);
}
.password-popover[data-state='closed'] {
    animation: fb-menu-out var(--fb-duration-menu-out) var(--fb-ease);
}
.password-popover__title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.9375rem;
    font-size: 0.875rem;
}
.password-popover__title small {
    color: var(--fb-text-subtle);
    font-size: inherit;
}
.password-popover__input {
    position: relative;
}
.password-popover__input .fb-input {
    height: 2.75rem;
    padding-right: 2.75rem;
    border-color: #6c587d;
    border-radius: 0.5625rem;
}
.password-popover__reveal {
    position: absolute;
    top: 50%;
    right: 0.4375rem;
    display: grid;
    width: 1.875rem;
    height: 1.875rem;
    place-items: center;
    border: 0;
    border-radius: 0.375rem;
    color: var(--fb-text-muted);
    background: transparent;
    transform: translateY(-50%);
    cursor: pointer;
}
.password-popover__reveal:hover {
    color: var(--fb-text);
    background: #ffffff09;
}
.password-popover__help {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    margin: 0.625rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.6875rem;
}
.password-popover__help--error {
    color: var(--fb-danger);
}
.password-popover__actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 1rem;
}
.password-popover__generate {
    width: 100%;
    min-height: 2.1875rem;
    margin-top: 0.75rem;
    font-size: 0.75rem;
}
.password-popover__actions .fb-button {
    min-height: 2.1875rem;
}
@media (prefers-reduced-motion: reduce) {
    .password-trigger,
    .password-popover,
    .password-popover[data-state='closed'] {
        transition: none;
        animation: none;
    }
}
</style>
