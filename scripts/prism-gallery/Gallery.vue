<script setup lang="ts">
import {
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
} from 'reka-ui';
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import type { CliConfig, TransferDriver, TransferLimits } from '../../ui/src/types';
import type { DownloadSession, ShareResult, UploadEntry } from '../../ui/src/upload-types';
import AnimatedHeight from '../../ui/src/components/layout/AnimatedHeight.vue';
import AnimatedReveal from '../../ui/src/components/layout/AnimatedReveal.vue';
import Button from '../../ui/src/components/primitives/Button.vue';
import CopyButton from '../../ui/src/components/primitives/CopyButton.vue';
import Icon from '../../ui/src/components/primitives/Icon.vue';
import Input from '../../ui/src/components/primitives/Input.vue';
import SmoothProgress from '../../ui/src/components/primitives/SmoothProgress.vue';
import Switch from '../../ui/src/components/primitives/Switch.vue';
import Toast from '../../ui/src/components/primitives/Toast.vue';
import Tooltip from '../../ui/src/components/primitives/Tooltip.vue';
import NoteComposer from '../../ui/src/components/notes/NoteComposer.vue';
import ShareReady from '../../ui/src/components/sharing/ShareReady.vue';
import WebRtcConsentDialog from '../../ui/src/components/sharing/WebRtcConsentDialog.vue';
import FilePond from '../../ui/src/components/upload/FilePond.vue';
import FileQueue from '../../ui/src/components/upload/FileQueue.vue';
import TransferMethod from '../../ui/src/components/upload/TransferMethod.vue';
import TransferModeTabs from '../../ui/src/components/upload/TransferModeTabs.vue';
import TransferOptions from '../../ui/src/components/upload/TransferOptions.vue';
import TransferPasswordPopover from '../../ui/src/components/upload/TransferPasswordPopover.vue';
import CliProvider from '../../ui/src/components/cli/CliProvider.vue';
import CliInstallButton from '../../ui/src/components/cli/CliInstallButton.vue';
import CliDownloadCard from '../../ui/src/components/cli/CliDownloadCard.vue';
import CliCommandField from '../../ui/src/components/cli/CliCommandField.vue';

type Section =
    | 'cli'
    | 'buttons'
    | 'inputs'
    | 'composer'
    | 'methods'
    | 'rows'
    | 'transfers'
    | 'notifications'
    | 'dialogs'
    | 'motion';

const sections: Array<{ id: Section; label: string; caption: string }> = [
    { id: 'cli', label: 'CLI', caption: 'Install, download, and command-copy states' },
    { id: 'buttons', label: 'Buttons', caption: 'Variants and interaction states' },
    { id: 'inputs', label: 'Inputs & password', caption: 'Fields, validation, and reveal' },
    { id: 'composer', label: 'Files & Notes', caption: 'Persistent production editors' },
    { id: 'methods', label: 'Methods & retention', caption: 'Policy, menus, and switches' },
    { id: 'rows', label: 'File rows', caption: 'Queue lifecycle and removal' },
    { id: 'transfers', label: 'Transfers', caption: 'Stored, Turbo, and live results' },
    { id: 'notifications', label: 'Copy & notifications', caption: 'Promise and toast lifecycle' },
    { id: 'dialogs', label: 'Dialogs', caption: 'Consent and confirmation focus' },
    { id: 'motion', label: 'Cards on a table', caption: 'Height, interruption, and motion' },
];

const activeSection = ref<Section>('buttons');
const slowMotion = ref(false);
const reducedMotion = ref(false);
const activity = ref('No fixture action yet.');
let motionPreference: MediaQueryList | undefined;
let interruptionTimers: number[] = [];

function syncMotionPreference(): void {
    reducedMotion.value = Boolean(motionPreference?.matches);
}

const slowMotionStyle = computed(() =>
    slowMotion.value
        ? {
              '--fb-duration-control': '450ms',
              '--fb-duration-switch': '750ms',
              '--fb-duration-selection': '950ms',
              '--fb-duration-menu-in': '700ms',
              '--fb-duration-menu-out': '500ms',
              '--fb-duration-pane': '1150ms',
              '--fb-duration-dialog-in': '900ms',
              '--fb-duration-dialog-out': '525ms',
              '--fb-duration-dialog-content': '1000ms',
              '--fb-duration-toast-in': '700ms',
              '--fb-duration-toast-out': '450ms',
          }
        : undefined,
);

function record(message: string): void {
    activity.value = message;
}

