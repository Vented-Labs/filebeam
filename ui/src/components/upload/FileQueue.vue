<script setup lang="ts">
import { nextTick, ref } from 'vue';
import type { UploadEntry } from '../../upload-types';
import { formatBytes } from '../../lib/format';
import Button from '../primitives/Button.vue';
import Icon, { type IconName } from '../primitives/Icon.vue';
import SmoothProgress from '../primitives/SmoothProgress.vue';
import AnimatedReveal from '../layout/AnimatedReveal.vue';
const props = defineProps<{ entries: UploadEntry[]; disabled: boolean }>();
const emit = defineEmits<{ choose: []; remove: [id: string] }>();
const queue = ref<HTMLElement>();
function iconFor(entry: UploadEntry): IconName {
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
async function remove(entry: UploadEntry): Promise<void> {
    const index = props.entries.indexOf(entry);
    const focusId = props.entries[index + 1]?.id ?? props.entries[index - 1]?.id;
    emit('remove', entry.id);
    await nextTick();
    const row = Array.from(
        queue.value?.querySelectorAll<HTMLElement>('[data-entry-id]') ?? [],
    ).find((element) => element.dataset.entryId === focusId);
    (
        row?.querySelector<HTMLButtonElement>('button') ??
        queue.value?.querySelector<HTMLButtonElement>('.file-queue__add') ??
        document.querySelector<HTMLButtonElement>('.file-pond__choose')
    )?.focus();
}
</script>

<template>
    <section ref="queue" class="file-queue" aria-labelledby="file-queue-title">
        <div class="flex items-center justify-between gap-3">
            <div>
                <h2 id="file-queue-title" class="font-semibold text-[var(--fb-text)]">
                    Your files
                </h2>
                <p class="mt-1 text-sm text-[var(--fb-text-muted)]">
                    {{ entries.length }} ready to protect
                </p>
            </div>
            <Button
                variant="secondary"
                class="file-queue__add"
                :disabled="disabled"
                @click="emit('choose')"
                ><Icon name="plus" :size="15" />Add more</Button
            >
        </div>
        <TransitionGroup name="queue" tag="ul" class="file-queue__list">
            <li
                v-for="entry in entries"
                :key="entry.id"
                class="file-queue__entry"
                data-testid="prism-file-row"
                :data-entry-id="entry.id"
            >
                <div class="flex min-w-0 items-center gap-3">
                    <span class="file-queue__thumb"
                        ><Icon
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
                        @click="remove(entry)"
                    >
                        <Icon name="x" :size="16" />
                    </button>
                </div>
                <div
                    class="file-queue__progress"
                    :class="{ 'file-queue__progress--idle': entry.state === 'queued' }"
                    :aria-hidden="entry.state === 'queued' || undefined"
                >
                    <SmoothProgress
                        :value="entry.progress"
                        :active="entry.state === 'encrypting' || entry.state === 'uploading'"
                        :label="`${entry.name} progress`"
                        size="small"
                    />
                </div>
                <AnimatedReveal :show="Boolean(entry.error)">
                    <p class="file-queue__error">{{ entry.error }}</p>
                </AnimatedReveal>
            </li>
        </TransitionGroup>
    </section>
</template>

<style scoped>
.file-queue {
    min-height: 18.4375rem;
    box-sizing: border-box;
    padding: 1.5rem;
    background: var(--fb-surface);
}
.file-queue h2 {
    font-size: 1.3125rem;
    font-weight: 550;
    letter-spacing: -0.025em;
}
.file-queue__list {
    position: relative;
    display: flex;
    max-height: 21.875rem;
    flex-direction: column;
    gap: 0.375rem;
    margin-top: 0.875rem;
    padding: 1px;
    overflow-y: auto;
    scrollbar-color: #53455f transparent;
    scrollbar-width: thin;
}
.file-queue__entry {
    position: relative;
    min-height: 3.8125rem;
    box-sizing: border-box;
    padding: 0.625rem 0.75rem;
    border: 1px solid #ffffff07;
    border-radius: 0.75rem;
    background: #110e1880;
    transition:
        background var(--fb-duration-control) ease,
        border-color var(--fb-duration-control) ease,
        transform var(--fb-duration-control) var(--fb-ease);
}
.file-queue__entry:hover {
    border-color: #6b527144;
    background: #252030;
}
.file-queue__progress {
    height: 0.25rem;
    margin-top: 0.625rem;
    opacity: 1;
    transition: opacity var(--fb-duration-control) ease;
}
.file-queue__progress--idle {
    opacity: 0.18;
}
.file-queue__error {
    margin: 0.5rem 0 0;
    color: var(--fb-danger);
    font-size: 0.75rem;
}
.file-queue__thumb {
    display: grid;
    width: 2.25rem;
    height: 2.25rem;
    flex: none;
    place-items: center;
    border: 1px solid #6651722a;
    border-radius: 0.5625rem;
    color: var(--fb-accent-text);
    background: #33263f;
}
.queue-enter-active,
.queue-leave-active {
    transition:
        opacity var(--fb-duration-control) ease,
        transform var(--fb-duration-control) var(--fb-ease);
}
.queue-move {
    transition: transform var(--fb-duration-pane) var(--fb-ease);
}
.queue-enter-from,
.queue-leave-to {
    opacity: 0;
    transform: translateX(18px);
}
.queue-leave-active {
    position: absolute;
    width: calc(100% - 2px);
}
@media (prefers-reduced-motion: reduce) {
    .queue-enter-active,
    .queue-leave-active {
        transition: none;
    }
}
</style>
