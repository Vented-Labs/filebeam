<script setup lang="ts">
import { Link } from '@inertiajs/vue3';

withDefaults(defineProps<{ href: string; viewTransition?: boolean }>(), { viewTransition: false });
</script>

<template>
    <Link :href="href" :view-transition="viewTransition"><slot /></Link>
</template>
