<script setup lang="ts">
import ReleaseGraphic from './ReleaseGraphic.vue';
import { ReleaseNodes } from './release-nodes';
import type { ReleaseDocument } from '../../types/changelog';

withDefaults(
    defineProps<{ document: ReleaseDocument | null; highlight?: string; graphics?: boolean }>(),
    { highlight: '', graphics: true },
);
</script>

<template>
    <div v-if="document" class="cl-body">
        <template v-for="block in document.blocks" :key="block.id">
            <div v-if="block.type === 'content'" class="release-block">
                <ReleaseNodes :nodes="block.nodes" :highlight="highlight" />
            </div>
            <ReleaseGraphic v-else-if="graphics" :graphic-key="block.key" />
        </template>
    </div>
</template>
