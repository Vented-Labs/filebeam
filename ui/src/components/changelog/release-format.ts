import type { ChangelogRelease } from '../../types/changelog';

const dateFormat = new Intl.DateTimeFormat('en-US', {
    month: 'long',
    day: 'numeric',
    year: 'numeric',
    timeZone: 'UTC',
});

export function releaseDate(release: Pick<ChangelogRelease, 'published_at'>): string | null {
    return release.published_at ? dateFormat.format(new Date(release.published_at)) : null;
}

export function releaseBadge(release: ChangelogRelease): string | null {
    if (release.prerelease) return 'Pre-release';
    if (release.latest) return 'Latest release';
    return null;
}

/** Absolute URL for a server-provided local path, on the current instance. */
export function absoluteUrl(url: string): string {
    if (typeof window === 'undefined') return url;
    return new URL(url, window.location.origin).href;
}
