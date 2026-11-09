<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import {
    ReleaseReader,
    RouteSurface,
    type ChangelogArchive,
    type ChangelogRelease,
    type ReleaseLink,
} from '@filebeam/ui';

defineOptions({ layout: RouteSurface });

defineProps<{
    release: ChangelogRelease;
    newer: ReleaseLink | null;
    older: ReleaseLink | null;
    archive: ChangelogArchive;
}>();
</script>

<template>
    <div>
        <Head :title="`${release.tag} · Changelog`">
            <meta name="description" :content="release.description" />
            <link rel="canonical" :href="release.url" />
        </Head>
        <ReleaseReader :release="release" :newer="newer" :older="older" :archive="archive" />
    </div>
</template>
