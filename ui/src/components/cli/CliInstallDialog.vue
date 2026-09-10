<script setup lang="ts">
import {
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
    RadioGroupItem,
    RadioGroupRoot,
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
import Tooltip from '../primitives/Tooltip.vue';
import CliCommandField from './CliCommandField.vue';

const props = defineProps<{ config?: CliConfig }>();
const open = defineModel<boolean>('open', { default: false });
const emit = defineEmits<{ closeAutoFocus: [event: Event] }>();
const selectedPlatform = ref<CliPlatform>();
const dialogTitle = ref<HTMLElement>();
const platforms = [
    { value: 'linux', label: 'Linux' },
    { value: 'macos', label: 'macOS' },
    { value: 'windows', label: 'Windows' },
] as const;
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
function selectPlatformWithKeyboard(event: KeyboardEvent): void {
    if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
    const current = Math.max(
        0,
        platforms.findIndex((platform) => platform.value === selectedPlatform.value),
    );
    const forward = event.key === 'ArrowRight' || event.key === 'ArrowDown';
    const next = (current + (forward ? 1 : -1) + platforms.length) % platforms.length;
    selectedPlatform.value = platforms[next]!.value;
}
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
                @open-auto-focus.prevent="dialogTitle?.focus()"
                @close-auto-focus="emit('closeAutoFocus', $event)"
            >
                <p class="cli-install-dialog__eyebrow">
                    <Icon name="code" :size="16" />Filebeam / terminal
                </p>
                <DialogTitle as-child>
                    <h2 ref="dialogTitle" class="fb-dialog__title" tabindex="-1">Install CLI</h2>
                </DialogTitle>
                <DialogDescription class="fb-dialog__description"
                    >Use Filebeam from your terminal.</DialogDescription
                >
                <section class="cli-install-dialog__installer">
                    <h2><span>01</span>Choose your platform</h2>
                    <RadioGroupRoot
                        v-model="selectedPlatform"
                        class="cli-install-dialog__platforms"
                        orientation="horizontal"
                        aria-label="Platform"
                        @keydown="selectPlatformWithKeyboard"
                    >
                        <Tooltip
                            v-for="platform in platforms"
                            :key="platform.value"
                            :content="platform.label"
                            :delay="150"
                            inline
                            @escape-key-down="open = false"
                        >
                            <RadioGroupItem
                                :value="platform.value"
                                class="cli-install-dialog__platform"
                                :aria-label="platform.label"
                            >
                                <Icon :name="`os-${platform.value}`" :size="26" />
                            </RadioGroupItem>
                        </Tooltip>
                    </RadioGroupRoot>
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
.cli-install-dialog__installer h2,
.cli-install-dialog__usage h2 {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    margin: 0 0 0.75rem;
    font-size: 0.75rem;
    font-weight: 500;
}
.cli-install-dialog__installer h2 > span,
.cli-install-dialog__usage h2 > span {
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
.cli-install-dialog__platforms {
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
.cli-install-dialog__platform {
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
.cli-install-dialog__platform:hover {
    background: #ffffff06;
    color: var(--fb-text);
}
.cli-install-dialog__platform:active {
    transform: scale(0.97);
}
.cli-install-dialog__platform:focus-visible {
    outline: 2px solid var(--fb-focus);
    outline-offset: 2px;
}
.cli-install-dialog__platform[aria-checked='true'] {
    border-color: #806191;
    background: #32253f;
    color: #d4b3fa;
    box-shadow: inset 0 1px 0 #ffffff0a;
}
@media (prefers-reduced-motion: reduce) {
    .cli-install-dialog__platform {
        transition: none;
    }
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
