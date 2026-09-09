<script setup lang="ts">
import { usePage } from '@inertiajs/vue3';
import { computed, onMounted, ref } from 'vue';
import type { FilebeamConfig } from '../../types';
import AppLink from '../primitives/AppLink.vue';

const props = defineProps<{
    href: '/login' | '/register';
    preserveState?: boolean;
    preserveScroll?: boolean;
}>();
const page = usePage<{ filebeam: FilebeamConfig }>();
const origin = ref('');
onMounted(() => {
    origin.value = window.location.origin;
});

const destination = computed(
    () => new URL(`${page.props.filebeam.main_site_url.replace(/\/+$/, '')}${props.href}`),
);
const sameOrigin = computed(() => destination.value.origin === origin.value);
const localHref = computed(
    () => `${destination.value.pathname}${destination.value.search}${destination.value.hash}`,
);
</script>

<template>
    <AppLink
        v-if="sameOrigin"
        :href="localHref"
        :preserve-state="preserveState"
        :preserve-scroll="preserveScroll"
        ><slot
    /></AppLink>
    <a v-else :href="destination.href"><slot /></a>
</template>
