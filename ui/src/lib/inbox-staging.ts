import type { Transfer } from '../composables/useEncryptedDownload';
import type { AccountKeyBundle } from './account-crypto';

type RecordValue = {
    key: string;
    transfer?: Transfer;
    ciphertext?: ArrayBuffer;
    complete?: boolean;
    dismissed?: boolean;
};
let account: number | null = null;
let controller: AbortController | undefined;
let timer: ReturnType<typeof setTimeout> | undefined;
let running: Promise<void> | undefined;

function database(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open('filebeam-inbox-staging-v1', 1);
        request.onupgradeneeded = () =>
            request.result.createObjectStore('ciphertext', { keyPath: 'key' });
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(new Error('Private browser staging is unavailable.'));
    });
}
async function record(key: string, value?: RecordValue): Promise<RecordValue | undefined> {
    const db = await database();
    return new Promise((resolve, reject) => {
        const transaction = db.transaction('ciphertext', value ? 'readwrite' : 'readonly');
        const store = transaction.objectStore('ciphertext');
        const request = value ? store.put(value) : store.get(key);
        transaction.oncomplete = () => {
            resolve(value ?? (request.result as RecordValue | undefined));
            db.close();
        };
        transaction.onerror = () => {
            reject(new Error('Browser staging storage failed.'));
            db.close();
        };
        transaction.onabort = transaction.onerror;
    });
}
function prefix(id: number): string {
    return `${location.origin}|${id}|`;
}
function setting(id: number): string {
    return `filebeam:auto-inbox:${prefix(id)}`;
}
export function browserReceivingEnabled(id: number): boolean {
    return localStorage.getItem(setting(id)) === 'on';
}
export function enableBrowserReceiving(id: number, enabled: boolean): void {
    localStorage.setItem(setting(id), enabled ? 'on' : 'off');
    setInboxReceiverAccount(id);
}
export function setInboxReceiverAccount(id: number | null): void {
    if (typeof window === 'undefined') return;
    controller?.abort();
    if (timer) clearTimeout(timer);
    account = id;
    if (id === null || !browserReceivingEnabled(id)) return;
    const next = new AbortController();
    controller = next;
    const loop = async (): Promise<void> => {
        try {
            await running?.catch(() => undefined);
            if (next.signal.aborted) return;
            running = navigator.locks
                ? navigator.locks.request(
                      `filebeam-inbox:${prefix(id)}`,
                      { ifAvailable: true },
                      async (lock) => {
                          if (lock) await sweep(id, next.signal);
                      },
                  )
                : sweep(id, next.signal);
            await running;
        } catch {
            /* Network, permission and quota failures leave durable ciphertext for a later sweep. */
        } finally {
            if (!next.signal.aborted)
                timer = setTimeout(() => {
                    void loop();
                }, 60_000);
        }
    };
    void loop();
}
async function json<T>(url: string, signal: AbortSignal): Promise<T> {
    const response = await fetch(url, {
        signal,
        cache: 'no-store',
        redirect: 'error',
        headers: { Accept: 'application/json' },
    });
    if (!response.ok) throw new Error('Automatic receiving is paused.');
    return ((await response.json()) as { data: T }).data;
}
async function sweep(id: number, signal: AbortSignal): Promise<void> {
    // Bind every sweep to the actual cookie session, not a cached UI identity.
    const session = await json<{ id: number }>('/api/native/v1/session', signal);
    if (session.id !== id || account !== id) return;
    let after: string | null = null;
    do {
        const page: {
            next: string | null;
            transfers: Array<{ id: string; autoDownload: boolean; ciphertext_bytes: number }>;
        } = await json(`/api/native/v1/inbox/sync${after ? `?after=${after}` : ''}`, signal);
        for (const item of page.transfers) {
            if (!item.autoDownload || signal.aborted) continue;
            const key = `${prefix(id)}${item.id}`;
            const existing = await record(key);
            if (existing?.complete || existing?.dismissed) continue;
            if (item.ciphertext_bytes > 512 * 1024 * 1024) continue;
            const estimate = await navigator.storage.estimate();
            if (
                (estimate.usage ?? 0) + item.ciphertext_bytes >
                Math.min(estimate.quota ?? 512 * 1024 * 1024, 512 * 1024 * 1024)
            )
                continue;
            const transfer = await json<Transfer>(
                `/api/native/v1/inbox/${item.id}/staging`,
                signal,
            );
            if (
                transfer.id !== item.id ||
                transfer.protocol_version !== 1 ||
                transfer.driver !== 'http' ||
                !transfer.chunk_bytes
            )
                throw new Error('Unsupported staged transfer.');
            if (transfer.items.reduce((total, file) => total + file.chunk_count, 0) > 100_000)
                continue;
            const bundles = await json<AccountKeyBundle[]>('/api/native/v1/account/keys', signal);
            const bundle = bundles.find(
                (value) => value.id === transfer.recipient_key?.bundle.id && value.user_id === id,
            );
            if (!bundle || bundle.public_key !== transfer.recipient_key?.bundle.public_key)
                throw new Error('Receiving key identity changed.');
            transfer.recipient_key = {
                bundle,
                encrypted_key: transfer.recipient_key!.encrypted_key,
            };
            await record(key, { key, transfer });
            for (const file of transfer.items) {
                const plaintext = file.ciphertext_bytes - file.chunk_count * 16;
                if (
                    !Number.isSafeInteger(plaintext) ||
                    plaintext < 0 ||
                    file.chunk_count !== Math.max(1, Math.ceil(plaintext / transfer.chunk_bytes))
                )
                    throw new Error('Invalid staged chunk geometry.');
                for (let index = 0; index < file.chunk_count; index++) {
                    const chunkKey = `${key}|${file.id}|${index}`;
                    if ((await record(chunkKey))?.ciphertext) continue;
                    const expected =
                        Math.min(
                            transfer.chunk_bytes,
                            Math.max(0, plaintext - index * transfer.chunk_bytes),
                        ) + 16;
                    const response = await fetch(
                        `/api/native/v1/inbox/${item.id}/items/${file.id}/chunks/${index}?automatic=1`,
                        { signal, redirect: 'error' },
                    );
                    if (!response.ok || Number(response.headers.get('Content-Length')) !== expected)
                        throw new Error('Automatic chunk unavailable.');
                    const ciphertext = await response.arrayBuffer();
                    if (ciphertext.byteLength !== expected || signal.aborted)
                        throw new Error('Incomplete automatic chunk.');
                    await record(chunkKey, { key: chunkKey, ciphertext });
                }
            }
            await record(key, { key, transfer, complete: true });
        }
        after = page.next;
    } while (after && !signal.aborted);
}
export async function cachedInboxTransfer(id: string): Promise<Transfer | undefined> {
    if (account === null) return undefined;
    const value = await record(`${prefix(account)}${id}`);
    return value?.complete ? value.transfer : undefined;
}
export async function cachedInboxChunk(
    id: string,
    item: string,
    chunk: number,
): Promise<ArrayBuffer | undefined> {
    if (account === null) return undefined;
    return (await record(`${prefix(account)}${id}|${item}|${chunk}`))?.ciphertext;
}

