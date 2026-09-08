<script setup lang="ts">
import type { UploadEntry } from '../../upload-types';
import { formatBytes } from '../../lib/format';
import Button from '../primitives/Button.vue';
import FilebeamIcon, { type FilebeamIconName } from '../primitives/FilebeamIcon.vue';
import SmoothProgress from '../primitives/SmoothProgress.vue';
defineProps<{ entries: UploadEntry[]; disabled: boolean }>();
const emit = defineEmits<{ choose: []; remove: [id: string] }>();
function iconFor(entry: UploadEntry): FilebeamIconName {
    if (entry.type.startsWith('image/')) return 'image';
    if (entry.type.startsWith('video/')) return 'video';
    if (entry.type.startsWith('audio/')) return 'music';
    if (entry.type.includes('zip') || entry.type.includes('archive')) return 'archive';
    if (
        entry.type.includes('json') ||
        entry.type.includes('javascript') ||
        entry.type.includes('text')
    )
        return 'code';
    return 'file';
}
</script>

<template>
    <aside
        class="file-queue rounded-[var(--queue-radius)] border border-[var(--fb-border)] bg-[var(--fb-surface-raised)] p-5 shadow-2xl shadow-black/20"
    >
        <div class="flex items-center justify-between gap-3">
            <div>
                <h2 class="font-semibold text-[var(--fb-text)]">Your files</h2>
                <p class="mt-1 text-sm text-[var(--fb-text-muted)]">
                    {{ entries.length }} ready to protect
                </p>
            </div>
            <Button variant="ghost" :disabled="disabled" @click="emit('choose')">Add more</Button>
        </div>
        <TransitionGroup
            name="queue"
            tag="ul"
            class="mt-5 max-h-[24rem] space-y-2 overflow-y-auto pr-1"
        >
            <li
                v-for="entry in entries"
                :key="entry.id"
                class="file-queue__entry rounded-[var(--queue-entry-radius)] border border-[var(--fb-border)] bg-[var(--fb-bg)] px-3 py-3"
            >
                <div class="flex min-w-0 items-center gap-3">
                    <span
                        class="grid size-10 shrink-0 place-items-center rounded-xl border border-[var(--fb-border)] bg-[var(--fb-selected-surface)] text-[var(--fb-accent-text)]"
                        ><FilebeamIcon
                            :name="entry.state === 'complete' ? 'check' : iconFor(entry)"
                            :size="21"
                    /></span>
                    <div class="min-w-0 flex-1">
                        <p
                            class="truncate text-sm font-medium text-[var(--fb-text)]"
                            :title="entry.name"
                        >
                            {{ entry.name }}
                        </p>
                        <p class="mt-0.5 text-xs text-[var(--fb-text-muted)]">
                            {{ formatBytes(entry.file.size) }}
                            <span v-if="entry.state !== 'queued'">&middot; {{ entry.state }}</span>
                        </p>
                    </div>
                    <button
                        class="rounded-lg p-2 text-[var(--fb-text-muted)] hover:bg-[var(--fb-selected-surface)] hover:text-[var(--fb-text)] disabled:cursor-not-allowed disabled:opacity-40"
                        :disabled="disabled"
                        :aria-label="`Remove ${entry.name}`"
                        @click="emit('remove', entry.id)"
                    >
                        <FilebeamIcon name="x" :size="16" />
                    </button>
                </div>
                <SmoothProgress
                    v-if="
                        entry.state === 'encrypting' ||
                        entry.state === 'uploading' ||
                        entry.state === 'complete'
                    "
                    class="mt-3"
                    :value="entry.progress"
                    :active="entry.state === 'encrypting' || entry.state === 'uploading'"
                    :label="`${entry.name} progress`"
                    size="small"
                />
                <p v-if="entry.error" class="mt-2 text-xs text-[var(--fb-danger)]">
                    {{ entry.error }}
                </p>
            </li>
        </TransitionGroup>
    </aside>
</template>

<style scoped>
.file-queue {
    --queue-radius: 1.75rem;
    --queue-inset: 1.25rem;
    --queue-entry-radius: max(0px, calc(var(--queue-radius) - var(--queue-inset) - 1px));
}
.queue-enter-active,
.queue-leave-active {
    transition:
        opacity 0.18s ease,
        transform 0.18s ease;
}
.queue-enter-from,
.queue-leave-to {
    opacity: 0;
    transform: translateY(6px);
}
@media (prefers-reduced-motion: reduce) {
    .queue-enter-active,
    .queue-leave-active {
        transition: none;
    }
}
</style>
