<script setup lang="ts">
import { computed, nextTick, ref, useId } from 'vue';
import {
    PopoverContent,
    PopoverPortal,
    PopoverRoot,
    PopoverTrigger,
    RadioGroupItem,
    RadioGroupRoot,
} from 'reka-ui';
import { useAppearance } from '../../composables/useAppearance';
import type { AppearanceMode, ThemePreset } from '../../lib/appearance-types';
import Button from '../primitives/Button.vue';
import Icon from '../primitives/Icon.vue';
import Tooltip from '../primitives/Tooltip.vue';

const appearance = useAppearance();
const { account, catalog, customColors, status, error } = appearance;
const mode = computed<AppearanceMode>({
    get: () => appearance.preference.value,
    set: appearance.set,
});
const preset = computed<ThemePreset>({
    get: () => appearance.preset.value,
    set: appearance.setPreset,
});
const open = ref(false);
const content = ref<InstanceType<typeof PopoverContent>>();
const titleId = useId();
const modes = [
    { value: 'system', label: 'System', icon: 'monitor' },
    { value: 'light', label: 'Light', icon: 'sun' },
    { value: 'dark', label: 'Dark', icon: 'moon' },
] as const;
function nextChoice<T extends string>(
    event: KeyboardEvent,
    choices: T[],
    current: T,
): T | undefined {
    if (
        !choices.length ||
        !['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)
    )
        return;
    if (event.key === 'Home') return choices[0];
    if (event.key === 'End') return choices[choices.length - 1];
    const index = Math.max(0, choices.indexOf(current));
    const direction = event.key === 'ArrowRight' || event.key === 'ArrowDown' ? 1 : -1;
    return choices[(index + direction + choices.length) % choices.length];
}
function modeKey(event: KeyboardEvent): void {
    const value = nextChoice(
        event,
        modes.map((item) => item.value),
        mode.value,
    );
    if (value) mode.value = value;
}
function colorKey(event: KeyboardEvent): void {
    const value = nextChoice(
        event,
        catalog.value
            .filter((item) => customColors.value || item.id === 'instance')
            .map((item) => item.id),
        preset.value,
    );
    if (value) preset.value = value;
}
function opened(value: boolean): void {
    open.value = value;
    if (value)
        void nextTick(() =>
            content.value?.$el
                ?.querySelector('[aria-checked="true"]')
                ?.focus({ preventScroll: true }),
        );
}
</script>

<template>
    <PopoverRoot :open="open" @update:open="opened">
        <PopoverTrigger as-child>
            <button
                type="button"
                class="fb-appearance-trigger"
                aria-label="Appearance"
                title="Appearance"
                data-appearance-trigger
            >
                <Icon name="brush" :size="18" />
            </button>
        </PopoverTrigger>
        <PopoverPortal>
            <PopoverContent
                ref="content"
                class="fb-appearance-popover"
                side="top"
                align="start"
                :side-offset="9"
                :collision-padding="8"
                :aria-labelledby="titleId"
            >
                <div class="fb-appearance-popover__title">
                    <span :id="titleId">Appearance</span><Icon name="brush" :size="16" />
                </div>
                <p class="fb-appearance-popover__label">Theme</p>
                <RadioGroupRoot
                    v-model="mode"
                    @keydown="modeKey"
                    orientation="horizontal"
                    aria-label="Theme"
                    class="fb-appearance-modes"
                >
                    <RadioGroupItem
                        v-for="item in modes"
                        :key="item.value"
                        :value="item.value"
                        class="fb-appearance-mode"
                    >
                        <Icon :name="item.icon" :size="15" />{{ item.label }}
                    </RadioGroupItem>
                </RadioGroupRoot>
                <p class="fb-appearance-popover__label">Color</p>
                <RadioGroupRoot
                    v-model="preset"
                    aria-label="Color"
                    class="fb-appearance-colors"
                    @keydown="colorKey"
                >
                    <Tooltip
                        v-for="item in catalog"
                        :key="item.id"
                        :content="item.label"
                        :delay="150"
                        @escape-key-down="open = false"
                    >
                        <RadioGroupItem
                            :value="item.id"
                            :aria-label="item.label"
                            :disabled="!customColors && item.id !== 'instance'"
                            class="fb-appearance-color"
                            :style="{
                                '--fb-preset-color': item.primary,
                                '--fb-preset-ink': item.on_color,
                            }"
                        >
                            <span class="fb-appearance-color__swatch"
                                ><Icon v-if="preset === item.id" name="check" :size="15" /><Icon
                                    v-else-if="item.id === 'instance'"
                                    name="redo"
                                    :size="15"
                            /></span>
                        </RadioGroupItem>
                    </Tooltip>
                </RadioGroupRoot>
                <p v-if="!customColors" class="fb-appearance-help">
                    Color presets require PHP GD. Using the instance color.
                </p>
                <p
                    class="fb-appearance-help"
                    :class="{ 'fb-appearance-help--error': error }"
                    role="status"
                >
                    {{
                        error ||
                        (status === 'saving'
                            ? 'Saving…'
                            : account
                              ? 'Saved to your account.'
                              : 'Saved in this browser.')
                    }}
                </p>
                <div class="fb-appearance-actions">
                    <Button variant="ghost" @click="appearance.reset">Reset</Button>
                    <Button v-if="error" variant="secondary" @click="appearance.retry"
                        >Retry</Button
                    >
                    <Button @click="open = false">Done <Icon name="check" :size="14" /></Button>
                </div>
            </PopoverContent>
        </PopoverPortal>
    </PopoverRoot>