export async function browserStagedTransfers(
    id: number,
): Promise<Array<{ id: string; transfer: Transfer }>> {
    const db = await database();
    const keys = await new Promise<IDBValidKey[]>((resolve, reject) => {
        const request = db
            .transaction('ciphertext')
            .objectStore('ciphertext')
            .getAllKeys(IDBKeyRange.bound(prefix(id), `${prefix(id)}\uffff`));
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });
    db.close();
    const metadata = await Promise.all(
        keys
            .filter((key) => typeof key === 'string' && key.split('|').length === 3)
            .map((key) => record(String(key))),
    );
    return metadata.flatMap((value) =>
        value?.complete && value.transfer
            ? [{ id: value.transfer.id, transfer: value.transfer }]
            : [],
    );
}
export async function dismissBrowserStaging(id: number, transfer: string): Promise<void> {
    controller?.abort();
    const key = `${prefix(id)}${transfer}`;
    const db = await database();
    await new Promise<void>((resolve, reject) => {
        const transaction = db.transaction('ciphertext', 'readwrite');
        const store = transaction.objectStore('ciphertext');
        store.put({ key, dismissed: true });
        const cursor = store.openKeyCursor(IDBKeyRange.bound(`${key}|`, `${key}|\uffff`));
        cursor.onsuccess = () => {
            const value = cursor.result;
            if (value) {
                store.delete(value.primaryKey);
                value.continue();
            }
        };
        transaction.oncomplete = () => resolve();
        transaction.onerror = () => reject(new Error('Could not remove local staging.'));
    });
    db.close();
    setInboxReceiverAccount(account);
}
