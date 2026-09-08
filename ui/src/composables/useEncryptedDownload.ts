import { computed, onBeforeUnmount, ref, shallowRef } from 'vue';
import type { RecipientKey } from '../lib/account-crypto';
import { decodeBase64Url } from '../lib/base64url';
import {
    AdaptiveConcurrency,
    fetchChunkWithRetry,
    readChunkWithRetry,
    retryAfterMilliseconds,
    transferConcurrency,
} from '../lib/transfer';
import { waitForWorkerMessage, type WorkerMessage } from '../lib/worker-request';
import { progress as transferProgress } from '../../../backend/resources/js/actions/App/Http/Controllers/Api/V1/TurboTransferController';
import {
    store as createSession,
    update as updateSession,
} from '../../../backend/resources/js/actions/App/Http/Controllers/Api/V1/DownloadSessionController';

const MAX_PLAINTEXT_CHUNK_BYTES = 25_000_000 - 16;

export type Transfer = {
    id: string;
    kind: 'files' | 'note';
    protocol_version: 1;
    encrypted_manifest: string | null;
    encrypted_descriptor?: string | null;
    status?: 'pending' | 'available';
    declared_ciphertext_bytes?: number;
    ciphertext_bytes: number;
    expires_at: string;
    burn_on_read: boolean;
    chunk_bytes?: number;
    download_concurrency?: number;
    recipient_key?: RecipientKey;
    items: Array<{
        id: string;
        position: number;
        chunk_count: number;
        ciphertext_bytes: number;
        declared_ciphertext_bytes?: number;
    }>;
};

export type ManifestItem = {
    id: string;
    name: string;
    type: string;
    size: number;
    nonce_prefix: string;
    chunk_count: number;
    digest?: { algorithm: 'sha256'; value: string };
};

export type Manifest = {
    version: 1;
    items: ManifestItem[];
    language?: string;
    title?: string;
    read_token?: string;
    purpose?: 'turbo-descriptor';
    chunk_bytes?: number;
};

type Availability = {
    status: 'pending' | 'available';
    progress: number;
    expires_at: string;
    uploader_status: 'uploading' | 'stalled' | 'completed' | 'unavailable';
    items: Array<{ id: string; ready_chunks: number; uploaded_chunks: number }>;
};
type DownloadPhase = 'downloading' | 'waiting' | 'verifying' | 'completed';
type SessionReporter = {
    id?: string;
    token?: string;
    sequence: number;
    progress: number;
    terminal?: 'completed' | 'cancelled' | 'error';
    timer?: number;
    controller: AbortController;
};

type WritableFile = {
    write: (data: Uint8Array) => Promise<void>;
    close: () => Promise<void>;
    abort: () => Promise<void>;
};
type SavePicker = {
    showSaveFilePicker: (options: {
        suggestedName: string;
    }) => Promise<{ createWritable: () => Promise<WritableFile> }>;
};

function normalizeKey(value: string): string | undefined {
    const key = value.trim().replace(/^v1\./, '');
    return key || undefined;
}

function hasPasswordFactor(encryptedManifest: string | null, protocolVersion: number): boolean {
    try {
        const envelope = JSON.parse(encryptedManifest ?? '') as Record<string, unknown>;
        return envelope.v === protocolVersion && typeof envelope.salt === 'string';
    } catch {
        return false;
    }
}

function safeFilename(name: string): string {
    return name.replace(/[\\/\p{Cc}]/gu, '_') || 'download';
}

