import { csrfHeaders } from './csrf';

export type ReceivingDefaults = {
    receivingPolicy: 'anyone' | 'authenticated' | 'friends' | 'nobody';
    autoDownloadFriends: boolean;
    revision: number;
};
export type ContactIdentity = { id: number; username: string; name: string };
export type Contact = ContactIdentity & {
    status: 'accepted' | 'incoming' | 'outgoing';
    canSend: boolean | null;
    autoDownload: boolean | null;
    effective: { canSend: boolean; autoDownload: boolean };
};
export type Contacts = {
    settings: ReceivingDefaults;
    contacts: Contact[];
    blocked: ContactIdentity[];
};

export async function contactRequest<T>(path: string, method = 'GET', body?: unknown): Promise<T> {
    const unavailable = 'The request could not be completed. Try again in a moment.';
    let response: Response;
    try {
        response = await fetch(`/api/native/v1/${path}`, {
            method,
            headers: csrfHeaders(true),
            credentials: 'same-origin',
            cache: 'no-store',
            redirect: 'error',
            ...(body === undefined ? {} : { body: JSON.stringify(body) }),
        });
    } catch {
        throw new Error(unavailable);
    }
    if (response.status === 401 || response.status === 419)
        throw new Error('Your session has expired. Sign in again and try once more.');
    if (response.status >= 500) throw new Error(unavailable);
    const payload = (await response.json().catch(() => null)) as {
        data: T;
        message?: string;
        errors?: Record<string, string[]>;
    } | null;
    if (!payload || typeof payload !== 'object') throw new Error(unavailable);
    if (!response.ok) {
        const message = Object.values(payload.errors ?? {}).flat()[0] ?? payload.message;
        if (!message || /App\\|No query results|model \[/i.test(message))
            throw new Error('This username or receiving inbox is unavailable on this instance.');
        throw new Error(message);
    }
    if (!('data' in payload)) throw new Error(unavailable);
    return payload.data;
}

export function overrideValue(value: string): boolean | null {
    return value === 'inherit' ? null : value === 'allow';
}
