export type AppearanceMode = 'system' | 'light' | 'dark';
export type ThemePreset =
    | 'instance'
    | 'purple'
    | 'blue'
    | 'teal'
    | 'green'
    | 'amber'
    | 'orange'
    | 'rose';
export type AppearancePreference = { mode: AppearanceMode; preset: ThemePreset };
export type AppearancePalette = {
    id: ThemePreset;
    label: string;
    primary: string;
    on_color: string;
    branding: {
        default_logo_url: string;
        default_light_logo_url: string;
        default_mark_url: string;
    };
    favicons: Record<string, string>;
    chrome: { dark: string; light: string };
};
export type AppearancePayload = {
    account: number | null;
    preference: AppearancePreference;
    needs_adoption: boolean;
    custom_colors: boolean;
    catalog: AppearancePalette[];
    save_url: string;
    csrf: string | null;
    styles_url: string;
};
export type AppearanceSnapshot = {
    preference: AppearancePreference;
    mode: 'light' | 'dark';
    account: number | null;
    catalog: AppearancePalette[];
    palette: AppearancePalette | null;
    customColors: boolean;
    status: 'idle' | 'saving' | 'saved' | 'error';
    error: string;
};
