<script setup lang="ts">
import { RadioGroupItem, RadioGroupRoot } from 'reka-ui';
import type { DesktopPlatform } from '../../lib/platform';
import Icon from './Icon.vue';
import Tooltip from './Tooltip.vue';

const selected = defineModel<DesktopPlatform>();
const emit = defineEmits<{ escapeKeyDown: [event: KeyboardEvent] }>();
const platforms = [
    { value: 'linux', label: 'Linux' },
    { value: 'macos', label: 'macOS' },
    { value: 'windows', label: 'Windows' },
] as const;

function selectWithKeyboard(event: KeyboardEvent): void {
    if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
    const current = Math.max(
        0,
        platforms.findIndex((platform) => platform.value === selected.value),
    );
    const forward = event.key === 'ArrowRight' || event.key === 'ArrowDown';
    selected.value =
        platforms[(current + (forward ? 1 : -1) + platforms.length) % platforms.length]!.value;
}
</script>

<template>
    <RadioGroupRoot
        v-model="selected"
        class="fb-platform-select"
        orientation="horizontal"
        aria-label="Platform"
        @keydown="selectWithKeyboard"
    >
        <Tooltip
            v-for="platform in platforms"
            :key="platform.value"
            :content="platform.label"
            :delay="150"
            inline
            @escape-key-down="emit('escapeKeyDown', $event)"
        >
            <RadioGroupItem
                :value="platform.value"
                class="fb-platform-select__item"
                :aria-label="platform.label"
            >
                <Icon :name="`os-${platform.value}`" :size="26" />
            </RadioGroupItem>
        </Tooltip>
    </RadioGroupRoot>
</template>

<style scoped>
.fb-platform-select {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.375rem;
    margin-bottom: 1rem;
    padding: 0.3rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.75rem;
    background: var(--fb-surface-sunken);
    box-shadow: inset 0 1px 2px #0003;
}
.fb-platform-select__item {
    position: relative;
    display: grid;
    min-width: 0;
    height: 3.25rem;
    place-items: center;
    border: 1px solid transparent;
    border-radius: 0.55rem;
    background: transparent;
    color: var(--fb-text-subtle);
    cursor: pointer;
    transition:
        border-color var(--fb-duration-control) ease,
        background var(--fb-duration-control) ease,
        color var(--fb-duration-control) ease,
        transform var(--fb-duration-control) var(--fb-ease);
}
.fb-platform-select__item:hover {
    background: #ffffff06;
    color: var(--fb-text);
}
.fb-platform-select__item:active {
    transform: scale(0.97);
}
.fb-platform-select__item:focus-visible {
    outline: 2px solid var(--fb-focus);
    outline-offset: 2px;
}
.fb-platform-select__item[aria-checked='true'] {
    border-color: #806191;
    background: #32253f;
    color: #d4b3fa;
    box-shadow: inset 0 1px 0 #ffffff0a;
}
@media (prefers-reduced-motion: reduce) {
    .fb-platform-select__item {
        transition: none;
    }
}
</style>
