<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import {
    ChangelogIndex,
    RouteSurface,
    type ChangelogArchive,
    type ChangelogFilters,
    type ChangelogPage,
    type ChangelogVersion,
} from '@filebeam/ui';

defineOptions({ layout: RouteSurface });

defineProps<{
    releases: ChangelogPage;
    filters: ChangelogFilters;
    versions: ChangelogVersion[];
    has_prereleases: boolean;
    archive: ChangelogArchive;
    source_url: string;
}>();
</script>

<template>
    <div>
        <Head title="Changelog">
            <meta name="description" content="Release notes for Filebeam." />
        </Head>
        <ChangelogIndex
            :releases="releases"
            :filters="filters"
            :versions="versions"
            :has-prereleases="has_prereleases"
            :archive="archive"
            :source-url="source_url"
        />
    </div>
</template>
