<script setup lang="ts">
import { inject } from 'vue';
import Icon from '../primitives/Icon.vue';
import { cliInstallKey } from './context';

const openInstall = inject(cliInstallKey);
</script>

<template>
    <button
        v-if="openInstall"
        type="button"
        class="cli-footer-launcher"
        aria-label="Install CLI — open terminal installation instructions"
        aria-haspopup="dialog"
        @click="openInstall"
    >
        <Icon name="code" :size="19" />
        <span class="cli-footer-divider" aria-hidden="true" />
        <code aria-hidden="true"
            ><span class="cli-footer-prompt">$</span> <strong>beam</strong> down
            <span class="cli-footer-argument">&lt;url or ulid&gt;</span></code
        >
        <span class="cli-footer-comment" aria-hidden="true"># install CLI</span>
        <Icon class="cli-footer-arrow" name="arrow-up-right" :size="14" />
    </button>
</template>

<style scoped>
.cli-footer-launcher {
    appearance: none;
    display: flex;
    align-items: center;
    gap: 13px;
    width: 100%;
    max-width: 556px;
    min-width: 0;
    height: 38px;
    padding: 0 12px;
    border: 1px solid transparent;
    border-bottom-color: var(--fb-line-cli);
    border-radius: 6px;
    background: var(--fb-wash-01);
    color: var(--fb-text-muted);
    box-shadow: none;
    font: 12px/1.4 var(--fb-font-code);
    text-align: left;
    cursor: pointer;
    transition:
        background var(--fb-duration-control) cubic-bezier(0.22, 1, 0.36, 1),
        border-color var(--fb-duration-control) cubic-bezier(0.22, 1, 0.36, 1),
        transform var(--fb-duration-control) cubic-bezier(0.22, 1, 0.36, 1);
}
.cli-footer-launcher > :deep(.fb-icon) {
    color: var(--fb-cli-icon);
}
.cli-footer-divider {
    width: 1px;
    height: 16px;
    background: var(--fb-wash-10);
    flex: none;
}
code {
    white-space: nowrap;
    font: inherit;
    color: var(--fb-cli-command);
}
strong {
    color: var(--fb-cli-accent);
    font-weight: 600;
}
.cli-footer-prompt {
    color: var(--fb-cli-option);
    margin-right: 9px;
}
.cli-footer-argument {
    color: var(--fb-cli-value);
}
.cli-footer-comment {
    margin-left: auto;
    white-space: nowrap;
    font-size: 11px;
    color: var(--fb-cli-label);
}
.cli-footer-arrow {
    transition: transform var(--fb-duration-control) cubic-bezier(0.22, 1, 0.36, 1);
}
.cli-footer-launcher:hover {
    background: var(--fb-wash-04);
    border-color: var(--fb-cli-border);
}
.cli-footer-launcher:hover .cli-footer-arrow {
    transform: translate(1px, -1px);
}
.cli-footer-launcher:active {
    transform: translateY(1px);
    background: var(--fb-wash-07);
    border-color: var(--fb-cli-hover-border);
}
.cli-footer-launcher:focus-visible {
    outline: 2px solid var(--fb-focus);
    outline-offset: 3px;
}
@media (max-width: 900px) {
    .cli-footer-launcher {
        display: none;
    }
}
@media (prefers-reduced-motion: reduce) {
    .cli-footer-launcher,
    .cli-footer-arrow {
        transition: none;
        transform: none !important;
    }
}
</style>
