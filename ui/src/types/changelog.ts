export const releaseGraphicKeys = ['theme-presets', 'friends-devices'] as const;
export type ReleaseGraphicKey = (typeof releaseGraphicKeys)[number];

/** Text, or an element from the server's allowlist with its attributes and children. */
export type ReleaseNode = string | { t: string; a?: Record<string, string>; c?: ReleaseNode[] };

export type ReleaseBlock =
    | { type: 'content'; id: string; nodes: ReleaseNode[] }
    | { type: 'graphic'; id: string; key: string };

export interface ReleaseDocument {
    schema: 2;
    blocks: ReleaseBlock[];
    headings: Array<{ id: string; text: string; level: number }>;
    truncated: boolean;
}

export interface ChangelogRelease {
    id: number;
    tag: string;
    title: string;
    titled: boolean;
    url: string;
    html_url: string;
    author: string | null;
    prerelease: boolean;
    latest: boolean;
    published_at: string | null;
    document: ReleaseDocument | null;
    description: string;
    featured?: boolean;
}

export interface ChangelogPage {
    data: ChangelogRelease[];
    current_page: number;
    last_page: number;
    total: number;
    prev_url: string | null;
    next_url: string | null;
}

export interface ChangelogArchive {
    state: 'ready' | 'empty' | 'unavailable';
    stale: boolean;
    checked_at: string | null;
}

export interface ChangelogVersion {
    tag: string;
    url: string;
    page: number;
    prerelease: boolean;
}

export interface ChangelogFilters {
    q: string;
    prereleases: boolean;
}

export interface ReleaseLink {
    tag: string;
    url: string;
}
