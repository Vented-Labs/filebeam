<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import { FeaturesPage, RouteSurface } from '@filebeam/ui';

defineOptions({ layout: RouteSurface });
defineProps<{ devices_enabled: boolean }>();
</script>

<template>
    <div>
        <Head title="Features">
            <meta
                name="description"
                content="End-to-end encrypted files and notes, sending to friends, transfer history, and a CLI."
            />
        </Head>
        <FeaturesPage :devices-enabled="devices_enabled" />
    </div>
</template>