const inputValue = ref('Encrypted draft');
const emptyValue = ref('');
const invalidValue = ref('Needs attention');
const password = ref('');

function fixtureEntry(
    id: string,
    name: string,
    type: string,
    state: UploadEntry['state'],
    progress: number,
    error?: string,
): UploadEntry {
    const file = new File([`Local gallery fixture ${id}`], name, { type });
    return { id, file, name, type, state, progress, error };
}

const composerMode = ref<'files' | 'note'>('files');
const composerFiles = ref<UploadEntry[]>([]);
const note = ref('const encrypted = true;\n\nconsole.log("Local fixture only");');
const noteTitle = ref('Browser-only draft');
const noteLanguage = ref('javascript');

function addComposerFiles(files: FileList): void {
    const additions = Array.from(files).map((file, index) => ({
        id: `selected-${Date.now()}-${index}`,
        file,
        name: file.name,
        type: file.type,
        state: 'queued' as const,
        progress: 0,
    }));
    composerFiles.value = [...composerFiles.value, ...additions];
    record(`${additions.length} local file fixture${additions.length === 1 ? '' : 's'} added.`);
}

function addComposerFixture(): void {
    if (composerFiles.value.length) return;
    composerFiles.value = [
        fixtureEntry('composer-one', 'prism-notes.txt', 'text/plain', 'queued', 0),
    ];
    record('A tiny local file fixture was added.');
}

const methodDriver = ref<TransferDriver>('http');
const methodMode = ref<'files' | 'note'>('files');
const methodProfile = ref<'finite' | 'unlimited' | 'http-only' | 'webrtc-only' | 'unsupported'>(
    'finite',
);
const methodDisabled = ref(false);
const methodPassword = ref('');
const methodIncludeKey = ref(true);
const methodRetention = ref(24);
const methodBurn = ref(false);
const finiteLimits: TransferLimits = {
    maximum_transfer_bytes: 2 * 1024 * 1024 * 1024,
    maximum_file_count: 20,
    maximum_note_bytes: 1024 * 1024,
};
const liveLimits: TransferLimits = {
    maximum_transfer_bytes: null,
    maximum_file_count: null,
    maximum_note_bytes: null,
};
const methodDrivers = computed<TransferDriver[]>(() =>
    methodProfile.value === 'http-only'
        ? ['http']
        : methodProfile.value === 'webrtc-only'
          ? ['webrtc']
          : ['http', 'webrtc'],
);
const methodSupported = computed(() => methodProfile.value !== 'unsupported');
const methodLimits = computed(() =>
    methodProfile.value === 'unlimited' && methodDriver.value === 'webrtc'
        ? liveLimits
        : finiteLimits,
);

function selectMethodProfile(profile: typeof methodProfile.value): void {
    methodProfile.value = profile;
    if (profile === 'webrtc-only') methodDriver.value = 'webrtc';
    else if (
        profile === 'http-only' ||
        (profile === 'unsupported' && methodDriver.value === 'webrtc')
    )
        methodDriver.value = 'http';
}

const initialRows = () => [
    fixtureEntry('row-ready', 'ready-document.pdf', 'application/pdf', 'queued', 0),
    fixtureEntry('row-encrypting', 'encrypting-image.png', 'image/png', 'encrypting', 27),
    fixtureEntry('row-uploading', 'uploading-archive.zip', 'application/zip', 'uploading', 64),
    fixtureEntry('row-complete', 'complete-audio.mp3', 'audio/mpeg', 'complete', 100),
    fixtureEntry(
        'row-error',
        '<script>rendered-as-text.txt',
        'text/plain',
        'error',
        41,
        'The encrypted chunk could not be uploaded.',
    ),
];
const rows = ref<UploadEntry[]>(initialRows());

function rotateRows(): void {
    if (rows.value.length > 1) rows.value = [...rows.value.slice(1), rows.value[0]!];
    record('Queue fixture order changed.');
}

const transferVariant = ref<'stored' | 'turbo' | 'live' | 'error'>('stored');
const turboUploading = ref(true);
const futureExpiry = '2030-01-01T00:00:00.000Z';
const sessions: DownloadSession[] = [
    {
        id: 'gallery-recipient-one',
        number: 1,
        progress: 100,
        status: 'completed',
        selection_count: 3,
        all_files: true,
    },
    {
        id: 'gallery-recipient-two',
        number: 2,
        progress: 62,
        status: 'downloading',
        selection_count: 1,
        all_files: false,
    },
];
const transferShare = computed<ShareResult>(() => ({
    link: `https://share.invalid/${transferVariant.value}-fixture`,
    key: 'v1.GALLERY_ONLY_KEY_MATERIAL_000000000000000000',
    deleteToken: 'gallery-only-delete-token',
    expiresAt: futureExpiry,
    transferId: `gallery-${transferVariant.value}`,
    includeKey: true,
    passwordProtected: false,
    turbo: transferVariant.value === 'turbo',
    driver: transferVariant.value === 'live' ? 'webrtc' : 'http',
}));

