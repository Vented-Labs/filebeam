<script setup lang="ts">
import { ref } from 'vue';
import './editor-probe';
import Button from '../../ui/src/components/primitives/Button.vue';
import Checkbox from '../../ui/src/components/primitives/Checkbox.vue';
import Switch from '../../ui/src/components/primitives/Switch.vue';
import Input from '../../ui/src/components/primitives/Input.vue';
import SmoothProgress from '../../ui/src/components/primitives/SmoothProgress.vue';
import Tooltip from '../../ui/src/components/primitives/Tooltip.vue';
import Toast from '../../ui/src/components/primitives/Toast.vue';
import NoteComposer from '../../ui/src/components/notes/NoteComposer.vue';
import AppearancePopover from '../../ui/src/components/layout/AppearancePopover.vue';
import BrandLogo from '../../ui/src/components/brand/BrandLogo.vue';
import TransferMethod from '../../ui/src/components/upload/TransferMethod.vue';
import TransferPasswordPopover from '../../ui/src/components/upload/TransferPasswordPopover.vue';
import FileQueue from '../../ui/src/components/upload/FileQueue.vue';
import ShareReady from '../../ui/src/components/sharing/ShareReady.vue';
import type { TransferDriver } from '../../ui/src/types';
import type { UploadEntry } from '../../ui/src/upload-types';

const sample =
    '// A private draft\nclass Parcel {\n  send(size = 42) {\n    const ready = true;\n    return /safe/i.test("hello\\nworld") && ready ? { size, value: null } : false;\n  }\n}\nnew Parcel().send();';
const note = ref(new URLSearchParams(location.search).has('large') ? 'x'.repeat(250_001) : sample);
const language = ref('javascript');
const title = ref('Color contract');
const readOnly = ref(false);
const off = ref(false);
const on = ref(true);
const progress = ref(50);
const toast = ref(false);
const driver = ref<TransferDriver>('http');
const password = ref('');
const limits = {
    maximum_transfer_bytes: 2_147_483_648,
    maximum_file_count: 20,
    maximum_note_bytes: 1_048_576,
};
const entries: UploadEntry[] = [
    {
        id: 'partial',
        name: 'uploading.txt',
        file: new File(['synthetic'], 'uploading.txt'),
        type: 'text/plain',
        state: 'uploading',
        progress: 50,
    },
    {
        id: 'complete',
        name: 'completed.txt',
        file: new File(['synthetic'], 'completed.txt'),
        type: 'text/plain',
        state: 'complete',
        progress: 100,
    },
    {
        id: 'failure',
        name: 'failed.txt',
        file: new File(['synthetic'], 'failed.txt'),
        type: 'text/plain',
        state: 'error',
        progress: 0,
        error: 'The encrypted chunk could not be uploaded.',
    },
];
</script>

<template>
    <main class="theme-fixture fb-shell">
        <header>
            <BrandLogo />
            <h1>Theme specimens</h1>
            <AppearancePopover />
        </header>
        <section class="theme-fixture__panel" data-testid="theme-controls">
            <div class="theme-fixture__row">
                <Button data-testid="primary">Primary action</Button>
                <Button variant="secondary">Secondary action</Button>
                <Button variant="danger">Destructive action</Button>
                <Button disabled>Disabled action</Button>
                <Button aria-disabled="true">Unavailable action</Button>
            </div>
            <div class="theme-fixture__row">
                <label class="fb-check-label"><Checkbox v-model="off" />Unchecked</label>
                <label class="fb-check-label"><Checkbox v-model="on" />Checked</label>
                <label class="fb-check-label"><Switch v-model="off" />Off switch</label>
                <label class="fb-check-label"><Switch v-model="on" />On switch</label>
                <Tooltip content="Readable tooltip"
                    ><Button variant="secondary">Tooltip trigger</Button></Tooltip
                >
                <a href="#editor" class="fb-text-link">Inline text link</a>
                <Button variant="secondary" @click="toast = true">Show toast</Button>
            </div>
            <div class="theme-fixture__row">
                <Input placeholder="Readable placeholder" aria-label="Empty input" />
                <Input value="Invalid value" aria-invalid="true" aria-label="Invalid input" />
                <TransferPasswordPopover v-model="password" />
            </div>
            <p class="fb-field-error">Use at least 8 characters.</p>
            <SmoothProgress
                v-for="value in [0.4, 50, 100]"
                :key="value"
                :value="value"
                :label="`Progress ${value}`"
            >
                <template #label="{ percentage }"
                    ><span>{{ percentage }}%</span></template
                >
            </SmoothProgress>
            <div class="theme-fixture__row" data-testid="progress-controls">
                <Button
                    v-for="value in [0, 0.4, 1, 50, 100, -10, NaN]"
                    :key="String(value)"
                    variant="secondary"
                    @click="progress = value"
                    >{{ String(value) }}</Button
                >
            </div>
            <SmoothProgress :value="progress" label="Adjustable progress" />
        </section>
        <section id="editor" class="theme-fixture__panel">
            <label class="fb-check-label"
                ><Switch v-model="readOnly" aria-label="Read only" />Read only</label
            >
            <NoteComposer
                v-model="note"
                v-model:language="language"
                v-model:title="title"
                :disabled="readOnly"
                :maximum-bytes="null"
                :retention-hours="24"
            />
        </section>
        <section class="theme-fixture__panel" data-testid="theme-transfer">
            <TransferMethod
                v-model="driver"
                :enabled-drivers="['http', 'webrtc']"
                :web-rtc-supported="true"
                :disabled="false"
                :limits="limits"
                mode="files"
            />
            <FileQueue :entries="entries" :disabled="false" />
        </section>
        <section class="theme-fixture__panel" data-testid="theme-share">
            <ShareReady
                :share="{
                    link: 'https://share.invalid/synthetic',
                    key: 'synthetic-key',
                    deleteToken: 'synthetic-delete',
                    transferId: 'synthetic',
                    includeKey: true,
                    passwordProtected: false,
                    driver: 'http',
                    turbo: true,
                }"
                mode="files"
                :deleting="false"
                :uploading="true"
                :progress="50"
            />
        </section>
        <Toast
            v-model:open="toast"
            title="Transfer completed"
            description="Your encrypted transfer is ready."
        />
    </main>
</template>

<style scoped>
.theme-fixture {
    max-width: 1080px;
    margin: auto;
    padding: 24px;
    gap: 24px;
}
.theme-fixture header,
.theme-fixture__row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 16px;
}
.theme-fixture header h1 {
    flex: 1;
}
.theme-fixture__panel {
    padding: 20px;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
}
.theme-fixture__row {
    margin-bottom: 16px;
}
.theme-fixture__row > .fb-input {
    flex: 1;
    min-width: 120px;
}
.smooth-progress {
    margin-block: 16px;
}
@media (max-width: 480px) {
    .theme-fixture {
        padding: 12px;
    }
    .theme-fixture__panel {
        padding: 12px;
    }
}
</style>
