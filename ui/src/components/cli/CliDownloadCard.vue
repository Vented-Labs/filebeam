<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import {
    buildDownloadCommand,
    cliUnavailableReason,
    type CliTransfer,
} from '../../lib/cli-commands';
import Icon from '../primitives/Icon.vue';
import CliCommandField from './CliCommandField.vue';
import CliInstallButton from './CliInstallButton.vue';

const props = defineProps<{ target: string; transfer: CliTransfer; passwordProtected?: boolean }>();
const now = ref(Date.now());
let timer: ReturnType<typeof setInterval> | undefined;
onMounted(() => {
    timer = setInterval(() => (now.value = Date.now()), 1000);
});
onBeforeUnmount(() => clearInterval(timer));
const presentation = computed(() => {
    const reason = cliUnavailableReason(props.transfer, now.value);
    if (reason) return { reason };
    try {
        return { command: buildDownloadCommand(props.target) };
    } catch {
        return {
            reason: 'This link format is not supported by the CLI. Use the browser to download.',
        };
    }
});
const includesKey = computed(() => props.target.includes('#'));
</script>

<template>
    <section
        class="cli-download"
        aria-label="Download with CLI"
        :data-available="Boolean(presentation.command)"
    >
        <header>
            <span class="cli-download__icon"><Icon name="code" :size="19" /></span>
            <div>
                <h2>Download with CLI</h2>
                <p>Run this command in your terminal.</p>
            </div>
            <CliInstallButton />
        </header>
        <CliCommandField
            v-if="presentation.command"
            :command="presentation.command"
            label="Download command"
        />
        <p v-else class="cli-download__unavailable" role="status">{{ presentation.reason }}</p>
        <p v-if="presentation.command" class="cli-download__note">
            <Icon name="lock" :size="13" />
            <span
                >{{
                    includesKey
                        ? 'Contains the decryption key. Keep it private; commands may remain in shell history.'
                        : 'The key is separate from this link. Enter it when the CLI prompts; it has not been added to the command.'
                }}
                <template v-if="passwordProtected">
                    Enter the transfer password only at the CLI prompt.</template
                >
            </span>
        </p>
    </section>
</template>

<style scoped>
.cli-download {
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
    margin-top: 1rem;
    padding: 1rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.875rem;
    background: var(--fb-surface);
    text-align: left;
}
.cli-download header {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    margin-bottom: 0.875rem;
}
.cli-download__icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 2rem;
    height: 2rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.625rem;
    background: var(--fb-selected-surface);
    color: var(--fb-accent-text);
}
.cli-download h2 {
    margin: 0;
    color: var(--fb-text);
    font-size: 0.8125rem;
    font-weight: 500;
}
.cli-download header p {
    margin: 0.125rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.6875rem;
}
.cli-download :deep(.cli-install-button) {
    margin-left: auto;
    min-height: 2rem;
    padding: 0.25rem;
    font-size: 0.6875rem;
    color: var(--fb-accent-text);
}
.cli-download :deep(.cli-install-button > .fb-icon) {
    display: none;
}
.cli-download__note {
    display: flex;
    align-items: start;
    gap: 0.5rem;
    margin: 0.75rem 0 0;
    color: var(--fb-text-subtle);
    font-size: 0.6875rem;
    line-height: 1.7;
}
.cli-download__note > .fb-icon {
    margin-top: 0.125rem;
}
.cli-download__unavailable {
    margin: 0;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    line-height: 1.7;
}
@media (max-width: 380px) {
    .cli-download {
        padding: 0.75rem;
    }
    .cli-download header {
        flex-wrap: wrap;
    }
}
</style>