</template>

<style>
.fb-appearance-trigger {
    display: grid;
    flex: none;
    width: 2rem;
    height: 2rem;
    place-items: center;
    padding: 0;
    border: 1px solid var(--fb-control-border);
    border-radius: var(--fb-radius-control);
    color: var(--fb-text-muted);
    background: var(--fb-surface-sunken);
    cursor: pointer;
    transition:
        background var(--fb-duration-control) ease,
        border-color var(--fb-duration-control) ease;
}
.fb-appearance-trigger:hover {
    border-color: var(--fb-password-hover-border);
    background: var(--fb-password-hover-surface);
}
.fb-appearance-trigger[data-state='open'] {
    border-color: var(--fb-password-open-border);
    background: var(--fb-password-open-surface);
}
.fb-appearance-popover {
    z-index: 80;
    width: min(19.375rem, calc(100vw - 1rem));
    max-height: var(--reka-popover-content-available-height);
    overflow-y: auto;
    box-sizing: border-box;
    padding: 1.0625rem;
    border: 1px solid var(--fb-password-menu-border);
    border-radius: 1rem;
    color: var(--fb-text);
    background: var(--fb-password-menu-surface);
    box-shadow: var(--fb-shadow-popover);
    font-family: var(--fb-font-ui);
    transform-origin: var(--reka-popover-content-transform-origin);
    animation: fb-menu-in var(--fb-duration-menu-in) var(--fb-ease);
}
.fb-appearance-popover[data-state='closed'] {
    animation: fb-menu-out var(--fb-duration-menu-out) var(--fb-ease);
}
.fb-appearance-popover__title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.9375rem;
    font-size: 0.875rem;
}
.fb-appearance-popover__label {
    margin: 1rem 0 0.5rem;
    font-size: 0.75rem;
    color: var(--fb-text-muted);
}
.fb-appearance-modes {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.25rem;
}
.fb-appearance-mode {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 0.375rem;
    min-height: 2.1875rem;
    padding: 0.25rem;
    border: 1px solid var(--fb-control-border);
    border-radius: var(--fb-radius-sm);
    background: var(--fb-surface-sunken);
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    cursor: pointer;
}
.fb-appearance-mode[aria-checked='true'] {
    background: var(--fb-choice-surface);
    color: var(--fb-choice-text);
    border-color: var(--fb-choice-border);
}
.fb-appearance-colors {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0.375rem;
}
.fb-appearance-color {
    display: grid;
    place-items: center;
    height: 2.625rem;
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--fb-radius-sm);
    background: transparent;
    cursor: pointer;
}
.fb-appearance-color:not(:disabled):not([aria-disabled='true']):hover {
    background: var(--fb-wash-06);
}
.fb-appearance-color[aria-checked='true'] {
    border-color: var(--fb-choice-border);
    background: var(--fb-selected-surface);
}
.fb-appearance-color:disabled {
    background: var(--fb-disabled-surface);
    cursor: not-allowed;
}
.fb-appearance-color__swatch {
    display: grid;
    place-items: center;
    width: 1.625rem;
    height: 1.625rem;
    border: 1px solid var(--fb-swatch-border);
    box-sizing: border-box;
    border-radius: 50%;
    background: var(--fb-preset-color);
    color: var(--fb-preset-ink);
}
.fb-appearance-mode:focus-visible,
.fb-appearance-color:focus-visible,
.fb-appearance-trigger:focus-visible {
    outline: 2px solid var(--fb-focus);
    outline-offset: 3px;
}
.fb-appearance-help {
    margin: 0.75rem 0 0;
    color: var(--fb-text-subtle);
    font-size: 0.6875rem;
    line-height: 1.5;
}
.fb-appearance-help--error {
    color: var(--fb-danger);
}
.fb-appearance-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 1rem;
}
.fb-appearance-actions .fb-button {
    min-height: 2.1875rem;
}
@media (prefers-reduced-motion: reduce) {
    .fb-appearance-trigger,
    .fb-appearance-popover,
    .fb-appearance-popover[data-state='closed'] {
        animation: none;
        transition: none;
    }
}
</style>
