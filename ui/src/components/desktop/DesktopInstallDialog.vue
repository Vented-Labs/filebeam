<script setup lang="ts">
import {
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
    SelectContent,
    SelectItem,
    SelectItemIndicator,
    SelectItemText,
    SelectPortal,
    SelectRoot,
    SelectTrigger,
    SelectValue,
    SelectViewport,
} from 'reka-ui';
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import type { DesktopArchitecture, DesktopRelease } from '../../types';
import { detectDesktopArchitecture, loadDesktopRelease } from '../../lib/desktop-install';
import {
    browserPlatformDetails,
    detectDesktopPlatform,
    type DesktopPlatform,
} from '../../lib/platform';
import { formatBytes } from '../../lib/format';
import Button from '../primitives/Button.vue';
import Icon from '../primitives/Icon.vue';
import FormField from '../primitives/FormField.vue';
import PlatformSelect from '../primitives/PlatformSelect.vue';

const open = defineModel<boolean>('open', { default: false });
const emit = defineEmits<{ closeAutoFocus: [event: Event] }>();
const platform = ref<DesktopPlatform>();
const architecture = ref<DesktopArchitecture | ''>('');
const release = ref<DesktopRelease>();
const state = ref<'loading' | 'available' | 'unavailable' | 'error'>('loading');
let request: AbortController | undefined;
let fetchedAt = 0;
const platforms = [
    { value: 'windows', label: 'Windows' },
    { value: 'macos', label: 'macOS' },
    { value: 'linux', label: 'Linux' },
] as const;
const platformLabel = computed(
    () => platforms.find((item) => item.value === platform.value)?.label,
);
const installer = computed(() =>
    release.value?.assets.find(
        (asset) => asset.os === platform.value && asset.architecture === architecture.value,
    ),
);
const architectures = computed(() => [
    { value: 'x86_64', label: platform.value === 'macos' ? 'Intel' : 'x86-64 (Intel / AMD)' },
    { value: 'aarch64', label: platform.value === 'macos' ? 'Apple Silicon' : 'ARM64' },
]);
const instructions = computed(() => {
    if (platform.value === 'windows')
        return [
            'Download the Windows installer.',
            'Run the downloaded setup file and follow the installation steps.',
            'If Windows SmartScreen shows an unrecognized-app prompt, select More info → Run anyway.',
            'Open Filebeam from the Start menu.',
        ];
    if (platform.value === 'macos')
        return [
            'Download and open the disk image (.dmg).',
            'Drag Filebeam into your Applications folder.',
            'Open Filebeam from Applications.',
            'If macOS blocks the first launch, use System Settings → Privacy & Security → Open Anyway.',
        ];
    return [
        'Download the AppImage to a folder you want to keep.',
        'In the file’s properties, allow it to run as a program.',
        'Open the AppImage to launch Filebeam.',
    ];
});

watch(platform, () => {
    architecture.value = '';
});
onMounted(async () => {
    const detected = detectDesktopPlatform(browserPlatformDetails());
    platform.value = detected;
    const detectedArchitecture = await detectDesktopArchitecture();
    if (detectedArchitecture && platform.value === detected && !architecture.value) {
        architecture.value = detectedArchitecture;
    }
});

async function load(): Promise<void> {
    request?.abort();
    const controller = new AbortController();
    request = controller;
    const timeout = setTimeout(() => controller.abort(), 15_000);
    state.value = 'loading';
    release.value = undefined;
    try {
        const result = await loadDesktopRelease(controller.signal);
        if (request !== controller) return;
        release.value = result.release ?? undefined;
        state.value = result.state;
        fetchedAt = Date.now();
    } catch {
        if (request === controller) state.value = 'error';
    } finally {
        clearTimeout(timeout);
    }
}
watch(
    open,
    (value) => {
        if (value && (Date.now() - fetchedAt > 300_000 || state.value === 'error')) void load();
    },
    { immediate: true },
);
onBeforeUnmount(() => {
    request?.abort();
    request = undefined;
});

