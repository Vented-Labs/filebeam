import { onMounted, onUnmounted, ref } from 'vue';

export type Appearance = 'light' | 'dark' | 'system';

declare global {
    interface Window {
        filebeamAppearance?: {
            get(): Appearance;
            set(value: Appearance): void;
            refresh(): void;
            dispose(): void;
        };
    }
}

export function useAppearance() {
    const mode = ref<'light' | 'dark'>(
        typeof document !== 'undefined' && document.documentElement.dataset.fbTheme === 'light'
            ? 'light'
            : 'dark',
    );
    const preference = ref<Appearance>(
        typeof window !== 'undefined' ? (window.filebeamAppearance?.get() ?? 'system') : 'system',
    );
    const update = () => {
        mode.value = document.documentElement.dataset.fbTheme === 'light' ? 'light' : 'dark';
        preference.value = window.filebeamAppearance?.get() ?? 'system';
    };
    onMounted(() => {
        update();
        window.addEventListener('filebeam:appearance', update);
    });
    onUnmounted(() => window.removeEventListener('filebeam:appearance', update));
    const set = (value: Appearance) => window.filebeamAppearance?.set(value);

    return { mode, preference, set };
}
