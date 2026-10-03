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
    const response = await fetch(`/api/native/v1/${path}`, {
        method,
        headers: csrfHeaders(true),
        credentials: 'same-origin',
        cache: 'no-store',
        redirect: 'error',
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    const payload = (await response.json()) as {
        data: T;
        message?: string;
        errors?: Record<string, string[]>;
    };
    if (!response.ok)
        throw new Error(
            Object.values(payload.errors ?? {}).flat()[0] ??
                payload.message ??
                'Could not update contacts.',
        );
    return payload.data;
}

export function overrideValue(value: string): boolean | null {
    return value === 'inherit' ? null : value === 'allow';
}
