<script setup lang="ts">
import { router } from '@inertiajs/vue3';
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import AppLink from '../primitives/AppLink.vue';
import Icon from '../primitives/Icon.vue';
import Toast from '../primitives/Toast.vue';
import ReleaseEntry from './ReleaseEntry.vue';
import { rememberIndex, rememberReaderOpenedFromIndex } from './changelog-history';
import { useBranding } from '../../lib/branding';
import type {
    ChangelogArchive,
    ChangelogFilters,
    ChangelogPage,
    ChangelogVersion,
} from '../../types/changelog';

const props = defineProps<{
    releases: ChangelogPage;
    filters: ChangelogFilters;
    versions: ChangelogVersion[];
    hasPrereleases: boolean;
    archive: ChangelogArchive;
    sourceUrl: string;
}>();

const branding = useBranding();
const term = ref(props.filters.q);
const search = ref<HTMLInputElement | null>(null);
const jump = ref('');
const announcement = ref('');
const toast = ref({ open: false, title: '' });
let searchTimer: number | undefined;

const query = computed(() => props.filters.q);
const checkedAt = computed(() =>
    props.archive.checked_at
        ? new Intl.DateTimeFormat('en-US', { dateStyle: 'medium', timeStyle: 'short' }).format(
              new Date(props.archive.checked_at),
          )
        : null,
);

function parameters(overrides: Record<string, string | number | undefined> = {}) {
    const values: Record<string, string | number | undefined> = {
        q: term.value.trim() || undefined,
        prereleases: props.filters.prereleases ? '1' : undefined,
        ...overrides,
    };
    return Object.fromEntries(Object.entries(values).filter(([, value]) => value !== undefined));
}

function runSearch(): void {
    window.clearTimeout(searchTimer);
    router.get('/changelog', parameters(), {
        only: ['releases', 'filters'],
        preserveState: true,
        preserveScroll: true,
        replace: true,
        onSuccess: () => {
            rememberIndex();
            if (props.filters.q) {
                const count = props.releases.total;
                announcement.value = `${count} matching ${count === 1 ? 'release' : 'releases'}.`;
            }
        },
    });
}

watch(term, (value) => {
    window.clearTimeout(searchTimer);
    if (value.trim() === props.filters.q) return;
    searchTimer = window.setTimeout(runSearch, 250);
});

function clearSearch(): void {
    term.value = '';
    runSearch();
    search.value?.focus();
}

function togglePrereleases(event: Event): void {
    const checked = (event.target as HTMLInputElement).checked;
    router.get('/changelog', parameters({ prereleases: checked ? '1' : undefined }), {
        preserveScroll: true,
        replace: true,
        onSuccess: rememberIndex,
    });
}

function focusRelease(tag: string): void {
    const link = Array.from(
        document.querySelectorAll<HTMLElement>('.fb-changelog [data-release-tag]'),
    ).find((element) => element.dataset.releaseTag === tag);
    if (!link) return;
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    link.closest('.cl-row')?.scrollIntoView({
        behavior: reduced ? 'auto' : 'smooth',
        block: 'start',
    });
    link.focus({ preventScroll: true });
}

function jumpTo(): void {
    const target = props.versions.find((version) => version.tag === jump.value);
    jump.value = '';
    if (!target) return;
    if (!props.filters.q && target.page === props.releases.current_page) {
        focusRelease(target.tag);
        return;
    }
    term.value = '';
    router.get(
        '/changelog',
        parameters({ q: undefined, page: target.page > 1 ? target.page : undefined }),
        {
            onSuccess: () => {
                rememberIndex();
                void nextTick(() => focusRelease(target.tag));
            },
        },
    );
}

function onKeydown(event: KeyboardEvent): void {
    if (event.key !== '/' || event.metaKey || event.ctrlKey || event.altKey) return;
    const active = document.activeElement as HTMLElement | null;
    if (active?.closest('input, textarea, select, [contenteditable], [role="dialog"]')) return;
    event.preventDefault();
    search.value?.focus();
}

function onSearchKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && term.value) {
        event.preventDefault();
        clearSearch();
    }
}

function onListClick(event: MouseEvent): void {
    const link = (event.target as HTMLElement).closest<HTMLAnchorElement>(
        'a.cl-version, .cl-title a',
    );
    if (link) rememberReaderOpenedFromIndex(link.href);
}

function copied(message: string): void {
    toast.value = { open: false, title: message };
    void nextTick(() => (toast.value.open = true));
}

onMounted(() => {
    rememberIndex();
    document.addEventListener('keydown', onKeydown);
});
onBeforeUnmount(() => {
    window.clearTimeout(searchTimer);
    document.removeEventListener('keydown', onKeydown);
});
</script>

