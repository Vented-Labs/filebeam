<script setup lang="ts">
import { onBeforeUnmount, ref, useId, watch } from 'vue';
import Button from '../primitives/Button.vue';
import Icon from '../primitives/Icon.vue';

const props = withDefaults(
    defineProps<{ command: string; label?: string; copyLabel?: string; disabled?: boolean }>(),
    { label: 'CLI command', copyLabel: 'Copy command', disabled: false },
);
const field = ref<HTMLTextAreaElement>();
const state = ref<'idle' | 'pending' | 'copied' | 'error'>('idle');
const feedbackId = useId();
let generation = 0;
let timer: ReturnType<typeof setTimeout> | undefined;

function reset(): void {
    generation++;
    clearTimeout(timer);
    state.value = 'idle';
}
watch([() => props.command, () => props.disabled], reset, { flush: 'sync' });
onBeforeUnmount(reset);

async function copy(): Promise<void> {
    if (props.disabled || !props.command || state.value === 'pending') return;
    const id = ++generation;
    const command = props.command;
    clearTimeout(timer);
    state.value = 'pending';
    try {
        if (!navigator.clipboard) throw new Error('Clipboard unavailable');
        await navigator.clipboard.writeText(command);
        if (id !== generation) return;
        state.value = 'copied';
        timer = setTimeout(() => (state.value = 'idle'), 2200);
    } catch {
        if (id !== generation) return;
        state.value = 'error';
        field.value?.focus({ preventScroll: true });
        field.value?.select();
    }
}
</script>

<template>
    <div class="cli-command" :data-state="state">
        <div class="cli-command__row">
            <span class="cli-command__prompt" aria-hidden="true">$</span>
            <textarea
                ref="field"
                class="cli-command__value fb-code"
                :value="command"
                :aria-label="label"
                :aria-describedby="state === 'error' ? feedbackId : undefined"
                readonly
                rows="2"
                wrap="off"
                spellcheck="false"
                autocomplete="off"
            />
            <Button
                variant="secondary"
                class="cli-command__copy"
                :disabled="disabled || !command || state === 'pending'"
                :aria-busy="state === 'pending' || undefined"
                :aria-label="state === 'idle' ? copyLabel : `${copyLabel}: ${state}`"
                @click="copy"
            >
                <span class="cli-command__sizer" aria-hidden="true"
                    ><Icon name="copy" :size="15" />{{ copyLabel }}</span
                >
                <span class="cli-command__content">
                    <Icon
                        :name="
                            state === 'pending' ? 'loader' : state === 'copied' ? 'check' : 'copy'
                        "
                        :size="15"
                    />
                    {{
                        state === 'pending'
                            ? 'Copying'
                            : state === 'copied'
                              ? 'Copied'
                              : state === 'error'
                                ? 'Retry copy'
                                : copyLabel
                    }}
                </span>
            </Button>
        </div>
        <p v-if="state === 'error'" :id="feedbackId" class="cli-command__feedback" role="status">
            Copy failed. The command is selected; copy it manually or retry.
        </p>
        <span v-else class="sr-only" role="status">{{
            state === 'copied' ? 'Command copied.' : ''
        }}</span>
    </div>
</template>

<style scoped>
.cli-command {
    min-width: 0;
    text-align: left;
}
.cli-command__row {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.625rem;
    padding: 0.625rem;
    border: 1px solid var(--fb-control-border);
    border-radius: var(--fb-radius-control);
    background: var(--fb-surface-sunken);
}
.cli-command__prompt {
    color: var(--fb-text-subtle);
    font: 0.75rem var(--fb-font-code);
    user-select: none;
}
.cli-command__value {
    display: block;
    min-width: 0;
    width: 100%;
    padding: 0.375rem 0.125rem;
    resize: none;
    border: 0;
    border-radius: 0.25rem;
    background: transparent;
    color: var(--fb-text);
    font-size: 0.6875rem;
    line-height: 1.6;
    overflow: auto;
}
.cli-command__value:focus-visible {
    outline: 2px solid var(--fb-focus);
    outline-offset: 2px;
}
.cli-command__copy {
    display: inline-grid;
    min-width: 8.5rem;
    min-height: 2.3125rem;
    font-size: 0.6875rem;
    white-space: nowrap;
}
.cli-command__sizer,
.cli-command__content {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    grid-area: 1 / 1;
}
.cli-command__sizer {
    visibility: hidden;
}
.cli-command__feedback {
    margin: 0.625rem 0 0;
    color: var(--fb-warning);
    font-size: 0.75rem;
    line-height: 1.6;
}
@media (max-width: 560px) {
    .cli-command__row {
        grid-template-columns: auto minmax(0, 1fr);
    }
    .cli-command__copy {
        width: 100%;
        grid-column: 1 / -1;
    }
}
</style>
