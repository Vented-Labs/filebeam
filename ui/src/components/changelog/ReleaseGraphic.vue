<script setup lang="ts">
import { computed } from 'vue';
import FriendsDevicesGraphic from './graphics/FriendsDevicesGraphic.vue';
import ThemePresetsGraphic from './graphics/ThemePresetsGraphic.vue';
import { releaseGraphicKeys, type ReleaseGraphicKey } from '../../types/changelog';

const props = defineProps<{ graphicKey: string }>();

// A literal registry: release Markdown can only select reviewed, shipped components.
const releaseGraphics = {
    'theme-presets': ThemePresetsGraphic,
    'friends-devices': FriendsDevicesGraphic,
} as const satisfies Record<ReleaseGraphicKey, unknown>;

const graphic = computed(() =>
    (releaseGraphicKeys as readonly string[]).includes(props.graphicKey)
        ? releaseGraphics[props.graphicKey as ReleaseGraphicKey]
        : null,
);
</script>

<template>
    <component :is="graphic" v-if="graphic" />
</template>
