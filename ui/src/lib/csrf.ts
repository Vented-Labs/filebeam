export function csrfHeaders(json = false): HeadersInit {
    const cookieToken = document.cookie
        .split('; ')
        .find((cookie) => cookie.startsWith('XSRF-TOKEN='))
        ?.slice('XSRF-TOKEN='.length);
    const token = cookieToken ? decodeURIComponent(cookieToken) : undefined;
    const metaToken = document.querySelector<HTMLMetaElement>('meta[name="csrf-token"]')?.content;

    return {
        Accept: 'application/json',
        ...(json ? { 'Content-Type': 'application/json' } : {}),
        ...(token ? { 'X-XSRF-TOKEN': token } : metaToken ? { 'X-CSRF-TOKEN': metaToken } : {}),
    };
}
