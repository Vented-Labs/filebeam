<script setup lang="ts">
import {
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
} from 'reka-ui';
import { computed, onMounted, ref } from 'vue';
import type { CliConfig } from '../../types';
import {
    buildInstallCommand,
    cliDefaults,
    detectDesktopPlatform,
    type CliPlatform,
} from '../../lib/cli-commands';
import Button from '../primitives/Button.vue';
import Icon from '../primitives/Icon.vue';
import CliCommandField from './CliCommandField.vue';

const props = defineProps<{ config?: CliConfig }>();
const open = defineModel<boolean>('open', { default: false });
const emit = defineEmits<{ closeAutoFocus: [event: Event] }>();
const selectedPlatform = ref<CliPlatform>();
const installCommand = computed(() => {
    if (!selectedPlatform.value) return;
    try {
        return buildInstallCommand(props.config ?? cliDefaults, selectedPlatform.value);
    } catch {
        return undefined;
    }
});
onMounted(() => {
    const navigatorWithUserAgentData = navigator as Navigator & {
        userAgentData?: { platform?: string; mobile?: boolean };
    };
    selectedPlatform.value = detectDesktopPlatform({
        userAgentDataPlatform: navigatorWithUserAgentData.userAgentData?.platform,
        userAgentDataMobile: navigatorWithUserAgentData.userAgentData?.mobile,
        platform: navigator.platform,
        userAgent: navigator.userAgent,
        maxTouchPoints: navigator.maxTouchPoints,
    });
});
const examples = [
    { command: 'beam', description: 'Interactive TUI' },
    { command: 'beam up <files...>', description: 'Upload via terminal' },
    { command: 'beam down <url or ulid>', description: 'Download via terminal' },
];
</script>

<template>
    <DialogRoot v-model:open="open">
        <DialogPortal>
            <DialogOverlay class="fb-dialog__overlay" />
            <DialogContent
                class="fb-dialog__content cli-install-dialog"
                @close-auto-focus="emit('closeAutoFocus', $event)"
            >
                <p class="cli-install-dialog__eyebrow">
                    <Icon name="code" :size="16" />Filebeam / terminal
                </p>
                <DialogTitle class="fb-dialog__title">Install CLI</DialogTitle>
                <DialogDescription class="fb-dialog__description"
                    >Use Filebeam from your terminal.</DialogDescription
                >
                <section class="cli-install-dialog__installer">
                    <h2><span>01</span>Choose your platform</h2>
                    <label class="cli-install-dialog__platform-label" for="cli-install-platform"
                        >Platform</label
                    >
                    <select id="cli-install-platform" v-model="selectedPlatform">
                        <option :value="undefined">Select a platform</option>
                        <option value="linux">Linux</option>
                        <option value="macos">macOS</option>
                        <option value="windows">Windows</option>
                    </select>
                    <h2><span>02</span>Run the installer</h2>
                    <CliCommandField
                        v-if="installCommand"
                        :command="installCommand"
                        label="Installer command"
                        copy-label="Copy installer"
                    />
                    <p v-else class="cli-install-dialog__unavailable" role="status">
                        {{
                            selectedPlatform
                                ? 'Installer instructions unavailable'
                                : 'Choose a desktop platform to view installer instructions'
                        }}
                    </p>
                    <p class="cli-install-dialog__note">
                        After installation, open a new terminal to use <code>beam</code> from your
                        PATH.
                    </p>
                </section>
                <section class="cli-install-dialog__usage">
                    <h2><span>03</span>Use the CLI</h2>
                    <dl>
                        <div v-for="example in examples" :key="example.command">
                            <dt class="fb-code">{{ example.command }}</dt>
                            <dd>{{ example.description }}</dd>
                        </div>
                    </dl>
                    <p class="cli-install-dialog__note">
                        Upload takes local files or directories. Download takes a transfer URL or
                        ULID. The CLI prompts for a separate key or password when needed.
                    </p>
                    <p class="cli-install-dialog__note">
                        Full URLs connect to their own instance. For a ULID alone, the default is
                        https://filebeam.io; set FILEBEAM_INSTANCE to use another server.
                    </p>
                </section>
                <footer>
                    <DialogClose as-child
                        ><Button>Done<Icon name="check" :size="16" /></Button
                    ></DialogClose>
                </footer>
                <DialogClose class="fb-dialog__close" aria-label="Close CLI instructions"
                    ><Icon name="x" :size="18"
                /></DialogClose>
            </DialogContent>
        </DialogPortal>
    </DialogRoot>
</template>

<style scoped>
.cli-install-dialog {
    width: min(calc(100vw - 2rem), 41rem);
    max-height: calc(100svh - 2rem);
    overflow-y: auto;
    padding: 1.625rem;
}
.cli-install-dialog__eyebrow {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin: 0 0 1.5rem;
    color: var(--fb-text-subtle);
    font-size: 0.625rem;
    letter-spacing: 0.1em;
    text-transform: uppercase;
}
.cli-install-dialog__installer,
.cli-install-dialog__usage {
    margin-top: 1.25rem;
}
.cli-install-dialog__installer {
    padding: 1rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.875rem;
    background: var(--fb-surface);
}
.cli-install-dialog h2 {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    margin: 0 0 0.75rem;
    font-size: 0.75rem;
    font-weight: 500;
}
.cli-install-dialog h2 > span {
    padding: 0.25rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.375rem;
    background: var(--fb-selected-surface);
    color: var(--fb-accent-text);
    font: 0.625rem var(--fb-font-code);
}
.cli-install-dialog__unavailable {
    color: var(--fb-text-muted);
    font-size: 0.875rem;
}
.cli-install-dialog__platform-label {
    display: block;
    margin-bottom: 0.375rem;
    color: var(--fb-text-muted);
    font-size: 0.6875rem;
}
.cli-install-dialog select {
    width: 100%;
    margin-bottom: 1rem;
    padding: 0.625rem 0.75rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.5rem;
    background: var(--fb-page);
    color: var(--fb-text);
    font: 0.8125rem var(--fb-font-sans);
}
.cli-install-dialog__note {
    margin: 0.75rem 0 0;
    color: var(--fb-text-subtle);
    font-size: 0.6875rem;
    line-height: 1.7;
}
.cli-install-dialog dl {
    margin: 0;
    border: 1px solid var(--fb-border);
    border-radius: 0.875rem;
    background: var(--fb-surface);
}
.cli-install-dialog dl > div {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.875rem;
}
.cli-install-dialog dl > div + div {
    border-top: 1px solid var(--fb-border);
}
.cli-install-dialog dt {
    font-size: 0.6875rem;
    overflow-wrap: anywhere;
}
.cli-install-dialog dd {
    margin: 0;
    flex: none;
    color: var(--fb-text-muted);
    font-size: 0.625rem;
}
.cli-install-dialog footer {
    display: flex;
    justify-content: end;
    margin-top: 1.25rem;
    padding-top: 1rem;
    border-top: 1px solid var(--fb-border);
}
@media (max-width: 560px) {
    .cli-install-dialog {
        padding: 1.25rem;
    }
    .cli-install-dialog dl > div {
        align-items: start;
        flex-direction: column;
        gap: 0.375rem;
    }
}
</style>