function validateManifest(candidate: unknown, transfer: Transfer, descriptor = false): Manifest {
    if (!candidate || typeof candidate !== 'object')
        throw new Error('The decrypted manifest is invalid.');
    const value = candidate as Partial<Manifest>;
    if (
        descriptor &&
        (value.purpose !== 'turbo-descriptor' ||
            value.version !== 1 ||
            transfer.protocol_version !== 1 ||
            transfer.kind !== 'files' ||
            value.chunk_bytes !== transfer.chunk_bytes)
    )
        throw new Error('The encrypted transfer descriptor is invalid.');
    if (!descriptor && value.purpose !== undefined)
        throw new Error('A final transfer manifest is required.');
    if (value.title !== undefined && (typeof value.title !== 'string' || value.title.length > 160))
        throw new Error('Invalid note title.');
    if (
        (transfer.burn_on_read || value.read_token !== undefined) &&
        (transfer.kind !== 'note' ||
            typeof value.read_token !== 'string' ||
            !/^[A-Za-z0-9]{64}$/.test(value.read_token))
    )
        throw new Error('Invalid burn-on-read capability.');
    if (
        value.version !== 1 ||
        !Array.isArray(value.items) ||
        value.items.length !== transfer.items.length ||
        (value.language !== undefined && typeof value.language !== 'string') ||
        (transfer.kind === 'note' && value.items.length !== 1)
    )
        throw new Error('The decrypted manifest does not match this transfer.');

    const ids = new Set<string>();
    for (const item of value.items) {
        const server = transfer.items.find((entry) => entry.id === item.id);
        if (
            !server ||
            ids.has(item.id) ||
            typeof item.name !== 'string' ||
            typeof item.type !== 'string' ||
            !Number.isSafeInteger(item.size) ||
            item.size < 0 ||
            typeof item.nonce_prefix !== 'string' ||
            decodeBase64Url(item.nonce_prefix).length !== 16 ||
            !Number.isSafeInteger(item.chunk_count) ||
            item.chunk_count < 1 ||
            (item.digest !== undefined &&
                (!item.digest ||
                    typeof item.digest !== 'object' ||
                    item.digest.algorithm !== 'sha256' ||
                    typeof item.digest.value !== 'string' ||
                    !/^[a-f0-9]{64}$/.test(item.digest.value))) ||
            (!descriptor && item.digest === undefined) ||
            (descriptor && item.digest !== undefined)
        )
            throw new Error('The decrypted manifest contains invalid item metadata.');
        if (
            !Number.isSafeInteger(transfer.chunk_bytes) ||
            !transfer.chunk_bytes ||
            transfer.chunk_bytes > MAX_PLAINTEXT_CHUNK_BYTES
        )
            throw new Error(
                'This server response is missing chunk_bytes, so download integrity cannot be verified.',
            );
        const expectedCount = Math.ceil(item.size / transfer.chunk_bytes) || 1;
        if (
            server.chunk_count !== expectedCount ||
            item.chunk_count !== expectedCount ||
            (descriptor ? server.declared_ciphertext_bytes : server.ciphertext_bytes) !==
                item.size + expectedCount * 16
        )
            throw new Error('The transfer is incomplete or has been altered.');
        ids.add(item.id);
    }
    if (
        (descriptor ? transfer.declared_ciphertext_bytes : transfer.ciphertext_bytes) !==
        transfer.items.reduce(
            (total, item) =>
                total + (descriptor ? item.declared_ciphertext_bytes! : item.ciphertext_bytes),
            0,
        )
    )
        throw new Error('The transfer size is invalid.');
    return value as Manifest;
}

