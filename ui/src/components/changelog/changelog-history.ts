const indexKey = 'filebeam:changelog:index';
const readerKey = 'filebeam:changelog:reader';

function storage(): Storage | null {
    try {
        return typeof window === 'undefined' ? null : window.sessionStorage;
    } catch {
        return null;
    }
}

/** Remembers the filtered index so "All releases" can restore search, page, and scroll. */
export function rememberIndex(): void {
    storage()?.setItem(indexKey, window.location.pathname + window.location.search);
}

export function rememberReaderOpenedFromIndex(url: string): void {
    storage()?.setItem(readerKey, new URL(url, window.location.origin).pathname);
}

export function indexUrl(): string {
    const saved = storage()?.getItem(indexKey);
    return saved?.startsWith('/changelog') ? saved : '/changelog';
}

/** True when the previous history entry is the index this reader was opened from. */
export function openedFromIndex(): boolean {
    return storage()?.getItem(readerKey) === window.location.pathname && window.history.length > 1;
}

export function forgetReaderOrigin(): void {
    storage()?.removeItem(readerKey);
}
