import { computed, onMounted, onUnmounted, shallowRef } from 'vue';
import type {
    AppearanceMode,
    AppearancePayload,
    AppearanceSnapshot,
    ThemePreset,
} from '../lib/appearance-types';

export type Appearance = AppearanceMode;

declare global {
    interface Window {
        filebeamAppearance?: {
            get(): AppearanceMode;
            set(value: AppearanceMode): void;
            setPreset(value: ThemePreset): void;
            reset(): void;
            retry(): void;
            hydrate(payload: AppearancePayload): void;
            snapshot(): AppearanceSnapshot;
            refresh(): void;
            dispose(): void;
        };
    }
}

function snapshot(): AppearanceSnapshot {
    return (
        (typeof window !== 'undefined' ? window.filebeamAppearance?.snapshot() : undefined) ?? {
            preference: { mode: 'system', preset: 'instance' },
            mode: 'dark',
            account: null,
            catalog: [],
            palette: null,
            customColors: false,
            status: 'idle',
            error: '',
        }
    );
}

export function useAppearance() {
    const state = shallowRef(snapshot());
    const update = () => {
        state.value = snapshot();
    };
    onMounted(() => {
        update();
        window.addEventListener('filebeam:appearance', update);
    });
    onUnmounted(() => window.removeEventListener('filebeam:appearance', update));

    return {
        mode: computed(() => state.value.mode),
        preference: computed(() => state.value.preference.mode),
        preset: computed(() => state.value.preference.preset),
        palette: computed(() => state.value.palette),
        catalog: computed(() => state.value.catalog),
        account: computed(() => state.value.account),
        customColors: computed(() => state.value.customColors),
        status: computed(() => state.value.status),
        error: computed(() => state.value.error),
        set: (value: AppearanceMode) => window.filebeamAppearance?.set(value),
        setPreset: (value: ThemePreset) => window.filebeamAppearance?.setPreset(value),
        reset: () => window.filebeamAppearance?.reset(),
        retry: () => window.filebeamAppearance?.retry(),
    };
}