function selectArchitecture(value: unknown): void {
    if (value === 'x86_64' || value === 'aarch64') architecture.value = value;
}
function focusTitle(event: Event): void {
    event.preventDefault();
    const heading = (event.target as HTMLElement).querySelector<HTMLElement>('.fb-dialog__title');
    void nextTick(() => heading?.focus());
}
</script>

<template>
    <DialogRoot v-model:open="open">
        <DialogPortal>
            <DialogOverlay class="fb-dialog__overlay" />
            <DialogContent
                class="fb-dialog__content desktop-install-dialog"
                @open-auto-focus="focusTitle"
                @close-auto-focus="emit('closeAutoFocus', $event)"
            >
                <p class="desktop-install-dialog__eyebrow">
                    <Icon name="monitor" :size="16" />Filebeam / desktop
                </p>
                <DialogTitle as-child>
                    <h2 class="fb-dialog__title" tabindex="-1">Install Desktop App</h2>
                </DialogTitle>
                <DialogDescription class="fb-dialog__description"
                    >Bring encrypted file sharing to your desktop.</DialogDescription
                >

                <section class="desktop-install-dialog__panel" aria-label="Desktop download">
                    <h3><span>01</span>Choose your platform</h3>
                    <PlatformSelect v-model="platform" @escape-key-down="open = false" />
                    <h3><span>02</span>Download the app</h3>
                    <FormField v-if="platform" id="desktop-processor" label="Processor">
                        <template #default="{ id }">
                            <SelectRoot
                                :model-value="architecture || undefined"
                                @update:model-value="selectArchitecture"
                            >
                                <SelectTrigger :id="id" class="fb-select-trigger">
                                    <SelectValue placeholder="Choose your processor" />
                                    <Icon name="chevron-down" :size="16" />
                                </SelectTrigger>
                                <SelectPortal>
                                    <SelectContent
                                        position="popper"
                                        :body-lock="false"
                                        class="fb-select-content"
                                        :side-offset="6"
                                    >
                                        <SelectViewport>
                                            <SelectItem
                                                v-for="item in architectures"
                                                :key="item.value"
                                                :value="item.value"
                                                class="fb-select-item"
                                            >
                                                <SelectItemText>{{ item.label }}</SelectItemText>
                                                <SelectItemIndicator
                                                    ><Icon name="check" :size="15"
                                                /></SelectItemIndicator>
                                            </SelectItem>
                                        </SelectViewport>
                                    </SelectContent>
                                </SelectPortal>
                            </SelectRoot>
                        </template>
                    </FormField>
                    <p v-if="platform === 'macos'" class="desktop-install-dialog__hint">
                        Find your chip or processor in Apple menu → About This Mac.
                    </p>
                    <p v-else-if="platform === 'linux'" class="desktop-install-dialog__hint">
                        Not sure? Run <code>uname -m</code>: x86_64 means x86-64; aarch64 means
                        ARM64.
                    </p>

                    <div
                        class="desktop-install-dialog__download"
                        aria-live="polite"
                        :aria-busy="state === 'loading'"
                    >
                        <p v-if="state === 'loading'" role="status">
                            <Icon name="loader" :size="17" />Checking available downloads…
                        </p>
                        <template v-else-if="state === 'error'">
                            <p role="status">
                                Desktop downloads could not be loaded. Please try again shortly.
                            </p>
                            <Button variant="secondary" @click="load">Try again</Button>
                        </template>
                        <template v-else-if="state === 'unavailable'">
                            <p role="status">
                                Desktop installers have not been published yet. Check back after the
                                next desktop release.
                            </p>
                            <Button variant="secondary" @click="load">Check again</Button>
                        </template>
                        <template v-else>
                            <p v-if="!platform" role="status">
                                Choose a desktop platform to view its download.
                            </p>
                            <p v-else-if="!architecture" role="status">
                                Choose your processor to get the right installer.
                            </p>
                            <p v-else-if="!installer" role="status">
                                An installer for this platform and processor is not available in
                                this release.
                            </p>
                            <template v-else>
                                <p class="desktop-install-dialog__details">
                                    Version {{ release?.version }} · {{ installer.format }} ·
                                    {{ formatBytes(installer.size) }}
                                </p>
                                <Button
                                    as="a"
                                    :href="installer.url"
                                    target="_blank"
                                    rel="noopener noreferrer"
                                    class="desktop-install-dialog__download-button"
                                >
                                    <Icon name="download" :size="18" />Download for
                                    {{ platformLabel }}
                                </Button>
                            </template>
                        </template>
                    </div>
                </section>

                <section
                    v-if="installer && state === 'available'"
                    class="desktop-install-dialog__instructions"
                >
                    <h3><span>03</span>Install and launch</h3>
                    <ol>
                        <li v-for="step in instructions" :key="step">{{ step }}</li>
                    </ol>
                </section>
                <footer>
                    <a
                        v-if="release"
                        :href="release.notes_url"
                        target="_blank"
                        rel="noopener noreferrer"
                        >Release notes <Icon name="arrow-up-right" :size="14"
                    /></a>
                    <DialogClose as-child
                        ><Button>Done<Icon name="check" :size="16" /></Button
                    ></DialogClose>
                </footer>
                <DialogClose class="fb-dialog__close" aria-label="Close desktop installation"
                    ><Icon name="x" :size="18"
                /></DialogClose>
            </DialogContent>
        </DialogPortal>
    </DialogRoot>
