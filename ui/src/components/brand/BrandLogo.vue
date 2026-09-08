<script setup lang="ts">
import { computed } from 'vue';
import { useBranding } from '../../lib/branding';

const props = withDefaults(
    defineProps<{
        compact?: boolean;
        label?: string;
    }>(),
    { compact: false },
);

const branding = useBranding();
const displayLabel = computed(() => props.label ?? branding.value.name);
const customIdentity = computed(
    () => Boolean(branding.value.logo_url) || displayLabel.value !== 'Filebeam',
);
</script>

<template>
    <span class="fb-brand" :class="{ 'fb-brand--compact': compact }">
        <!-- The default lockup includes outlined lettering; never add a second wordmark. -->
        <img
            :class="compact || customIdentity ? 'fb-brand__glyph' : 'fb-brand__lockup'"
            :src="
                branding.logo_url ||
                (compact || customIdentity ? branding.default_mark_url : branding.default_logo_url)
            "
            :alt="customIdentity && !compact ? '' : displayLabel"
        />
        <strong v-if="customIdentity && !compact" class="fb-brand__wordmark">{{
            displayLabel
        }}</strong>
    </span>
</template>
