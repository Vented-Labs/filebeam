/** Roving focus for small illustration tablists: arrows, Home, and End move and select. */
export function onTabKeydown(event: KeyboardEvent): void {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    const list = (event.currentTarget as HTMLElement).closest('[role="tablist"]');
    if (!list) return;
    const tabs = Array.from(list.querySelectorAll<HTMLElement>('[role="tab"]:not([disabled])'));
    const current = tabs.indexOf(document.activeElement as HTMLElement);
    if (current < 0) return;
    event.preventDefault();
    const next =
        event.key === 'Home'
            ? 0
            : event.key === 'End'
              ? tabs.length - 1
              : (current + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
    tabs[next]?.focus();
    tabs[next]?.click();
}

/** Host of this instance, for illustrative share links on the real origin. */
export function instanceHost(mainSiteUrl: string | undefined): string {
    try {
        return new URL(mainSiteUrl ?? '').host || 'filebeam.example';
    } catch {
        return 'filebeam.example';
    }
}
