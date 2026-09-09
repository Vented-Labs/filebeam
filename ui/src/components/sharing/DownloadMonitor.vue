<script setup lang="ts">
import type { DownloadSession } from '../../upload-types';
import AnimatedHeight from '../layout/AnimatedHeight.vue';
import SmoothProgress from '../primitives/SmoothProgress.vue';

defineProps<{
    sessions: DownloadSession[];
    unavailable: boolean;
}>();

function statusLabel(session: DownloadSession): string {
    return {
        downloading: 'Downloading',
        waiting: 'Waiting to resume',
        verifying: 'Verifying',
        completed: 'Completed',
        cancelled: 'Cancelled',
        error: 'Could not finish',
        stale: 'Connection lost',
    }[session.status];
}

function selectionLabel(session: DownloadSession): string {
    if (session.all_files) return 'All files';
    return `${session.selection_count} file${session.selection_count === 1 ? '' : 's'}`;
}
</script>

<template>
    <section class="download-monitor" aria-labelledby="download-activity-heading">
        <div class="flex items-baseline justify-between gap-3">
            <h2 id="download-activity-heading" class="text-sm font-semibold text-[var(--fb-text)]">
                Download activity
            </h2>
            <span class="text-xs text-[var(--fb-text-muted)]">Anonymous</span>
        </div>
        <p class="mt-3 min-h-10 text-sm text-[var(--fb-text-muted)]" role="status">
            {{
                unavailable
                    ? 'Download activity is temporarily unavailable. Showing the last update.'
                    : !sessions.length
                      ? 'Download activity will appear here'
                      : 'Progress reported by each receiving browser.'
            }}
        </p>
        <AnimatedHeight v-if="sessions.length" class="mt-3">
            <TransitionGroup name="session" tag="ul" class="space-y-2">
                <li v-for="session in sessions" :key="session.id" class="download-monitor__row">
                    <SmoothProgress
                        :value="session.progress"
                        :active="
                            !unavailable &&
                            ['downloading', 'waiting', 'verifying'].includes(session.status)
                        "
                        :label="`Download ${session.number}`"
                        size="small"
                    >
                        <template #label="{ percentage }">
                            <div class="flex items-center justify-between gap-3 text-sm">
                                <span class="font-medium text-[var(--fb-text)]"
                                    >Download {{ session.number }}</span
                                >
                                <span class="tabular-nums text-[var(--fb-text-muted)]"
                                    >{{ percentage }}%</span
                                >
                            </div>
                            <div class="download-monitor__meta">
                                <span>{{ selectionLabel(session) }}</span
                                ><span>{{ statusLabel(session) }}</span>
                            </div>
                        </template>
                    </SmoothProgress>
                </li>
            </TransitionGroup>
        </AnimatedHeight>
    </section>
</template>

<style scoped>
.download-monitor {
    margin-top: 1.5rem;
    border-top: 1px solid var(--fb-border);
    padding-top: 1.25rem;
    text-align: left;
}
.download-monitor__row {
    overflow: hidden;
    max-height: 7rem;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-control);
    padding: 0.75rem;
    background: var(--fb-surface-raised);
}
.download-monitor__meta {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    margin-top: 0.2rem;
    margin-bottom: 0.625rem;
    color: var(--fb-text-muted);
    font-size: 0.75rem;
}
.session-enter-active,
.session-leave-active {
    transition:
        opacity 160ms ease,
        transform 160ms ease,
        max-height var(--fb-duration-switch) var(--fb-ease),
        padding-block var(--fb-duration-switch) var(--fb-ease);
}
.session-enter-from,
.session-leave-to {
    max-height: 0;
    padding-block: 0;
    opacity: 0;
    transform: translateY(4px);
}
@media (prefers-reduced-motion: reduce) {
    .session-enter-active,
    .session-leave-active {
        transition: none;
    }
}
</style>
