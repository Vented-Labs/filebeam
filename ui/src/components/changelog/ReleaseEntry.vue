<script setup lang="ts">
import { computed } from 'vue';
import AppLink from '../primitives/AppLink.vue';
import Icon from '../primitives/Icon.vue';
import ReleaseActions from './ReleaseActions.vue';
import ReleaseBody from './ReleaseBody.vue';
import { releaseBadge, releaseDate } from './release-format';
import type { ChangelogRelease } from '../../types/changelog';

const props = withDefaults(defineProps<{ release: ChangelogRelease; query?: string }>(), {
    query: '',
});
defineEmits<{ copied: [message: string] }>();
const date = computed(() => releaseDate(props.release));
const badge = computed(() => releaseBadge(props.release));
const titleId = computed(() => `release-title-${props.release.id}`);
// Keep the hover arrow on the same line as the title's last word.
const titleParts = computed(() => {
    const title = props.release.title;
    const split = title.lastIndexOf(' ') + 1;
    return { head: title.slice(0, split), tail: title.slice(split) };
});
</script>

<template>
    <section
        :id="`release-${release.id}`"
        class="cl-row"
        :class="{ 'is-featured': release.featured }"
        :aria-labelledby="titleId"
    >
        <div class="cl-meta">
            <AppLink :href="release.url" class="cl-version" :data-release-tag="release.tag">{{
                release.tag
            }}</AppLink>
            <time v-if="date" class="cl-date" :datetime="release.published_at ?? undefined">{{
                date
            }}</time>
            <span v-if="badge" class="cl-label" :class="{ 'cl-label--muted': release.prerelease }"
                ><Icon name="tag" :size="11" />{{ badge }}</span
            >
        </div>
        <article class="cl-sheet">
            <h2 :id="titleId" class="cl-title" :class="{ 'cl-sr-only': !release.titled }">
                <AppLink :href="release.url"
                    >{{ titleParts.head
                    }}<span class="cl-title__tail"
                        >{{ titleParts.tail
                        }}<Icon class="cl-title__arrow" name="arrow-right" :size="19" /></span
                ></AppLink>
            </h2>
            <ReleaseBody :document="release.document" :highlight="query" :graphics="!query" />
            <ReleaseActions :release="release" @copied="$emit('copied', $event)" />
        </article>
    </section>
</template>