const copyFailure = ref(false);
const copyHold = ref(false);
let releaseClipboard = (): void => undefined;
const installerPublished = ref(true);
const cliVariant = ref<'ready' | 'keyless' | 'expired' | 'unavailable' | 'live' | 'note'>('ready');
const cliConfig = computed<CliConfig>(() => ({
    installer_url: installerPublished.value
        ? 'https://releases.filebeam.test/cli/install.sh'
        : null,
    installer_interpreter: 'sh',
    executable: 'beam',
}));
const cliTarget = computed(
    () =>
        `https://filebeam.test/01K46FN13WJVCWKMBWRMC9Q9KN${cliVariant.value === 'keyless' ? '' : `#k=v1.${'A'.repeat(43)}`}`,
);
const longCommand = `beam down 'https://filebeam.test/${'long-fixture-'.repeat(100)}'`;
const toastOpen = ref(false);
let originalClipboard: PropertyDescriptor | undefined;
let clipboardInstalled = false;

function installClipboardFixture(): void {
    try {
        Object.defineProperty(navigator, 'clipboard', {
            configurable: true,
            value: {
                writeText: async () => {
                    if (copyHold.value)
                        await new Promise<void>((resolve) => {
                            releaseClipboard = resolve;
                        });
                    if (copyFailure.value) throw new Error('Gallery clipboard rejection');
                },
            },
        });
        clipboardInstalled = true;
    } catch {
        clipboardInstalled = false;
    }
}

function showToast(): void {
    toastOpen.value = false;
    requestAnimationFrame(() => {
        toastOpen.value = true;
    });
}

const consentOpen = ref(false);
const confirmationOpen = ref(false);
const confirmationStep = ref<'question' | 'complete'>('question');

function openConfirmation(): void {
    confirmationStep.value = 'question';
    confirmationOpen.value = true;
}

function answerConsent(answer: 'accepted' | 'declined'): void {
    consentOpen.value = false;
    record(`Consent fixture ${answer}; no cookie or peer was created.`);
}

function hideOutgoing(element: Element): void {
    const pane = element as HTMLElement;
    pane.inert = true;
    pane.setAttribute('aria-hidden', 'true');
}

const motionPane = ref(0);
const motionDetail = ref(false);

function interruptMotion(): void {
    for (const timer of interruptionTimers) window.clearTimeout(timer);
    interruptionTimers = [];
    for (let index = 0; index < 10; index++) {
        interruptionTimers.push(
            window.setTimeout(() => {
                motionPane.value = index % 3;
            }, index * 55),
        );
    }
    record('Ten rapid presentation changes started.');
}

watch(copyFailure, () => {
    if (clipboardInstalled) installClipboardFixture();
});

onMounted(() => {
    motionPreference = window.matchMedia('(prefers-reduced-motion: reduce)');
    syncMotionPreference();
    motionPreference.addEventListener('change', syncMotionPreference);
    originalClipboard = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
    installClipboardFixture();
});

onBeforeUnmount(() => {
    for (const timer of interruptionTimers) window.clearTimeout(timer);
    motionPreference?.removeEventListener('change', syncMotionPreference);
    if (clipboardInstalled) {
        if (originalClipboard) Object.defineProperty(navigator, 'clipboard', originalClipboard);
        else Reflect.deleteProperty(navigator, 'clipboard');
    }
});
</script>

