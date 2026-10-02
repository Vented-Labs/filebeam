<script setup lang="ts">
import { ref } from 'vue';
import FilePond from '../../ui/src/components/upload/FilePond.vue';
import FileQueue from '../../ui/src/components/upload/FileQueue.vue';
import NoteComposer from '../../ui/src/components/notes/NoteComposer.vue';
import ShareReady from '../../ui/src/components/sharing/ShareReady.vue';
import OgCard from '../../ui/src/components/brand/OgCard.vue';
const note = ref('const message = "Private draft";\nconst count = 42;');
const language = ref('javascript');
const title = ref('Gradient specimen');
const compact = new URLSearchParams(location.search).has('compact');
const queued = [
    {
        id: 'fixture',
        name: 'fixture.txt',
        file: new File(['synthetic'], 'fixture.txt'),
        type: 'text/plain',
        state: 'queued' as const,
        progress: 0,
    },
];
</script>

<template>
    <main class="fb-shell gradient-fixture">
        <header data-testid="gradient-ambient">
            <h1>Share files privately</h1>
            <p>End-to-end encrypted files and notes.</p>
        </header>
        <div class="gradient-fixture__frame">
            <FilePond :disabled="false" :dragging="false" :compact="compact">
                <FileQueue :entries="queued" :disabled="false" />
            </FilePond>
        </div>
        <div class="gradient-fixture__frame">
            <NoteComposer
                v-model="note"
                v-model:language="language"
                v-model:title="title"
                :disabled="false"
                :maximum-bytes="null"
                :retention-hours="24"
            />
        </div>
        <div class="gradient-fixture__frame">
            <ShareReady
                :share="{
                    link: 'https://share.invalid/fixture',
                    key: 'synthetic-key',
                    deleteToken: 'synthetic-delete',
                    transferId: 'synthetic',
                    includeKey: true,
                    passwordProtected: false,
                    driver: 'http',
                }"
                mode="files"
                :deleting="false"
            />
        </div>
        <div class="gradient-fixture__social"><OgCard variant="home" /></div>
    </main>
</template>

<style scoped>
.gradient-fixture {
    gap: 24px;
    padding-bottom: 24px;
}
.gradient-fixture header {
    height: 220px;
    padding: 32px;
    text-align: center;
}
.gradient-fixture h1 {
    font-size: 2rem;
    color: var(--fb-text);
}
.gradient-fixture header p {
    color: var(--fb-text-muted);
}
.gradient-fixture__frame {
    width: min(1080px, calc(100% - 32px));
    margin-inline: auto;
    background: var(--fb-surface);
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
}
.gradient-fixture__social {
    width: 1200px;
    max-width: 100%;
    margin-inline: auto;
    overflow: hidden;
}
@media (max-width: 600px) {
    .gradient-fixture__social {
        height: 205px;
    }
    .gradient-fixture__social :deep(.og-card) {
        transform: scale(0.325);
        transform-origin: top left;
    }
}
</style>
