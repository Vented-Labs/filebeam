<script setup lang="ts">
import { computed } from 'vue';
import { useAppearance } from '../../../composables/useAppearance';
import Icon from '../../primitives/Icon.vue';
import ReleaseArtFrame from './ReleaseArtFrame.vue';

const appearance = useAppearance();
// Swatches come from this instance's preset catalog; surfaces and text follow the active theme.
const presets = computed(() => appearance.catalog.value.filter((item) => item.id !== 'instance'));
const current = computed(() => {
    if (appearance.preset.value !== 'instance') return appearance.preset.value;
    const instance = appearance.catalog.value.find((item) => item.id === 'instance');
    return appearance.catalog.value.find(
        (item) => item.id !== 'instance' && item.primary === instance?.primary,
    )?.id;
});
const currentLabel = computed(
    () =>
        appearance.catalog.value.find((item) => item.id === appearance.preset.value)?.label ??
        'Instance default',
);
const modes = [
    { value: 'system', label: 'System', icon: 'monitor' },
    { value: 'light', label: 'Light', icon: 'sun' },
    { value: 'dark', label: 'Dark', icon: 'moon' },
] as const;
</script>

<template>
    <ReleaseArtFrame
        title="Color presets."
        caption="Light, dark, or system."
        description="Illustration: the Appearance panel with system, light, and dark modes and seven color presets, beside a pasted readme note."
    >
        <div class="composition">
            <div class="paper-shadow" />
            <div class="appearance-slip">
                <div class="slip-top"><Icon name="brush" :size="13" /> Appearance</div>
                <div class="slip-modes">
                    <span
                        v-for="mode in modes"
                        :key="mode.value"
                        :class="{ 'is-active': appearance.mode.value === mode.value }"
                        ><Icon :name="mode.icon" :size="10" />{{ mode.label }}</span
                    >
                </div>
                <div class="slip-swatches">
                    <span
                        v-for="preset in presets"
                        :key="preset.id"
                        class="slip-swatch"
                        :class="{ 'is-current': preset.id === current }"
                        :style="{ '--sample': preset.primary }"
                    />
                </div>
                <div class="slip-label">{{ currentLabel }}</div>
            </div>
            <div class="clipboard-slip">
                <div class="clip-title"><Icon name="note" :size="12" /> readme.md</div>
                <div class="clip-body"><span># Project notes</span><br />Ready to share.</div>
                <div class="clip-key">Ctrl + V</div>
            </div>
        </div>
    </ReleaseArtFrame>
</template>

<style scoped>
.composition {
    position: absolute;
    top: 0;
    right: 20px;
    width: 360px;
    height: 100%;
    transform-origin: right center;
}
.paper-shadow {
    position: absolute;
    z-index: 0;
    top: 9px;
    left: 150px;
    width: 147px;
    height: 115px;
    border: 1px solid var(--fb-border);
    border-radius: 10px;
    background: var(--fb-accent-surface);
    opacity: 0.6;
    transform: rotate(16deg);
}
.appearance-slip {
    position: absolute;
    z-index: 3;
    top: 25px;
    left: 5px;
    width: 185px;
    box-sizing: border-box;
    padding: 13px 14px;
    border: 1px solid color-mix(in srgb, var(--fb-control-border) 45%, var(--fb-border));
    border-radius: 12px;
    background: linear-gradient(145deg, var(--cl-paper), var(--fb-surface));
    box-shadow: var(--cl-shadow);
    color: var(--fb-text);
    transform: rotate(-6deg);
}
.slip-top {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 10px;
    font-weight: 500;
}
.slip-top .fb-icon {
    color: var(--fb-accent-text);
}
.slip-modes {
    display: flex;
    align-items: center;
    gap: 1px;
    margin-top: 10px;
    padding: 3px;
    border: 1px solid var(--fb-border);
    border-radius: 6px;
    color: var(--fb-text-muted);
    font-size: 8px;
}
.slip-modes span {
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 4px 0;
    border-radius: 3px;
}
.slip-modes .is-active {
    background: var(--fb-surface-raised);
    box-shadow: inset 0 0 0 1px var(--cl-line);
    color: var(--fb-accent-text);
}
.slip-swatches {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 12px;
    margin-top: 14px;
}
.slip-swatch {
    position: relative;
    width: 12px;
    height: 12px;
    flex: none;
    border-radius: 50%;
    background: var(--sample);
    box-shadow: inset 0 0 0 1px var(--fb-line-strong);
}
.slip-swatch.is-current::after {
    content: '';
    position: absolute;
    inset: -3px;
    border: 1px solid var(--fb-text-muted);
    border-radius: 50%;
}
.slip-label {
    margin-top: 10px;
    color: var(--fb-text-muted);
    font-size: 7px;
}
.clipboard-slip {
    position: absolute;
    z-index: 2;
    top: 42px;
    left: 176px;
    width: 170px;
    height: 133px;
    box-sizing: border-box;
    padding: 14px;
    border: 1px solid var(--fb-border);
    border-radius: 10px;
    background: linear-gradient(140deg, var(--cl-paper), var(--fb-surface-sunken));
    box-shadow: var(--cl-shadow);
    color: var(--fb-text);
    transform: rotate(8deg);
}
.clip-title {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--fb-text-muted);
    font: 8px var(--fb-font-code);
}
.clip-title .fb-icon {
    color: var(--fb-accent-text);
}
.clip-body {
    margin-top: 17px;
    font: 9px/1.75 var(--fb-font-code);
}
.clip-body span {
    color: var(--fb-accent-text);
}
.clip-key {
    position: absolute;
    right: 12px;
    bottom: 14px;
    padding: 3px 6px;
    border: 1px solid var(--fb-border);
    border-radius: 4px;
    box-shadow: 0 2px 0 var(--fb-border);
    color: var(--fb-text-muted);
    font: 9px var(--fb-font-code);
}
@container (max-width: 760px) {
    .composition {
        right: 5px;
        transform: scale(0.9);
    }
}
@container (max-width: 640px) {
    .composition {
        right: 50%;
        margin-right: -180px;
        transform: scale(0.88);
        transform-origin: center center;
    }
}
@container (max-width: 380px) {
    .composition {
        transform: scale(0.75);
    }
}
</style>
