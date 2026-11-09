<script setup lang="ts">
import { router } from '@inertiajs/vue3';
import { computed, nextTick, onMounted, ref } from 'vue';
import AppLink from '../primitives/AppLink.vue';
import Icon from '../primitives/Icon.vue';
import Toast from '../primitives/Toast.vue';
import ReleaseActions from './ReleaseActions.vue';
import ReleaseBody from './ReleaseBody.vue';
import { forgetReaderOrigin, indexUrl, openedFromIndex } from './changelog-history';
import { releaseBadge, releaseDate } from './release-format';
import type { ChangelogArchive, ChangelogRelease, ReleaseLink } from '../../types/changelog';

const props = defineProps<{
    release: ChangelogRelease;
    newer: ReleaseLink | null;
    older: ReleaseLink | null;
    archive: ChangelogArchive;
}>();

const date = computed(() => releaseDate(props.release));
const badge = computed(() => releaseBadge(props.release));
const sections = computed(() => {
    const headings = props.release.document?.headings ?? [];
    const top = Math.min(...headings.map((heading) => heading.level));
    return headings.filter((heading) => heading.level === top).slice(0, 12);
});
const backHref = ref('/changelog');
const toast = ref({ open: false, title: '' });

onMounted(() => {
    backHref.value = indexUrl();
});

function back(event: MouseEvent): void {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
        return;
    event.preventDefault();
    // Returning through history restores the index's search, page, and scroll position.
    if (openedFromIndex()) {
        forgetReaderOrigin();
        window.history.back();
        return;
    }
    router.visit(backHref.value);
}

function scrollToSection(event: MouseEvent, id: string): void {
    const target = document.getElementById(id);
    if (!target || event.button !== 0 || event.metaKey || event.ctrlKey) return;
    event.preventDefault();
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    target.scrollIntoView({ behavior: reduced ? 'auto' : 'smooth', block: 'start' });
    window.history.replaceState(window.history.state, '', `#${id}`);
}

function copied(message: string): void {
    toast.value = { open: false, title: message };
    void nextTick(() => (toast.value.open = true));
}
</script>

<template>
    <div class="fb-changelog cl-reader">
        <div class="cl-breadcrumb">
            <a :href="backHref" class="cl-back" @click="back"
                ><Icon name="arrow-left" :size="17" /> All releases</a
            >
            <code>Changelog / {{ release.tag }}</code>
        </div>
        <div class="cl-reader__layout">
            <aside class="cl-reader__aside" aria-label="Release details">
                <time v-if="date" class="cl-date" :datetime="release.published_at ?? undefined">{{
                    date
                }}</time>
                <span
                    v-if="badge"
                    class="cl-label"
                    :class="{ 'cl-label--muted': release.prerelease }"
                    ><Icon name="tag" :size="11" />{{ badge }}</span
                >
                <nav v-if="sections.length > 1" class="cl-sections" aria-label="In this release">
                    <span>In this release</span>
                    <a
                        v-for="section in sections"
                        :key="section.id"
                        :href="`#${section.id}`"
                        @click="scrollToSection($event, section.id)"
                        >{{ section.text }}</a
                    >
                </nav>
            </aside>
            <article class="cl-article" aria-labelledby="release-title">
                <span v-if="release.titled" class="cl-article__version">{{ release.tag }}</span>
                <h1 id="release-title" tabindex="-1">{{ release.title }}</h1>
                <div class="cl-reader__mobile-meta">
                    <time
                        v-if="date"
                        class="cl-date"
                        :datetime="release.published_at ?? undefined"
                        >{{ date }}</time
                    >
                    <span
                        v-if="badge"
                        class="cl-label"
                        :class="{ 'cl-label--muted': release.prerelease }"
                        ><Icon name="tag" :size="11" />{{ badge }}</span
                    >
                </div>
                <ReleaseBody :document="release.document" />
                <ReleaseActions :release="release" @copied="copied" />
            </article>
        </div>
        <nav v-if="newer || older" class="cl-reader__nav" aria-label="Adjacent releases">
            <AppLink v-if="newer" :href="newer.url" class="cl-nav-card cl-nav-card--newer"
                ><Icon name="arrow-left" :size="17" />
                <div>
                    <small>Newer release</small><strong>{{ newer.tag }}</strong>
                </div></AppLink
            >
            <AppLink v-if="older" :href="older.url" class="cl-nav-card cl-nav-card--older"
                ><div>
                    <small>Older release</small><strong>{{ older.tag }}</strong>
                </div>
                <Icon name="arrow-right" :size="17"
            /></AppLink>
        </nav>
        <Toast v-model:open="toast.open" :title="toast.title" />
    </div>
</template>

<style src="./changelog.css"></style>
