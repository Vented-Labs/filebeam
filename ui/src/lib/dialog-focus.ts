export function restoreDialogFocus(trigger?: HTMLElement | null): void {
    const target = [
        trigger,
        document.querySelector<HTMLElement>('.fb-mobile-nav'),
        document.querySelector<HTMLElement>('.fb-header__brand'),
    ].find((element) => element?.isConnected && element.getClientRects().length);
    target?.focus({ preventScroll: true });
}