<template>
    <div class="fb-changelog">
        <section class="cl-heading" aria-labelledby="changelog-title">
            <div>
                <h1 id="changelog-title" tabindex="-1">Changelog<span>.</span></h1>
                <p>Release notes for {{ branding.name }}.</p>
            </div>
            <a class="cl-source-link" :href="sourceUrl" target="_blank" rel="noopener noreferrer"
                ><Icon name="social-github" :size="15" /> GitHub Releases
                <Icon class="cl-external" name="arrow-up-right" :size="13"
            /></a>
        </section>

        <p v-if="archive.stale" class="cl-notice" role="status">
            <Icon name="clock" :size="15" />
            <span
                >Showing saved release notes.<template v-if="checkedAt">
                    Last checked {{ checkedAt }}.</template
                ></span
            >
        </p>

        <div v-if="archive.state === 'ready'" class="cl-toolbar">
            <div class="cl-toolbar__label">
                <Icon name="clock" :size="17" /><span>All releases</span>
            </div>
            <div class="cl-toolbar__controls">
                <label v-if="hasPrereleases" class="cl-toggle"
                    ><input
                        type="checkbox"
                        :checked="filters.prereleases"
                        @change="togglePrereleases"
                    />Include pre-releases</label
                >
                <div class="cl-jump">
                    <label for="changelog-jump" class="cl-sr-only">Jump to version</label>
                    <select id="changelog-jump" v-model="jump" @change="jumpTo">
                        <option value="">Jump to version</option>
                        <option v-for="version in versions" :key="version.tag" :value="version.tag">
                            {{ version.tag }}{{ version.prerelease ? ' · pre-release' : '' }}
                        </option>
                    </select>
                    <Icon name="chevron-down" :size="12" />
                </div>
                <div class="cl-search">
                    <Icon name="search" :size="16" />
                    <label class="cl-sr-only" for="changelog-search">Search releases</label>
                    <input
                        id="changelog-search"
                        ref="search"
                        v-model="term"
                        type="search"
                        placeholder="Search releases"
                        autocomplete="off"
                        spellcheck="false"
                        maxlength="120"
                        @keydown="onSearchKeydown"
                    />
                    <button
                        v-if="term"
                        type="button"
                        class="cl-search__clear"
                        aria-label="Clear search"
                        @click="clearSearch"
                    >
                        <Icon name="x" :size="14" />
                    </button>
                    <kbd v-else aria-hidden="true">/</kbd>
                </div>
            </div>
        </div>

        <p v-if="query && releases.total" class="cl-feedback">
            {{ releases.total }} {{ releases.total === 1 ? 'release' : 'releases' }} matching “{{
                query
            }}”
        </p>

        <div class="cl-list" @click="onListClick">
            <ReleaseEntry
                v-for="release in releases.data"
                :key="release.id"
                :release="release"
                :query="query"
                @copied="copied"
            />
        </div>

        <div v-if="!releases.data.length" class="cl-empty">
            <div class="cl-empty__mark" aria-hidden="true">
                <Icon :name="query ? 'search' : 'clock'" :size="25" />
            </div>
            <template v-if="query">
                <h2>No matching releases.</h2>
                <p>
                    Nothing found for “{{ query }}”. Try a version, feature, or pull request number.
                </p>
                <button type="button" class="fb-button fb-button--secondary" @click="clearSearch">
                    Clear search <Icon name="arrow-right" :size="16" />
                </button>
            </template>
            <template v-else-if="archive.state === 'unavailable'">
                <h2>Release notes aren’t available yet.</h2>
                <p>This server hasn’t downloaded them yet. They’re always on GitHub.</p>
                <a
                    class="fb-button fb-button--secondary"
                    :href="sourceUrl"
                    target="_blank"
                    rel="noopener noreferrer"
                    >GitHub Releases <Icon name="arrow-up-right" :size="16"
                /></a>
            </template>
            <template v-else>
                <h2>No releases yet.</h2>
                <p>Published releases will appear here.</p>
            </template>
        </div>

        <nav
            v-if="releases.prev_url || releases.next_url"
            class="cl-pagination"
            aria-label="Release pages"
        >
            <AppLink
                v-if="releases.prev_url"
                :href="releases.prev_url"
                class="cl-nav-card cl-nav-card--newer"
                ><Icon name="arrow-left" :size="17" />
                <div>
                    <small>Newer releases</small
                    ><strong>Page {{ releases.current_page - 1 }}</strong>
                </div></AppLink
            >
            <AppLink
                v-if="releases.next_url"
                :href="releases.next_url"
                class="cl-nav-card cl-nav-card--older"
                ><div>
                    <small>Older releases</small
                    ><strong>Page {{ releases.current_page + 1 }}</strong>
                </div>
                <Icon name="arrow-right" :size="17"
            /></AppLink>
        </nav>

        <div v-if="!query && archive.state === 'ready'" class="cl-end">
            <span>Looking for an earlier version?</span>
            <a :href="sourceUrl" target="_blank" rel="noopener noreferrer"
                >Browse the archive <Icon name="arrow-up-right" :size="13"
            /></a>
        </div>

        <div class="cl-sr-only" role="status" aria-live="polite" aria-atomic="true">
            {{ announcement }}
        </div>
        <Toast v-model:open="toast.open" :title="toast.title" />
    </div>
</template>

<style src="./changelog.css"></style>