export function useEncryptedDownload(transferId: string, inbox = false) {
    const transfer = ref<Transfer>();
    const manifest = ref<Manifest>();
    // Typed arrays must not be proxied: crypto APIs require their native backing buffer.
    const masterKey = shallowRef<Uint8Array>();
    const state = ref<'loading' | 'key' | 'ready' | 'downloading' | 'error'>('loading');
    const error = ref('');
    const progress = ref(0);
    const uploadProgress = ref(0);
    const uploaderStatus = ref<Availability['uploader_status']>('uploading');
    const downloadPhase = ref<DownloadPhase>('downloading');
    const downloadItemCount = ref(0);
    const downloadedItemIds = ref<string[]>([]);
    const turbo = computed(
        () =>
            !inbox &&
            transfer.value?.kind === 'files' &&
            Boolean(transfer.value.encrypted_descriptor),
    );
    let earlyManifest: Manifest | undefined;
    let availability: Availability | undefined;
    let availabilityError: Error | undefined;
    let activityController: AbortController | undefined;
    let activityTimer: number | undefined;
    let availabilityOnlineListener: (() => void) | undefined;
    const availabilityWaiters = new Set<() => void>();
    let availabilityGeneration = 0;
    let reporter: SessionReporter | undefined;
    const note = ref('');
    const burned = ref(false);
    const isBurning = ref(false);
    const isUnlocking = ref(false);
    let worker: Worker | undefined;
    let controller: AbortController | undefined;
    let activeJob = '';
    const waiters = new Map<string, Set<(reason: Error) => void>>();

    const isNote = computed(() => transfer.value?.kind === 'note');
    const isDownloading = computed(() => state.value === 'downloading');
    const passwordRequired = computed(
        () =>
            transfer.value !== undefined &&
            hasPasswordFactor(
                transfer.value.encrypted_manifest ?? transfer.value.encrypted_descriptor ?? null,
                transfer.value.protocol_version,
            ),
    );

    function wakeAvailability(): void {
        availabilityGeneration++;
        for (const wake of availabilityWaiters) wake();
    }

    function startAvailability(retry = false): void {
        if (retry) availabilityError = undefined;
        if (availabilityError) return;
        activityController?.abort();
        window.clearTimeout(activityTimer);
        const activity = new AbortController();
        activityController = activity;
        let failures = 0;
        const poll = async () => {
            if (activity.signal.aborted) return;
            let retryAfter = 0;
            try {
                const response = await fetch(transferProgress.url(transferId), {
                    cache: 'no-store',
                    signal: AbortSignal.any([activity.signal, AbortSignal.timeout(8_000)]),
                });
                if (activity.signal.aborted || activityController !== activity) return;
                if (response.status === 404) {
                    availabilityError = new Error(
                        'The sender cancelled this transfer, or it has expired.',
                    );
                    uploaderStatus.value = 'unavailable';
                    if (!isDownloading.value) error.value = availabilityError.message;
                    wakeAvailability();
                    return;
                }
                retryAfter = retryAfterMilliseconds(response.headers.get('Retry-After')) ?? 0;
                if (!response.ok) throw new Error('Unable to check upload progress.');
                const payload = (await response.json()) as { data: Availability };
                if (activity.signal.aborted || activityController !== activity) return;
                if (
                    !payload?.data ||
                    !['pending', 'available'].includes(payload.data.status) ||
                    !Number.isFinite(payload.data.progress) ||
                    !['uploading', 'stalled', 'completed', 'unavailable'].includes(
                        payload.data.uploader_status,
                    ) ||
                    !Array.isArray(payload.data.items) ||
                    payload.data.items.some(
                        (item) =>
                            typeof item.id !== 'string' ||
                            !Number.isSafeInteger(item.ready_chunks) ||
                            item.ready_chunks < 0 ||
                            !Number.isSafeInteger(item.uploaded_chunks) ||
                            item.uploaded_chunks < 0,
                    )
                )
                    throw new Error('Invalid upload progress response.');
                availability = payload.data;
                uploadProgress.value = Math.max(uploadProgress.value, payload.data.progress);
                uploaderStatus.value = payload.data.uploader_status;
                if (transfer.value) transfer.value.expires_at = payload.data.expires_at;
                failures = 0;
            } catch {
                if (activity.signal.aborted) return;
                // Transient progress failures must not strand a Turbo receiver.
                uploaderStatus.value = 'unavailable';
                failures++;
            } finally {
                if (!activity.signal.aborted && activityController === activity) {
                    wakeAvailability();
                    if (!availabilityError && availability?.status !== 'available')
                        activityTimer = window.setTimeout(
                            poll,
                            Math.max(
                                retryAfter,
                                Math.min(
                                    15_000,
                                    (isDownloading.value ? 1_000 : 2_000) * 2 ** failures,
                                ),
                            ) +
                                Math.random() * 250,
                        );
                }
            }
        };
        void poll();
    }

    function nextAvailability(signal: AbortSignal, predicate: () => boolean): Promise<void> {
        signal.throwIfAborted();
        if (predicate()) return Promise.resolve();
        const generation = availabilityGeneration;
        return new Promise((resolve, reject) => {
            const cleanup = () => {
                availabilityWaiters.delete(wake);
                signal.removeEventListener('abort', abort);
            };
            const wake = () => {
                cleanup();
                resolve();
            };
            const abort = () => {
                cleanup();
                reject(new DOMException('Download cancelled.', 'AbortError'));
            };
            availabilityWaiters.add(wake);
            signal.addEventListener('abort', abort, { once: true });
            // Register first, then synchronously recheck so a completed poll cannot be missed.
            if (predicate() || availabilityGeneration !== generation) wake();
        });
    }

    function nextAvailabilityGeneration(signal: AbortSignal): Promise<void> {
        const generation = availabilityGeneration;
        return nextAvailability(signal, () =>
            Boolean(availabilityError || availabilityGeneration !== generation),
        );
    }

    async function waitUntilAvailable(jobId: string, itemId?: string, index = 0): Promise<void> {
        while (turbo.value && transfer.value?.status === 'pending') {
            ensureActive(jobId);
            if (availabilityError) throw availabilityError;
            const ready = () =>
                Boolean(availability?.status === 'available') ||
                Boolean(
                    itemId &&
                    (availability?.items.find((item) => item.id === itemId)?.ready_chunks ?? 0) >
                        index,
                );
            if (ready()) return;
            await nextAvailability(controller!.signal, () => ready() || Boolean(availabilityError));
            // A wake can be caused by a transient polling failure; retain the loop and its predicate.
        }
        ensureActive(jobId);
    }

    async function reportSession(
        session: SessionReporter,
        status: DownloadPhase | 'cancelled' | 'error',
        terminal = false,
    ): Promise<void> {
        const sequence = ++session.sequence;
        if (!session.terminal)
            session.progress = status === 'completed' ? 100 : Math.min(99, progress.value);
        if (terminal) {
            session.terminal = status as 'completed' | 'cancelled' | 'error';
            window.clearTimeout(session.timer);
            session.controller.abort();
            if (reporter === session) reporter = undefined;
        }
        if (!session.id || !session.token) return;
        try {
            await fetch(updateSession.url({ transfer: transferId, session: session.id }), {
                method: 'PATCH',
                headers: {
                    'Content-Type': 'application/json',
                    'X-Filebeam-Session-Token': session.token,
                },
                body: JSON.stringify({ sequence, progress: session.progress, status }),
                signal: terminal
                    ? AbortSignal.timeout(5_000)
                    : AbortSignal.any([session.controller.signal, AbortSignal.timeout(5_000)]),
                keepalive: terminal,
            });
        } catch {
            // Telemetry must never interrupt a verified file transfer.
        }
        if (!terminal && reporter === session && !session.controller.signal.aborted)
            session.timer = window.setTimeout(
                () => reportSession(session, downloadPhase.value),
                2_000 + Math.random() * 250,
            );
    }

    async function startSession(
        items: ManifestItem[],
        jobId: string,
        session: SessionReporter,
    ): Promise<void> {
        try {
            const response = await fetch(createSession.url(transferId), {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ item_ids: items.map((item) => item.id) }),
                signal: AbortSignal.timeout(5_000),
            });
            if (!response.ok) return;
            const payload = (await response.json()) as { data: { id: string; token: string } };
            session.id = payload.data.id;
            session.token = payload.data.token;
            if (session.terminal) {
                void reportSession(session, session.terminal, true);
                return;
            }
            if (activeJob !== jobId || controller?.signal.aborted || reporter !== session) {
                void reportSession(session, 'cancelled', true);
                return;
            }
            void reportSession(session, downloadPhase.value);
        } catch {
            // Saving still works when anonymous monitoring is unavailable.
        }
    }

    function clearMasterKey(): void {
        masterKey.value?.fill(0);
        masterKey.value = undefined;
    }

    function discardWorker(): void {
        worker?.terminate();
        worker = undefined;
    }

    function getWorker(): Worker {
        worker ??= new Worker(
            new URL(
                '../../../backend/resources/js/workers/filebeam-crypto.worker.ts',
                import.meta.url,
            ),
            { type: 'module' },
        );
        return worker;
    }

    function waitFor(type: string, jobId: string, requestId: string): Promise<WorkerMessage> {
        const activeWorker = getWorker();
        const pendingRequest = waitForWorkerMessage(
            activeWorker,
            (message): message is WorkerMessage =>
                message.jobId === jobId &&
                message.requestId === requestId &&
                (message.type === 'error' || message.type === type),
            {
                timeoutMs: 120_000,
                timeoutMessage: 'Decryption is taking too long. Please try again.',
                workerErrorMessage: 'The encryption worker could not start. Refresh and try again.',
                messageErrorMessage: 'The encryption worker returned unreadable data.',
                onWorkerError: () => discardWorker(),
            },
        );
        const rejecter = pendingRequest.reject;
        const pending = waiters.get(jobId) ?? new Set<(reason: Error) => void>();
        pending.add(rejecter);
        waiters.set(jobId, pending);
        const cleanup = () => {
            pending.delete(rejecter);
            if (!pending.size) waiters.delete(jobId);
        };
        void pendingRequest.promise.then(cleanup, cleanup);
        return pendingRequest.promise.then((message) => {
            if (message.type === 'error') throw new Error(String(message.message));

            return message;
        });
    }

    function ensureActive(jobId: string): void {
        if (activeJob !== jobId || (state.value === 'downloading' && controller?.signal.aborted))
            throw new Error('Download cancelled.');
    }

    async function load(): Promise<void> {
        error.value = '';
        try {
            const response = await fetch(
                inbox ? `/account/inbox/${transferId}/metadata` : `/api/v1/transfers/${transferId}`,
                {
                    headers: { Accept: 'application/json' },
                    cache: 'no-store',
                },
            );
            if (!response.ok || response.redirected)
                throw new Error(
                    response.status === 404
                        ? 'This transfer is unavailable or has expired.'
                        : 'Unable to load this transfer.',
                );
            const payload = (await response.json()) as { data: Transfer };
            if (
                payload.data.id !== transferId ||
                payload.data.protocol_version !== 1 ||
                !['files', 'note'].includes(payload.data.kind)
            )
                throw new Error('Unsupported transfer protocol.');
            transfer.value = payload.data;
            if (turbo.value) {
                if (payload.data.status === 'pending') {
                    startAvailability();
                    availabilityOnlineListener ??= () => {
                        if (turbo.value && transfer.value?.status === 'pending')
                            startAvailability();
                    };
                    window.addEventListener('online', availabilityOnlineListener);
                } else {
                    uploadProgress.value = 100;
                    uploaderStatus.value = 'completed';
                }
            }
            if (inbox) {
                if (
                    !payload.data.recipient_key ||
                    payload.data.kind !== 'files' ||
                    payload.data.protocol_version !== 1
                )
                    throw new Error('This inbox transfer is missing its recipient key.');
                state.value = 'key';
                return;
            }
            const fragment = new URLSearchParams(window.location.hash.slice(1)).get('k');
            const expectedPrefix = 'v1.';
            if (
                !hasPasswordFactor(
                    payload.data.encrypted_manifest ?? payload.data.encrypted_descriptor ?? null,
                    payload.data.protocol_version,
                ) &&
                fragment?.startsWith(expectedPrefix)
            )
                await unlock(fragment.slice(expectedPrefix.length), '');
            else state.value = 'key';
        } catch (reason) {
            state.value = 'error';
            error.value =
                reason instanceof Error ? reason.message : 'Unable to load this transfer.';
        }
    }

    async function unlock(key: string, password: string): Promise<void> {
        if (!transfer.value || isUnlocking.value || isDownloading.value) return;
        error.value = '';
        isUnlocking.value = true;
        const jobId = crypto.randomUUID();
        activeJob = jobId;
        let decryptedKey: Uint8Array | undefined;
        try {
            const descriptor = turbo.value && transfer.value.status === 'pending';
            const requestId = crypto.randomUUID();
            const result = waitFor(
                descriptor ? 'descriptor-manifest' : 'manifest',
                jobId,
                requestId,
            );
            getWorker().postMessage({
                type: descriptor ? 'decrypt-descriptor' : 'decrypt-manifest',
                jobId,
                requestId,
                transferId: transfer.value.id,
                ...(descriptor
                    ? { encryptedDescriptor: transfer.value.encrypted_descriptor }
                    : { encryptedManifest: transfer.value.encrypted_manifest }),
                protocolVersion: transfer.value.protocol_version,
                key: normalizeKey(key),
                password: password || undefined,
            });
            const decrypted = await result;
            ensureActive(jobId);
            decryptedKey = new Uint8Array(decrypted.masterKey as ArrayBuffer);
            const decryptedManifest = validateManifest(
                JSON.parse(String(decrypted.manifest)),
                transfer.value,
                descriptor,
            );
            clearMasterKey();
            masterKey.value = decryptedKey;
            decryptedKey = undefined;
            manifest.value = decryptedManifest;
            earlyManifest = descriptor ? decryptedManifest : undefined;
            state.value = 'ready';
        } catch (reason) {
            if (activeJob === jobId) {
                decryptedKey?.fill(0);
                clearMasterKey();
                manifest.value = undefined;
                error.value =
                    reason instanceof Error
                        ? reason.message
                        : 'The key or password could not decrypt this transfer.';
                state.value = 'key';
            }
        } finally {
            if (activeJob === jobId) activeJob = '';
            isUnlocking.value = false;
        }
    }

    async function fetchChunk(
        itemId: string,
        index: number,
        expectedBytes: number,
        jobId: string,
        onProgress: (loaded: number) => void,
        onRetry: () => void,
        onReady?: () => void,
        onWaiting?: () => void,
    ): Promise<ArrayBuffer> {
        if (!transfer.value) throw new Error('Transfer unavailable.');
        while (true) {
            await waitUntilAvailable(jobId, itemId, index);
            onReady?.();
            const result = await readChunkWithRetry(
                `${inbox ? '/account/inbox' : '/api/v1/transfers'}/${transfer.value.id}/items/${itemId}/chunks/${index}`,
                {},
                controller!.signal,
                expectedBytes,
                onProgress,
                onRetry,
            );
            if (result !== null) return result;
            onWaiting?.();
            if (
                !turbo.value ||
                transfer.value.status !== 'pending' ||
                availability?.status === 'available'
            )
                throw new Error('The completed transfer is missing a chunk.');
            // A 202 can contradict stale readiness. Wait for a new poll, never re-GET immediately.
            await nextAvailabilityGeneration(controller!.signal);
            if (availabilityError) throw availabilityError;
        }
    }

    async function finalManifest(jobId: string): Promise<Manifest> {
        if (!earlyManifest) return manifest.value!;
        downloadPhase.value = 'verifying';
        await waitUntilAvailable(jobId);
        const payload = await fetchChunkWithRetry(
            `/api/v1/transfers/${transferId}`,
            {},
            controller!.signal,
            (response) => response.json() as Promise<{ data: Transfer }>,
        );
        ensureActive(jobId);
        const completed = payload.data;
        if (
            completed.id !== transferId ||
            completed.status !== 'available' ||
            completed.protocol_version !== transfer.value!.protocol_version ||
            completed.chunk_bytes !== earlyManifest.chunk_bytes
        )
            throw new Error('The final transfer metadata does not match the upload.');
        const requestId = crypto.randomUUID();
        const result = waitFor('manifest', jobId, requestId);
        getWorker().postMessage({
            type: 'decrypt-manifest',
            jobId,
            requestId,
            transferId,
            protocolVersion: completed.protocol_version,
            encryptedManifest: completed.encrypted_manifest,
            masterKey: masterKey.value,
        });
        const decrypted = await result;
        ensureActive(jobId);
        const final = validateManifest(JSON.parse(String(decrypted.manifest)), completed);
        if (
            final.items.length !== earlyManifest.items.length ||
            earlyManifest.items.some((item, index) => {
                const saved = final.items[index];
                return (
                    !saved ||
                    saved.id !== item.id ||
                    saved.name !== item.name ||
                    saved.type !== item.type ||
                    saved.size !== item.size ||
                    saved.nonce_prefix !== item.nonce_prefix ||
                    saved.chunk_count !== item.chunk_count
                );
            })
        )
            throw new Error('The final manifest differs from the shared file metadata.');
        transfer.value = completed;
        manifest.value = final;
        earlyManifest = undefined;
        return final;
    }

    async function decryptChunk(
        item: ManifestItem,
        index: number,
        ciphertext: ArrayBuffer,
        jobId: string,
    ): Promise<Uint8Array> {
        ensureActive(jobId);
        if (!transfer.value || !masterKey.value) throw new Error('The transfer is locked.');
        const requestId = crypto.randomUUID();
        const result = waitFor('plaintext', jobId, requestId);
        getWorker().postMessage(
            {
                type: 'decrypt-chunk',
                jobId,
                requestId,
                transferId: transfer.value.id,
                itemId: item.id,
                protocolVersion: transfer.value.protocol_version,
                prefix: decodeBase64Url(item.nonce_prefix),
                index,
                ciphertext: new Uint8Array(ciphertext),
                masterKey: masterKey.value,
            },
            [ciphertext],
        );
        const plaintext = await result;
        ensureActive(jobId);
        return new Uint8Array(plaintext.plaintext as ArrayBuffer);
    }

    async function hash(
        type: 'hash-start' | 'hash-update' | 'hash-finalize',
        jobId: string,
        hashId: string,
        bytes?: Uint8Array,
    ): Promise<WorkerMessage> {
        ensureActive(jobId);
        const requestId = crypto.randomUUID();
        const expected =
            type === 'hash-start'
                ? 'hash-started'
                : type === 'hash-update'
                  ? 'hash-updated'
                  : 'hash-finalized';
        const result = waitFor(expected, jobId, requestId);
        getWorker().postMessage(
            { type, jobId, requestId, hashId, ...(bytes ? { bytes } : {}) },
            bytes ? [bytes.buffer as ArrayBuffer] : [],
        );
        return result;
    }

    async function decryptItem(
        item: ManifestItem,
        jobId: string,
        onChunk: (plaintext: Uint8Array) => Promise<void> | void,
        onProgress?: (bytes: number) => void,
    ): Promise<void> {
        if (!transfer.value?.chunk_bytes) throw new Error('Transfer unavailable.');
        const downloadController = controller!;
        const hashId = crypto.randomUUID();
        const verifyDigest = Boolean(item.digest || turbo.value);
        if (verifyDigest) await hash('hash-start', jobId, hashId);
        const maximum = transferConcurrency(
            transfer.value.download_concurrency,
            transfer.value.chunk_bytes,
        );
        const pending = new Map<number, Promise<{ plaintext?: Uint8Array; reason?: unknown }>>();
        let pipelineFailure: unknown;
        let nextFetch = 0;
        let written = 0;
        const buffered = new Map<number, number>();
        const reportProgress = () => {
            if (activeJob !== jobId || controller?.signal.aborted) return;
            onProgress?.(
                written + [...buffered.values()].reduce((total, bytes) => total + bytes, 0),
            );
        };
        const ready = (index: number) => {
            if (!turbo.value || transfer.value?.status !== 'pending') return true;
            if (!availability) return index === 0 && pending.size === 0;
            if (availability.status === 'available') return true;
            return (
                (availability.items.find((entry) => entry.id === item.id)?.ready_chunks ?? 0) >
                index
            );
        };
        let queue = () => undefined;
        const concurrency = new AdaptiveConcurrency(maximum, () => queue());
        queue = () => {
            while (
                nextFetch < item.chunk_count &&
                pending.size < concurrency.limit &&
                ready(nextFetch)
            ) {
                const index = nextFetch++;
                const expected =
                    index === item.chunk_count - 1
                        ? item.size - index * transfer.value!.chunk_bytes!
                        : transfer.value!.chunk_bytes!;
                const expectedCiphertext = expected + 16;
                const sampleKey = `${item.id}:${index}`;
                let requestStartedAt = 0;
                buffered.set(index, 0);
                const task = fetchChunk(
                    item.id,
                    index,
                    expectedCiphertext,
                    jobId,
                    (loaded) => {
                        if (activeJob !== jobId || controller?.signal.aborted) return;
                        concurrency.sample(sampleKey, loaded);
                        buffered.set(
                            index,
                            Math.min(expected, (loaded / expectedCiphertext) * expected),
                        );
                        reportProgress();
                    },
                    () => {
                        concurrency.forget(sampleKey);
                        concurrency.congested();
                        if (activeJob !== jobId || controller?.signal.aborted) return;
                        buffered.set(index, 0);
                        reportProgress();
                    },
                    () => {
                        requestStartedAt = performance.now();
                        concurrency.sample(sampleKey, 0);
                        queue();
                    },
                    () => concurrency.forget(sampleKey),
                )
                    .then(async (ciphertext) => {
                        concurrency.forget(sampleKey);
                        if (requestStartedAt)
                            concurrency.observe(
                                expectedCiphertext,
                                performance.now() - requestStartedAt,
                            );
                        if (ciphertext.byteLength !== expectedCiphertext)
                            throw new Error('The download ciphertext was truncated.');
                        const plaintext = await decryptChunk(item, index, ciphertext, jobId);
                        if (plaintext.byteLength !== expected)
                            throw new Error('The download was truncated or altered.');
                        return plaintext;
                    })
                    .then(
                        (plaintext) => ({ plaintext }),
                        (reason) => {
                            concurrency.forget(sampleKey);
                            pipelineFailure ??= reason;
                            downloadController.abort(reason);
                            return { reason };
                        },
                    );
                pending.set(index, task);
            }
        };
        try {
            queue();
            for (let index = 0; index < item.chunk_count; index++) {
                if (pipelineFailure) throw pipelineFailure;
                ensureActive(jobId);
                if (!pending.has(index)) {
                    downloadPhase.value = 'waiting';
                    await waitUntilAvailable(jobId, item.id, index);
                    queue();
                }
                downloadPhase.value =
                    turbo.value &&
                    transfer.value?.status === 'pending' &&
                    availability?.status !== 'available' &&
                    (availability?.items.find((entry) => entry.id === item.id)?.ready_chunks ??
                        0) <= index
                        ? 'waiting'
                        : 'downloading';
                const result = await pending.get(index)!;
                if (result.reason) throw result.reason;
                let plaintext = result.plaintext!;
                downloadPhase.value = 'downloading';
                if (verifyDigest) {
                    const hashed = await hash('hash-update', jobId, hashId, plaintext);
                    plaintext = new Uint8Array(hashed.bytes as ArrayBuffer);
                }
                await onChunk(plaintext);
                written += plaintext.byteLength;
                buffered.delete(index);
                reportProgress();
                ensureActive(jobId);
                // A task remains counted until its sequential write bounds ready plaintext buffers.
                pending.delete(index);
                queue();
            }
            if (verifyDigest) {
                const final = await finalManifest(jobId);
                downloadPhase.value = 'verifying';
                const expectedDigest = final.items.find((entry) => entry.id === item.id)?.digest;
                const result = await hash('hash-finalize', jobId, hashId);
                const actual = [...new Uint8Array(result.digest as ArrayBuffer)]
                    .map((byte) => byte.toString(16).padStart(2, '0'))
                    .join('');
                if (!expectedDigest || actual !== expectedDigest.value)
                    throw new Error('The downloaded file failed its integrity check.');
            }
        } catch (reason) {
            downloadController.abort(reason);
            await Promise.all(pending.values());
            if (verifyDigest && worker)
                worker.postMessage({
                    type: 'hash-free',
                    jobId,
                    requestId: crypto.randomUUID(),
                    hashId,
                });
            throw pipelineFailure ?? reason;
        }
    }

    function saveBlob(blob: Blob, name: string): void {
        const anchor = document.createElement('a');
        const url = URL.createObjectURL(blob);
        anchor.href = url;
        anchor.download = safeFilename(name);
        anchor.click();
        window.setTimeout(() => URL.revokeObjectURL(url), 0);
    }

    async function decryptNote(): Promise<void> {
        if (!transfer.value || !manifest.value?.items[0] || isDownloading.value) return;
        error.value = '';
        const jobId = crypto.randomUUID();
        activeJob = jobId;
        controller = new AbortController();
        state.value = 'downloading';
        progress.value = 0;
        const item = manifest.value.items[0];
        try {
            const chunks: Uint8Array[] = [];
            await decryptItem(
                item,
                jobId,
                (plaintext) => {
                    chunks.push(plaintext);
                },
                (bytes) => {
                    progress.value = Math.max(
                        progress.value,
                        item.size ? Math.min(99, (bytes / item.size) * 100) : 99,
                    );
                },
            );
            ensureActive(jobId);
            note.value = new TextDecoder('utf-8', { fatal: true }).decode(
                await new Blob(chunks.map((chunk) => chunk.slice().buffer)).arrayBuffer(),
            );
            ensureActive(jobId);
            if (manifest.value.read_token) await burnNote();
            progress.value = 100;
            state.value = 'ready';
        } catch (reason) {
            if (
                activeJob === jobId &&
                !(reason instanceof DOMException && reason.name === 'AbortError')
            )
                error.value =
                    reason instanceof Error ? reason.message : 'Could not decrypt this note.';
            if (activeJob === jobId) state.value = 'ready';
        } finally {
            if (activeJob === jobId) activeJob = '';
        }
    }

    function downloadNote(): void {
        if (!manifest.value?.items[0] || !note.value) return;
        saveBlob(
            new Blob([note.value], { type: 'application/octet-stream' }),
            manifest.value.items[0].name,
        );
    }

    async function burnNote(): Promise<void> {
        if (!note.value || !manifest.value?.read_token || burned.value || isBurning.value) return;
        isBurning.value = true;
        try {
            const response = await fetch(`/api/v1/transfers/${transferId}/consume`, {
                method: 'POST',
                headers: { 'X-Filebeam-Read-Token': manifest.value.read_token },
            });
            if (!response.ok && response.status !== 404)
                throw new Error('Removal could not be confirmed.');
            burned.value = true;
            error.value = '';
        } catch {
            error.value =
                'The note decrypted, but removal could not be confirmed. Retry removal before closing this tab.';
        } finally {
            isBurning.value = false;
        }
    }

    async function downloadFiles(items = manifest.value?.items): Promise<void> {
        if (!transfer.value || !manifest.value || !items?.length || isDownloading.value) return;
        error.value = '';
        downloadItemCount.value = items.length;
        const jobId = crypto.randomUUID();
        activeJob = jobId;
        controller = new AbortController();
        state.value = 'downloading';
        progress.value = 0;
        downloadPhase.value = 'downloading';
        if (turbo.value && transfer.value.status === 'pending') {
            startAvailability(true);
        }
        let activeWritable: WritableFile | undefined;
        let downloadSession: SessionReporter | undefined;
        try {
            const picker =
                typeof (window as Window & Partial<SavePicker>).showSaveFilePicker === 'function'
                    ? (window as unknown as Window & SavePicker)
                    : undefined;
            const blobLimit = 512 * 1024 * 1024;
            if (!picker && items.some((item) => item.size > blobLimit))
                throw new Error(
                    'This browser cannot save this large download without the File System Access API. Use a desktop browser with file save support.',
                );
            let complete = 0;
            const total = items.reduce((sum, item) => sum + item.size, 0);
            for (const item of items) {
                ensureActive(jobId);
                activeWritable = picker
                    ? await picker
                          .showSaveFilePicker({ suggestedName: safeFilename(item.name) })
                          .then((handle) => handle.createWritable())
                    : undefined;
                ensureActive(jobId);
                if (turbo.value && !downloadSession) {
                    downloadSession = {
                        sequence: 0,
                        progress: 0,
                        controller: new AbortController(),
                    };
                    reporter = downloadSession;
                    void startSession(items, jobId, downloadSession);
                }
                const chunks: Uint8Array[] = [];
                let itemWritten = 0;
                await decryptItem(
                    item,
                    jobId,
                    async (plaintext) => {
                        if (activeWritable) await activeWritable.write(plaintext);
                        else chunks.push(plaintext);
                        complete += plaintext.byteLength;
                        itemWritten += plaintext.byteLength;
                    },
                    (bytes) => {
                        const estimated = complete - itemWritten + bytes;
                        progress.value = Math.max(
                            progress.value,
                            total ? Math.min(99, (estimated / total) * 100) : 99,
                        );
                    },
                );
                ensureActive(jobId);
                if (activeWritable) {
                    await activeWritable.close();
                    activeWritable = undefined;
                } else {
                    ensureActive(jobId);
                    saveBlob(
                        new Blob(
                            chunks.map((chunk) => chunk.slice().buffer),
                            { type: 'application/octet-stream' },
                        ),
                        item.name,
                    );
                }
                ensureActive(jobId);
                if (!downloadedItemIds.value.includes(item.id))
                    downloadedItemIds.value.push(item.id);
            }
            progress.value = 100;
            downloadPhase.value = 'completed';
            if (downloadSession) void reportSession(downloadSession, 'completed', true);
            state.value = 'ready';
        } catch (reason) {
            if (activeWritable) await activeWritable.abort().catch(() => undefined);
            if (downloadSession && reporter === downloadSession)
                void reportSession(
                    downloadSession,
                    reason instanceof DOMException && reason.name === 'AbortError'
                        ? 'cancelled'
                        : 'error',
                    true,
                );
            if (
                activeJob === jobId &&
                !(reason instanceof DOMException && reason.name === 'AbortError')
            )
                error.value = reason instanceof Error ? reason.message : 'Download failed.';
            if (activeJob === jobId) state.value = 'ready';
        } finally {
            if (activeJob === jobId) activeJob = '';
        }
    }

    function cancel(): void {
        if (reporter) void reportSession(reporter, 'cancelled', true);
        const jobId = activeJob;
        activeJob = '';
        controller?.abort();
        for (const reject of waiters.get(jobId) ?? []) reject(new Error('Download cancelled.'));
        discardWorker();
        if (state.value === 'downloading') state.value = 'ready';
    }

    onBeforeUnmount(() => {
        cancel();
        activityController?.abort();
        window.clearTimeout(activityTimer);
        if (availabilityOnlineListener)
            window.removeEventListener('online', availabilityOnlineListener);
        clearMasterKey();
        discardWorker();
    });
    return {
        transfer,
        manifest,
        state,
        error,
        progress,
        turbo,
        uploadProgress,
        uploaderStatus,
        downloadPhase,
        downloadItemCount,
        downloadedItemIds,
        note,
        burned,
        isBurning,
        burnNote,
        isNote,
        passwordRequired,
        isUnlocking,
        isDownloading,
        load,
        unlock,
        decryptNote,
        downloadNote,
        downloadFiles,
        cancel,
    };
}
