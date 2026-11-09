<script setup lang="ts">
import { computed } from 'vue';
import BrandLogo from '../brand/BrandLogo.vue';
import Icon from '../primitives/Icon.vue';
import { useAppearance } from '../../composables/useAppearance';
import type { AppearanceMode, ThemePreset } from '../../lib/appearance-types';

// The same preference contract as the footer control; changes apply site-wide.
const appearance = useAppearance();
const swatches = computed(() =>
    appearance.catalog.value.filter(
        (item) => appearance.customColors.value || item.id === 'instance',
    ),
);
const currentLabel = computed(
    () =>
        appearance.catalog.value.find((item) => item.id === appearance.preset.value)?.label ??
        'Instance default',
);
const modes = [
    { value: 'system', label: 'System appearance', icon: 'monitor' },
    { value: 'light', label: 'Light appearance', icon: 'sun' },
    { value: 'dark', label: 'Dark appearance', icon: 'moon' },
] as const satisfies ReadonlyArray<{ value: AppearanceMode; label: string; icon: string }>;

function choose(preset: ThemePreset): void {
    appearance.setPreset(preset);
}
</script>

<template>
    <article class="appearance-card feature-card" aria-labelledby="appearance-title">
        <div class="section-icon"><Icon class="icon" name="brush" /></div>
        <h2 id="appearance-title">Pick a color.</h2>
        <p>Light, dark, or system. The entire page follows along.</p>
        <div class="appearance-demo">
            <div class="palette-orbit" aria-hidden="true">
                <div class="palette-orbit-ring" />
                <div class="palette-fold"><BrandLogo compact label="" /></div>
            </div>
            <div class="theme-controls">
                <div class="swatch-row" role="group" aria-label="Color">
                    <button
                        v-for="item in swatches"
                        :key="item.id"
                        type="button"
                        class="swatch"
                        :data-preset="item.id"
                        :style="{ '--swatch': item.primary }"
                        :aria-pressed="appearance.preset.value === item.id"
                        :aria-label="item.label"
                        :title="item.label"
                        @click="choose(item.id)"
                    />
                </div>
                <div class="appearance-bottom">
                    <span class="current-color-label">{{ currentLabel }}</span>
                    <div class="mode-picker" role="group" aria-label="Appearance">
                        <button
                            v-for="mode in modes"
                            :key="mode.value"
                            type="button"
                            class="icon-button small"
                            :aria-label="mode.label"
                            :aria-pressed="appearance.preference.value === mode.value"
                            @click="appearance.set(mode.value)"
                        >
                            <Icon class="icon" :name="mode.icon" />
                        </button>
                    </div>
                </div>
            </div>
        </div>
    </article>
</template>
