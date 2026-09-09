export function buildShareLink(shareUrl: string, origin: string, fragment = ''): string {
    const url = new URL(shareUrl, origin).toString().split('#')[0];
    return `${url}${fragment}`;
}