<template>
    <CliProvider :config="cliConfig">
        <div class="prism-gallery" :style="slowMotionStyle">
            <header class="gallery-header">
                <div class="gallery-header__brand">
                    <img src="/brand/filebeam-logo-header.svg" alt="Filebeam" />
                    <span>Prism component gallery</span>
                </div>
                <div class="gallery-header__meta">
                    <span class="gallery-development-badge">Development only</span>
                    <label class="gallery-motion-toggle">
                        <span>{{ slowMotion ? '2.5x review motion' : 'Normal motion' }}</span>
                        <Switch v-model="slowMotion" aria-label="Slow review motion" />
                    </label>
                    <span class="gallery-reduced-status">
                        Reduced motion {{ reducedMotion ? 'on' : 'off' }}
                    </span>
                </div>
            </header>

            <div class="gallery-layout">
                <aside class="gallery-sidebar" aria-label="Gallery sections">
                    <p>Production components</p>
                    <button
                        v-for="item in sections"
                        :key="item.id"
                        type="button"
                        :class="{ 'gallery-sidebar__item--active': activeSection === item.id }"
                        :aria-current="activeSection === item.id ? 'page' : undefined"
                        @click="activeSection = item.id"
                    >
                        <strong>{{ item.label }}</strong>
                        <small>{{ item.caption }}</small>
                    </button>
                </aside>

                <main class="gallery-main">
                    <div class="gallery-intro">
                        <div>
                            <p>Prism 02 / shared implementation</p>
                            <h1>{{ sections.find((item) => item.id === activeSection)?.label }}</h1>
                        </div>
                        <p class="gallery-activity" role="status">{{ activity }}</p>
                    </div>

                    <section
                        v-if="activeSection === 'cli'"
                        class="gallery-section"
                        aria-labelledby="gallery-cli-title"
                    >
                        <div class="gallery-section__heading">
                            <h2 id="gallery-cli-title">CLI production components</h2>
                            <p>
                                Offline command specimens. Use pointer and keyboard for hover,
                                pressed, and focus states.
                            </p>
                        </div>
                        <div class="gallery-fixture-controls">
                            <CliInstallButton />
                            <label class="gallery-inline-switch"
                                >Published installer<Switch
                                    v-model="installerPublished"
                                    aria-label="Published installer"
                            /></label>
                            <label class="gallery-inline-switch"
                                >Reject clipboard<Switch
                                    v-model="copyFailure"
                                    aria-label="Reject CLI clipboard"
                            /></label>
                            <label class="gallery-inline-switch"
                                >Hold clipboard<Switch
                                    v-model="copyHold"
                                    aria-label="Hold CLI clipboard"
                            /></label>
                            <Button variant="secondary" @click="releaseClipboard()"
                                >Resolve clipboard</Button
                            >
                        </div>
                        <div class="gallery-fixture-controls">
                            <Button
                                v-for="variant in [
                                    'ready',
                                    'keyless',
                                    'expired',
                                    'unavailable',
                                    'live',
                                    'note',
                                ] as const"
                                :key="variant"
                                :variant="cliVariant === variant ? 'primary' : 'secondary'"
                                @click="cliVariant = variant"
                                >{{ variant }}</Button
                            >
                        </div>
                        <CliDownloadCard
                            :target="cliTarget"
                            :transfer="{
                                kind: cliVariant === 'note' ? 'note' : 'files',
                                driver: cliVariant === 'live' ? 'webrtc' : 'http',
                                available: cliVariant !== 'unavailable',
                                expiresAt:
                                    cliVariant === 'expired'
                                        ? '2000-01-01T00:00:00Z'
                                        : futureExpiry,
                            }"
                        />
                        <CliCommandField
                            :command="longCommand"
                            label="Long command specimen"
                            copy-label="Copy long specimen"
                        />
                    </section>
                    <section
                        v-else-if="activeSection === 'buttons'"
                        class="gallery-section"
                        aria-labelledby="gallery-buttons-title"
                    >
                        <div class="gallery-section__heading">
                            <h2 id="gallery-buttons-title">Button system</h2>
                            <p>
                                Use the real controls with pointer, keyboard, focus, and disabled
                                states.
                            </p>
                        </div>
                        <div class="gallery-specimen-grid">
                            <article class="gallery-specimen">
                                <span class="gallery-specimen__label">Variants</span>
                                <div class="gallery-button-row">
                                    <Button><Icon name="lock" :size="16" />Primary</Button>
                                    <Button variant="secondary">Secondary</Button>
                                    <Button variant="ghost">Ghost</Button>
                                    <Button variant="danger">Danger</Button>
                                    <Tooltip content="Named icon control">
                                        <Button icon aria-label="Download fixture">
                                            <Icon name="download" :size="17" />
                                        </Button>
                                    </Tooltip>
                                </div>
                            </article>
                            <article class="gallery-specimen">
                                <span class="gallery-specimen__label"
                                    >Persistent review states</span
                                >
                                <div class="gallery-state-grid">
                                    <span><small>Default</small><Button>Continue</Button></span>
                                    <span class="gallery-force-hover"
                                        ><small>Hover</small><Button>Continue</Button></span
                                    >
                                    <span class="gallery-force-press"
                                        ><small>Pressed</small><Button>Continue</Button></span
                                    >
                                    <span class="gallery-force-focus"
                                        ><small>Focus</small><Button>Continue</Button></span
                                    >
                                    <span
                                        ><small>Disabled</small
                                        ><Button disabled>Continue</Button></span
                                    >
                                    <span
                                        ><small>Busy</small
                                        ><Button disabled aria-busy="true"
                                            ><Icon name="loader" :size="16" />Encrypting</Button
                                        ></span
                                    >
                                </div>
                            </article>
                        </div>
                    </section>

                    <section
                        v-else-if="activeSection === 'inputs'"
                        class="gallery-section"
                        aria-labelledby="gallery-inputs-title"
                    >
                        <div class="gallery-section__heading">
                            <h2 id="gallery-inputs-title">Inputs and password</h2>
                            <p>Native input behavior and the production sender password popup.</p>
                        </div>
                        <div class="gallery-form-grid">
                            <label
                                >Empty<Input v-model="emptyValue" placeholder="Start typing"
                            /></label>
                            <label>Filled<Input v-model="inputValue" /></label>
                            <label class="gallery-force-input-focus"
                                >Focused<Input value="Keyboard focus"
                            /></label>
                            <label
                                >Invalid<Input
                                    v-model="invalidValue"
                                    :invalid="true"
                                    aria-describedby="gallery-invalid-help"
                                /><small id="gallery-invalid-help" class="gallery-error"
                                    >This value needs attention.</small
                                ></label
                            >
                            <label>Read only<Input value="Immutable value" readonly /></label>
                            <label>Disabled<Input value="Unavailable" disabled /></label>
                        </div>
                        <div class="gallery-password-card">
                            <label id="gallery-password-label"
                                >Password <span>optional</span></label
                            >
                            <TransferPasswordPopover
                                v-model="password"
                                labelled-by="gallery-password-label"
                            />
                            <p>The value remains local and is never echoed into the trigger.</p>
                        </div>
                    </section>

                    <section
                        v-else-if="activeSection === 'composer'"
                        class="gallery-section"
                        aria-labelledby="gallery-composer-title"
                    >
                        <div class="gallery-section__heading gallery-section__heading--split">
                            <div>
                                <h2 id="gallery-composer-title">Files and Notes</h2>
                                <p>
                                    Switch repeatedly; the picker, queue, editor, and draft stay
                                    owned.
                                </p>
                            </div>
                            <Button variant="secondary" @click="addComposerFixture"
                                >Add local fixture</Button
                            >
                        </div>
                        <TransferModeTabs v-model="composerMode" />
                        <div class="gallery-composer-frame">
                            <div
                                v-show="composerMode === 'files'"
                                :inert="composerMode !== 'files' || undefined"
                                :aria-hidden="composerMode !== 'files' || undefined"
                            >
                                <FilePond
                                    :disabled="false"
                                    :dragging="false"
                                    :compact="composerFiles.length > 0"
                                    @choose="record('The production file picker was requested.')"
                                    @files="addComposerFiles"
                                >
                                    <FileQueue
                                        :entries="composerFiles"
                                        :disabled="false"
                                        @choose="
                                            record('The production file picker was requested.')
                                        "
                                        @remove="
                                            composerFiles = composerFiles.filter(
                                                (entry) => entry.id !== $event,
                                            )
                                        "
                                    />
                                </FilePond>
                            </div>
                            <div
                                v-show="composerMode === 'note'"
                                :inert="composerMode !== 'note' || undefined"
                                :aria-hidden="composerMode !== 'note' || undefined"
                            >
                                <NoteComposer
                                    v-model="note"
                                    v-model:title="noteTitle"
                                    v-model:language="noteLanguage"
                                    :disabled="false"
                                    :maximum-bytes="1024 * 1024"
                                    :retention-hours="24"
                                />
                            </div>
                        </div>
                    </section>

                    <section
                        v-else-if="activeSection === 'methods'"
                        class="gallery-section"
                        aria-labelledby="gallery-methods-title"
                    >
                        <div class="gallery-section__heading">
                            <h2 id="gallery-methods-title">HTTP, WebRTC, retention, and sharing</h2>
                            <p>
                                All actions are intercepted locally; no transfer or consent can
                                start.
                            </p>
                        </div>
                        <div class="gallery-fixture-controls" aria-label="Method fixtures">
                            <Button
                                v-for="profile in [
                                    ['finite', 'Both finite'],
                                    ['unlimited', 'Live unlimited'],
                                    ['http-only', 'HTTP only'],
                                    ['webrtc-only', 'WebRTC only'],
                                    ['unsupported', 'Live unsupported'],
                                ] as const"
                                :key="profile[0]"
                                :variant="methodProfile === profile[0] ? 'primary' : 'secondary'"
                                :aria-pressed="methodProfile === profile[0]"
                                @click="selectMethodProfile(profile[0])"
                                >{{ profile[1] }}</Button
                            >
                            <Button
                                :variant="methodMode === 'files' ? 'primary' : 'secondary'"
                                @click="methodMode = 'files'"
                                >Files</Button
                            >
                            <Button
                                :variant="methodMode === 'note' ? 'primary' : 'secondary'"
                                @click="methodMode = 'note'"
                                >Notes</Button
                            >
                            <Button variant="ghost" @click="methodDisabled = !methodDisabled">
                                {{ methodDisabled ? 'Unlock controls' : 'Show busy controls' }}
                            </Button>
                        </div>
                        <div class="gallery-method-composer">
                            <TransferMethod
                                v-model="methodDriver"
                                :enabled-drivers="methodDrivers"
                                :web-rtc-supported="methodSupported"
                                :disabled="methodDisabled"
                                :limits="methodLimits"
                                :mode="methodMode"
                            />
                            <div class="gallery-method-placeholder">
                                <Icon
                                    :name="methodMode === 'files' ? 'folder' : 'note'"
                                    :size="28"
                                />
                                <strong>{{
                                    methodMode === 'files' ? 'File work region' : 'Note work region'
                                }}</strong>
                                <span>Presentation fixture; no uploader is mounted.</span>
                            </div>
                            <TransferOptions
                                v-model:password="methodPassword"
                                v-model:include-key="methodIncludeKey"
                                v-model:retention-hours="methodRetention"
                                v-model:burn-on-read="methodBurn"
                                v-model:driver="methodDriver"
                                :disabled="methodDisabled"
                                :can-upload="!methodDisabled"
                                :retention-options="[1, 6, 24, 72, 168, 720]"
                                :mode="methodMode"
                                :uploading="methodDisabled"
                                @submit="
                                    record('Encrypt and share was intercepted by the gallery.')
                                "
                                @turbo="record('Turbo Transfer was intercepted by the gallery.')"
                                @cancel="methodDisabled = false"
                            />
                        </div>
                    </section>

                    <section
                        v-else-if="activeSection === 'rows'"
                        class="gallery-section"
                        aria-labelledby="gallery-rows-title"
                    >
                        <div class="gallery-section__heading gallery-section__heading--split">
                            <div>
                                <h2 id="gallery-rows-title">File row lifecycle</h2>
                                <p>
                                    Ready, encrypting, uploading, complete, error, hostile text, and
                                    FLIP moves.
                                </p>
                            </div>
                            <div class="gallery-button-row">
                                <Button variant="secondary" @click="rotateRows">Rotate rows</Button>
                                <Button variant="ghost" @click="rows = initialRows()"
                                    >Restore fixtures</Button
                                >
                            </div>
                        </div>
                        <div class="gallery-queue-frame">
                            <FileQueue
                                :entries="rows"
                                :disabled="false"
                                @choose="record('Add more was intercepted by the gallery.')"
                                @remove="rows = rows.filter((entry) => entry.id !== $event)"
                            />
                        </div>
                    </section>

                    <section
                        v-else-if="activeSection === 'transfers'"
                        class="gallery-section"
                        aria-labelledby="gallery-transfers-title"
                    >
                        <div class="gallery-section__heading">
                            <h2 id="gallery-transfers-title">Transfer result presentations</h2>
                            <p>
                                Clearly artificial local props; no uploader, polling, peer, or API
                                owner exists here.
                            </p>
                        </div>
                        <div class="gallery-fixture-controls">
                            <Button
                                v-for="variant in ['stored', 'turbo', 'live', 'error'] as const"
                                :key="variant"
                                :variant="transferVariant === variant ? 'primary' : 'secondary'"
                                :aria-pressed="transferVariant === variant"
                                @click="transferVariant = variant"
                                >{{ variant }}</Button
                            >
                            <Button
                                v-if="transferVariant === 'turbo'"
                                variant="ghost"
                                @click="turboUploading = !turboUploading"
                                >{{
                                    turboUploading ? 'Finish upload' : 'Resume upload fixture'
                                }}</Button
                            >
                        </div>
                        <div class="gallery-result-frame">
                            <ShareReady
                                :share="transferShare"
                                mode="files"
                                :deleting="false"
                                :uploading="
                                    transferVariant === 'live' ||
                                    (transferVariant === 'turbo' && turboUploading)
                                "
                                :progress="62"
                                activity="Encrypting local fixture blocks"
                                :sessions="
                                    transferVariant === 'live' || transferVariant === 'turbo'
                                        ? sessions
                                        : []
                                "
                                :monitoring-unavailable="false"
                                :can-restart-http="true"
                                @reset="record('New transfer was intercepted by the gallery.')"
                                @delete="record('Delete was intercepted by the gallery.')"
                                @cancel="record('Stop or cancel was intercepted by the gallery.')"
                                @restart-http="
                                    record('HTTP restart was intercepted by the gallery.')
                                "
                            />
                            <AnimatedReveal :show="transferVariant === 'error'">
                                <p class="gallery-transfer-error" role="alert">
                                    The encrypted fixture could not be finalized. Its result remains
                                    available.
                                </p>
                            </AnimatedReveal>
                        </div>
                    </section>

                    <section
                        v-else-if="activeSection === 'notifications'"
                        class="gallery-section"
                        aria-labelledby="gallery-notifications-title"
                    >
                        <div class="gallery-section__heading">
                            <h2 id="gallery-notifications-title">
                                Copy, tooltip, and toast lifecycle
                            </h2>
                            <p>The gallery controls only the local clipboard Promise outcome.</p>
                        </div>
                        <div class="gallery-notification-card">
                            <label class="gallery-inline-switch">
                                <span>
                                    <strong>Reject clipboard Promise</strong>
                                    <small
                                        >Exercise recoverable copy failure without a secret
                                        value.</small
                                    >
                                </span>
                                <Switch
                                    v-model="copyFailure"
                                    aria-label="Reject clipboard promise"
                                />
                            </label>
                            <div class="gallery-button-row">
                                <CopyButton
                                    value="gallery-only-copy-value"
                                    label="Copy fixture value"
                                />
                                <CopyButton
                                    value="gallery-only-secondary-value"
                                    label="Copy secondary value"
                                    variant="secondary"
                                />
                                <Tooltip content="Production tooltip content" :delay="0">
                                    <Button variant="ghost">Focus or hover for tooltip</Button>
                                </Tooltip>
                                <Button @click="showToast">Show notification</Button>
                            </div>
                        </div>
                        <Toast
                            v-model:open="toastOpen"
                            title="Gallery notification"
                            description="Dismiss, repeat, or swipe this production toast."
                        />
                    </section>

                    <section
                        v-else-if="activeSection === 'dialogs'"
                        class="gallery-section"
                        aria-labelledby="gallery-dialogs-title"
                    >
                        <div class="gallery-section__heading">
                            <h2 id="gallery-dialogs-title">Dialog focus and content</h2>
                            <p>
                                The shared consent presentation is detached from cookie and
                                transport ownership.
                            </p>
                        </div>
                        <div class="gallery-button-row">
                            <Button @click="consentOpen = true">Open consent fixture</Button>
                            <Button variant="secondary" @click="openConfirmation">
                                Open confirmation fixture
                            </Button>
                        </div>
                        <WebRtcConsentDialog
                            :open="consentOpen"
                            @accept="answerConsent('accepted')"
                            @decline="answerConsent('declined')"
                        />
                        <DialogRoot v-model:open="confirmationOpen">
                            <DialogPortal>
                                <DialogOverlay class="fb-dialog__overlay" />
                                <DialogContent class="fb-dialog__content gallery-confirmation">
                                    <AnimatedHeight>
                                        <Transition
                                            name="gallery-dialog-pane"
                                            @before-leave="hideOutgoing"
                                        >
                                            <div
                                                :key="confirmationStep"
                                                class="gallery-dialog-pane"
                                            >
                                                <template v-if="confirmationStep === 'question'">
                                                    <DialogTitle class="fb-dialog__title">
                                                        Restart as stored HTTP?
                                                    </DialogTitle>
                                                    <DialogDescription
                                                        class="fb-dialog__description"
                                                    >
                                                        This development fixture demonstrates the
                                                        real confirmation material and focus trap
                                                        without stopping a live transfer.
                                                    </DialogDescription>
                                                    <div class="gallery-dialog-actions">
                                                        <Button
                                                            variant="ghost"
                                                            @click="confirmationOpen = false"
                                                        >
                                                            Cancel
                                                        </Button>
                                                        <Button
                                                            @click="confirmationStep = 'complete'"
                                                        >
                                                            Confirm fixture
                                                        </Button>
                                                    </div>
                                                </template>
                                                <template v-else>
                                                    <DialogTitle class="fb-dialog__title">
                                                        Confirmation received
                                                    </DialogTitle>
                                                    <DialogDescription
                                                        class="fb-dialog__description"
                                                    >
                                                        The same dialog surface changed height. No
                                                        network action ran.
                                                    </DialogDescription>
                                                    <div class="gallery-dialog-actions">
                                                        <Button @click="confirmationOpen = false"
                                                            >Done</Button
                                                        >
                                                    </div>
                                                </template>
                                            </div>
                                        </Transition>
                                    </AnimatedHeight>
                                </DialogContent>
                            </DialogPortal>
                        </DialogRoot>
                    </section>

                    <section v-else class="gallery-section" aria-labelledby="gallery-motion-title">
                        <div class="gallery-section__heading gallery-section__heading--split">
                            <div>
                                <h2 id="gallery-motion-title">Cards on a table</h2>
                                <p>
                                    Measured production height with overlapping, interruption-safe
                                    panes.
                                </p>
                            </div>
                            <Button variant="secondary" @click="interruptMotion"
                                >Interrupt 10 times</Button
                            >
                        </div>
                        <div class="gallery-fixture-controls" aria-label="Motion fixture panes">
                            <Button
                                v-for="(label, index) in ['Compact', 'Progress', 'Detail']"
                                :key="label"
                                :variant="motionPane === index ? 'primary' : 'secondary'"
                                @click="motionPane = index"
                                >{{ label }}</Button
                            >
                            <Button variant="ghost" @click="motionDetail = !motionDetail">
                                {{ motionDetail ? 'Hide inline detail' : 'Show inline detail' }}
                            </Button>
                        </div>
                        <div class="gallery-motion-stage">
                            <AnimatedHeight>
                                <Transition name="gallery-card" @before-leave="hideOutgoing">
                                    <article :key="motionPane" class="gallery-motion-card">
                                        <template v-if="motionPane === 0">
                                            <span class="gallery-motion-card__icon">
                                                <Icon name="lock" :size="24" />
                                            </span>
                                            <h3>Compact protected card</h3>
                                            <p>
                                                A short pane establishes the current measured
                                                height.
                                            </p>
                                        </template>
                                        <template v-else-if="motionPane === 1">
                                            <span class="gallery-motion-card__icon">
                                                <Icon name="bolt" :size="24" />
                                            </span>
                                            <h3>Encryption progress</h3>
                                            <p>
                                                The progress target is static and clearly belongs to
                                                this fixture.
                                            </p>
                                            <SmoothProgress
                                                :value="62"
                                                label="Gallery progress fixture"
                                            >
                                                <template #label="{ percentage }">
                                                    <div class="gallery-progress-label">
                                                        <span>Preparing local fixture</span>
                                                        <span>{{ percentage }}%</span>
                                                    </div>
                                                </template>
                                            </SmoothProgress>
                                        </template>
                                        <template v-else>
                                            <span class="gallery-motion-card__icon">
                                                <Icon name="note" :size="24" />
                                            </span>
                                            <h3>Intrinsic detail pane</h3>
                                            <p>
                                                This taller state exercises the same height helper
                                                used by transfer, auth, account, and dialog
                                                presentations.
                                            </p>
                                            <AnimatedReveal :show="motionDetail">
                                                <div class="gallery-motion-detail">
                                                    Conditional copy enters without snapping the
                                                    surrounding card.
                                                </div>
                                            </AnimatedReveal>
                                        </template>
                                    </article>
                                </Transition>
                            </AnimatedHeight>
                        </div>
                        <dl class="gallery-token-list">
                            <div>
                                <dt>Control</dt>
                                <dd>180ms</dd>
                            </div>
                            <div>
                                <dt>Switch</dt>
                                <dd>300ms</dd>
                            </div>
                            <div>
                                <dt>Selection</dt>
                                <dd>380ms</dd>
                            </div>
                            <div>
                                <dt>Pane</dt>
                                <dd>460ms</dd>
                            </div>
                            <div>
                                <dt>Easing</dt>
                                <dd>cubic-bezier(.22, 1, .36, 1)</dd>
                            </div>
                        </dl>
                    </section>
                </main>
            </div>
        </div>
    </CliProvider>
</template>
