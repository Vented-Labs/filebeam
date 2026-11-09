<script setup lang="ts">
import { Link } from '@inertiajs/vue3';

withDefaults(defineProps<{ href: string; viewTransition?: boolean; prefetch?: boolean }>(), {
    viewTransition: false,
    prefetch: false,
});
</script>

<template>
    <Link :href="href" :view-transition="viewTransition" :prefetch="prefetch"><slot /></Link>
</template>