</template>

<style scoped>
.desktop-install-dialog {
    width: min(calc(100vw - 2rem), 41rem);
    max-height: calc(100svh - 2rem);
    overflow-y: auto;
    padding: 1.625rem;
}
.desktop-install-dialog__eyebrow {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0 0 1.5rem;
    color: var(--fb-text-subtle);
    font-size: 0.625rem;
    letter-spacing: 0.1em;
    text-transform: uppercase;
}
.desktop-install-dialog__panel {
    margin-top: 1.25rem;
    padding: 1rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.875rem;
    background: var(--fb-surface);
}
.desktop-install-dialog h3 {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    margin: 0 0 0.75rem;
    font-size: 0.75rem;
    font-weight: 500;
}
.desktop-install-dialog h3 > span {
    padding: 0.25rem;
    border: 1px solid var(--fb-border);
    border-radius: 0.375rem;
    background: var(--fb-selected-surface);
    color: var(--fb-accent-text);
    font: 0.625rem var(--fb-font-code);
}
.desktop-install-dialog__hint {
    margin: 0.625rem 0 0;
    color: var(--fb-text-subtle);
    font-size: 0.6875rem;
    line-height: 1.6;
}
.desktop-install-dialog__download {
    margin-top: 1rem;
    font-size: 0.8125rem;
    color: var(--fb-text-muted);
    line-height: 1.6;
}
.desktop-install-dialog__download p {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0 0 0.75rem;
}
.desktop-install-dialog__download-button {
    width: 100%;
}
.desktop-install-dialog__instructions {
    margin-top: 1.25rem;
}
.desktop-install-dialog__instructions ol {
    margin: 0;
    padding: 0;
    list-style-position: inside;
    border: 1px solid var(--fb-border);
    border-radius: 0.875rem;
    background: var(--fb-surface);
    color: var(--fb-text-muted);
    font-size: 0.75rem;
    line-height: 1.8;
}
.desktop-install-dialog__instructions li {
    padding: 0.875rem;
}
.desktop-install-dialog__instructions li + li {
    border-top: 1px solid var(--fb-border);
}
.desktop-install-dialog footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 1rem;
    margin-top: 1.25rem;
    padding-top: 1rem;
    border-top: 1px solid var(--fb-border);
}
.desktop-install-dialog footer a {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    margin-right: auto;
    color: var(--fb-accent-text);
    font-size: 0.75rem;
}
@media (max-width: 560px) {
    .desktop-install-dialog {
        padding: 1.25rem;
    }
}
</style>
