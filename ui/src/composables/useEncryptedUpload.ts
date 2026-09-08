import { computed, onBeforeUnmount, ref } from 'vue';
import type { FilebeamConfig, PublicRecipient } from '../types';
import { sealRecipientKey } from '../lib/account-crypto';
import type { DownloadSession, ShareResult, TransferMode, UploadEntry } from '../upload-types';
import {
    descriptor,
    heartbeat,
    monitor,
} from '../../../backend/resources/js/actions/App/Http/Controllers/Api/V1/TurboTransferController';
import { ciphertextBytes } from '../lib/format';
import { fetchChunkWithRetry, transferConcurrency } from '../lib/transfer';
import { waitForWorkerMessage, type WorkerMessage } from '../lib/worker-request';

type ServerTransfer = {
    id: string;
    share_url: string;
    chunk_bytes: number;
    upload_token: string;
    delete_token: string;
    monitor_token?: string;
    expires_at?: string;
    read_token?: string;
    items: Array<{ id: string; position: number }>;
};
const MAX_CIPHERTEXT_CHUNK_BYTES = 25_000_000;
const MAX_PLAINTEXT_CHUNK_BYTES = MAX_CIPHERTEXT_CHUNK_BYTES - 16;

function fileType(file: File): string {
    return file.type || 'application/octet-stream';
}

export function useEncryptedUpload(config: FilebeamConfig) {
    const entries = ref<UploadEntry[]>([]);
    const status = ref<'ready' | 'uploading' | 'complete' | 'error'>('ready');
    const progress = ref(0);
    const error = ref('');
    const share = ref<ShareResult>();
    const sessions = ref<DownloadSession[]>([]);
    const monitoringUnavailable = ref(false);
    let monitorController: AbortController | undefined;
    let monitorTimer: number | undefined;
    let turboReservation: ServerTransfer | undefined;
    let controller: AbortController | undefined;
    let worker: Worker | undefined;
    let activeJob = '';
    const waiters = new Map<string, Set<(reason: Error) => void>>();

    const isUploading = computed(() => status.value === 'uploading');
    const totalBytes = computed(() =>
        entries.value.reduce((total, entry) => total + entry.file.size, 0),
    );
    const totalCiphertextBytes = computed(() =>
        entries.value.reduce(
            (total, entry) => total + ciphertextBytes(entry.file.size, config.chunk_bytes),
            0,
        ),
    );

    function stopMonitoring(): void {
        monitorController?.abort();
        window.clearTimeout(monitorTimer);
        monitorController = undefined;
    }

    function startMonitoring(reservation: ServerTransfer): void {
        stopMonitoring();
        const activityController = new AbortController();
        monitorController = activityController;
        let failures = 0;
        let lastPulse = 0;
        const poll = async () => {
            if (activityController.signal.aborted) return;
            try {
                if (isUploading.value && Date.now() - lastPulse >= 10_000) {
                    lastPulse = Date.now();
                    void fetch(heartbeat.url(reservation.id), {
                        method: 'PATCH',
                        headers: { 'X-Filebeam-Upload-Token': reservation.upload_token },
                        signal: AbortSignal.any([
                            activityController.signal,
                            AbortSignal.timeout(8_000),
                        ]),
                    }).catch(() => undefined);
                }
                const response = await fetch(monitor.url(reservation.id), {
                    headers: { 'X-Filebeam-Monitor-Token': reservation.monitor_token! },
                    cache: 'no-store',
                    signal: AbortSignal.any([
                        activityController.signal,
                        AbortSignal.timeout(8_000),
                    ]),
                });
                if (response.status === 404 || response.status === 403) {
                    monitoringUnavailable.value = true;
                    stopMonitoring();
                    return;
                }
                if (!response.ok) throw new Error('Monitoring unavailable.');
                const payload = (await response.json()) as {
                    data: { sessions: DownloadSession[] };
                };
                if (activityController.signal.aborted) return;
                sessions.value = payload.data.sessions;
                monitoringUnavailable.value = false;
                failures = 0;
            } catch {
                if (activityController.signal.aborted) return;
                monitoringUnavailable.value = true;
                failures++;
            }
            if (!activityController.signal.aborted)
                monitorTimer = window.setTimeout(
                    poll,
                    Math.min(15_000, 2_000 * 2 ** failures) + Math.random() * 250,
                );
        };
        void poll();
    }

    function revokeTurbo(): void {
        const reservation = turboReservation;
        turboReservation = undefined;
        stopMonitoring();
        sessions.value = [];
        share.value = undefined;
        if (reservation)
            void fetch(`/api/v1/transfers/${reservation.id}`, {
                method: 'DELETE',
                headers: { 'X-Filebeam-Delete-Token': reservation.delete_token },
                signal: AbortSignal.timeout(10_000),
                keepalive: true,
            }).catch(() => undefined);
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

    function discardWorker(expectedWorker?: Worker): void {
        if (expectedWorker && worker !== expectedWorker) return;
        worker?.terminate();
        worker = undefined;
    }

    function cancelActiveUpload(reason = 'Upload cancelled.'): void {
        activeJob = '';
        controller?.abort();
        for (const rejecters of waiters.values())
            for (const reject of rejecters) reject(new Error(reason));
        discardWorker();
    }

    function waitFor(
        type: string,
        jobId: string,
        requestId: string,
        timeoutMs?: number,
    ): Promise<WorkerMessage> {
        const activeWorker = getWorker();
        const pendingRequest = waitForWorkerMessage(
            activeWorker,
            (message): message is WorkerMessage =>
                message.jobId === jobId &&
                message.requestId === requestId &&
                message.type !== 'chunk' &&
                (message.type === 'error' || message.type === type),
            {
                timeoutMs,
                timeoutMessage: 'Encryption is taking too long. Please try again.',
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
        if (activeJob !== jobId) throw new Error('Upload cancelled.');
    }

    async function request<T>(url: string, signal: AbortSignal, init?: RequestInit): Promise<T> {
        const response = await fetch(url, { ...init, signal });
        if (!response.ok)
            throw new Error(
                (await response.json().catch(() => null))?.message ??
                    `Request failed (${response.status}).`,
            );
        return response.json() as Promise<T>;
    }

    async function putChunk(
        transfer: ServerTransfer,
        itemId: string,
        index: number,
        ciphertext: Uint8Array,
        signal: AbortSignal,
    ): Promise<void> {
        await fetchChunkWithRetry(
            `/api/v1/transfers/${transfer.id}/items/${itemId}/chunks/${index}`,
            {
                method: 'PUT',
                body: ciphertext.buffer as ArrayBuffer,
                headers: {
                    'Content-Type': 'application/octet-stream',
                    'X-Filebeam-Upload-Token': transfer.upload_token,
                },
            },
            signal,
            async () => undefined,
        );
    }

    function addFiles(files: FileList | File[]): void {
        if (isUploading.value) return;
        const fingerprints = new Set(
            entries.value.map(
                (entry) =>
                    `${entry.name}:${entry.file.size}:${entry.file.lastModified}:${entry.type}`,
            ),
        );
        let duplicateCount = 0;
        const additions = [...files].filter((file) => {
            const fingerprint = `${file.name}:${file.size}:${file.lastModified}:${fileType(file)}`;
            if (fingerprints.has(fingerprint)) {
                duplicateCount++;
                return false;
            }
            fingerprints.add(fingerprint);
            return true;
        });
        const available = config.maximum_file_count - entries.value.length;
        if (additions.length > available)
            error.value = `Only ${Math.max(available, 0)} more file${available === 1 ? '' : 's'} can be added (maximum ${config.maximum_file_count}).`;
        else if (duplicateCount)
            error.value = `${duplicateCount} duplicate file${duplicateCount === 1 ? ' was' : 's were'} already in the queue.`;
        else error.value = '';
        const nextEntries: UploadEntry[] = additions
            .slice(0, Math.max(available, 0))
            .map((file): UploadEntry => ({
                id: crypto.randomUUID(),
                file,
                name: file.name,
                type: fileType(file),
                state: 'queued',
                progress: 0,
            }));
        entries.value.push(...nextEntries);
    }

    function removeFile(id: string): void {
        if (!isUploading.value) entries.value = entries.value.filter((entry) => entry.id !== id);
    }
    function clear(): void {
        stopMonitoring();
        sessions.value = [];
        monitoringUnavailable.value = false;
        turboReservation = undefined;
        cancelActiveUpload();
        controller = undefined;
        entries.value = [];
        error.value = '';
        status.value = 'ready';
        progress.value = 0;
        share.value = undefined;
    }

    async function upload(options: {
        mode: TransferMode;
        turbo?: boolean;
        note?: string;
        title?: string;
        language?: string;
        password: string;
        includeKey: boolean;
        retentionHours: number;
        burnOnRead: boolean;
        recipient?: PublicRecipient;
    }): Promise<void> {
        if (
            options.recipient &&
            (options.mode !== 'files' || options.password || options.burnOnRead)
        ) {
            error.value = 'Inbox deliveries use the recipient key and support files only.';
            return;
        }
        const source =
            options.mode === 'note'
                ? new File([options.note ?? ''], 'note.txt', { type: 'text/plain' })
                : undefined;
        const uploadEntries = source
            ? [
                  {
                      id: crypto.randomUUID(),
                      file: source,
                      name: source.name,
                      type: source.type,
                      state: 'queued',
                      progress: 0,
                  } satisfies UploadEntry,
              ]
            : entries.value;
        const maximum =
            options.mode === 'note' ? config.maximum_note_bytes : config.maximum_transfer_bytes;
        const encryptedSize = uploadEntries.reduce(
            (total, entry) => total + ciphertextBytes(entry.file.size, config.chunk_bytes),
            0,
        );
        if (!uploadEntries.length || encryptedSize > maximum || isUploading.value) return;
        if (
            !Number.isSafeInteger(config.chunk_bytes) ||
            config.chunk_bytes < 1 ||
            config.chunk_bytes > MAX_PLAINTEXT_CHUNK_BYTES
        ) {
            error.value = 'The server supplied an invalid encrypted chunk policy.';
            return;
        }
        if (options.password && Array.from(options.password).length < 8) {
            error.value = 'Passwords must contain at least 8 characters.';
            return;
        }
        error.value = '';
        share.value = undefined;
        sessions.value = [];
        monitoringUnavailable.value = false;
        stopMonitoring();
        const turbo = options.turbo === true && options.mode === 'files' && !options.recipient;
        status.value = 'uploading';
        progress.value = 0;
        const uploadController = new AbortController();
        controller = uploadController;
        const jobId = crypto.randomUUID();
        activeJob = jobId;
        let masterKey: Uint8Array | undefined;
        let chunkHandler: ((event: MessageEvent) => void) | undefined;
        let activeWorker: Worker | undefined;
        try {
            const uploadWorker = getWorker();
            activeWorker = uploadWorker;
            const prepareRequestId = crypto.randomUUID();
            const preparedResult = waitFor('prepared', jobId, prepareRequestId, 120_000);
            uploadWorker.postMessage({
                type: 'prepare',
                jobId,
                requestId: prepareRequestId,
                password: options.password || null,
            });
            const prepared = await preparedResult;
            ensureActive(jobId);
            masterKey = new Uint8Array(prepared.masterKey as Uint8Array);
            const created = await request<{ data: ServerTransfer }>(
                '/api/v1/transfers',
                uploadController.signal,
                {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        kind: options.mode,
                        protocol_version: 1,
                        chunk_bytes: config.chunk_bytes,
                        retention_hours: options.retentionHours,
                        burn_on_read: options.mode === 'note' && options.burnOnRead,
                        ...(options.recipient
                            ? {
                                  recipient_username: options.recipient.username,
                                  account_key_bundle_id: options.recipient.account_key_bundle_id,
                              }
                            : {}),
                        items: uploadEntries.map((entry) => ({
                            ciphertext_bytes: ciphertextBytes(entry.file.size, config.chunk_bytes),
                            chunk_count: Math.ceil(entry.file.size / config.chunk_bytes) || 1,
                        })),
                    }),
                },
            );
            ensureActive(jobId);
            if (turbo) {
                turboReservation = created.data;
                if (!created.data.monitor_token) throw new Error('Turbo Transfer is unavailable.');
            }
            if (created.data.chunk_bytes !== config.chunk_bytes)
                throw new Error('The server did not apply the current encrypted chunk policy.');
            const encryptedKey = options.recipient
                ? await sealRecipientKey(
                      masterKey,
                      options.recipient.public_key,
                      created.data.id,
                      options.recipient.id,
                      options.recipient.account_key_bundle_id,
                  )
                : undefined;
            ensureActive(jobId);
            const serverItems = new Map(created.data.items.map((item) => [item.position, item]));
            if (
                serverItems.size !== uploadEntries.length ||
                [...serverItems].some(
                    ([position, item]) =>
                        !Number.isSafeInteger(position) ||
                        position < 0 ||
                        typeof item.id !== 'string' ||
                        !item.id,
                ) ||
                uploadEntries.some((_, position) => !serverItems.has(position))
            )
                throw new Error('Transfer response did not include every item.');
            const entryByItemId = new Map(
                uploadEntries.map((entry, position) => [serverItems.get(position)!.id, entry]),
            );
            if (entryByItemId.size !== uploadEntries.length)
                throw new Error('Transfer response contains duplicate item identifiers.');
            const publishShare = (expiresAt?: string) => {
                const key = `v1.${String(prepared.shareKey)}`;
                const url = new URL(created.data.share_url, window.location.origin)
                    .toString()
                    .split('#')[0];
                share.value = {
                    link: `${url}${options.includeKey ? `#k=${key}` : ''}`,
                    key,
                    deleteToken: created.data.delete_token,
                    expiresAt,
                    transferId: created.data.id,
                    includeKey: options.includeKey,
                    passwordProtected: Boolean(options.password),
                    turbo,
                };
            };
            let uploaded = 0;
            const sourceBytes = uploadEntries.reduce((total, entry) => total + entry.file.size, 0);
            const uploadedByItem = new Map<string, number>();
            chunkHandler = (event: MessageEvent) => {
                const message = event.data as WorkerMessage;
                if (activeJob !== jobId || message.jobId !== jobId) return;
                if (message.type === 'descriptor' && turbo) {
                    void (async () => {
                        try {
                            await fetchChunkWithRetry(
                                descriptor.url(created.data.id),
                                {
                                    method: 'PUT',
                                    headers: {
                                        'Content-Type': 'application/json',
                                        'X-Filebeam-Upload-Token': created.data.upload_token,
                                    },
                                    body: JSON.stringify({
                                        encrypted_descriptor: message.encryptedDescriptor,
                                    }),
                                },
                                uploadController.signal,
                                async () => undefined,
                            );
                            ensureActive(jobId);
                            publishShare(created.data.expires_at);
                            startMonitoring(created.data);
                            uploadWorker.postMessage({ type: 'uploaded', token: message.token });
                        } catch (reason) {
                            if (activeJob !== jobId) return;
                            for (const reject of waiters.get(jobId) ?? [])
                                reject(
                                    reason instanceof Error
                                        ? reason
                                        : new Error('Could not publish this transfer.'),
                                );
                            uploadController.abort();
                            discardWorker(uploadWorker);
                        }
                    })();
                    return;
                }
                if (message.type !== 'chunk') return;
                const entry = entryByItemId.get(String(message.itemId));
                if (!entry) return;
                if (entry) entry.state = 'uploading';
                void (async () => {
                    try {
                        const ciphertext =
                            message.ciphertext instanceof Uint8Array
                                ? message.ciphertext
                                : new Uint8Array(message.ciphertext as ArrayBuffer);
                        await putChunk(
                            created.data,
                            String(message.itemId),
                            Number(message.index),
                            ciphertext,
                            uploadController.signal,
                        );
                        ensureActive(jobId);
                        uploaded += ciphertext.byteLength - 16;
                        progress.value = sourceBytes
                            ? Math.min(99, Math.round((uploaded / sourceBytes) * 100))
                            : 99;
                        const itemId = String(message.itemId);
                        const itemUploaded =
                            (uploadedByItem.get(itemId) ?? 0) + ciphertext.byteLength - 16;
                        uploadedByItem.set(itemId, itemUploaded);
                        entry.progress = entry.file.size
                            ? Math.min(99, Math.round((itemUploaded / entry.file.size) * 100))
                            : 99;
                        if (itemUploaded === entry.file.size) {
                            entry.state = 'complete';
                            const nextEntry = uploadEntries[uploadEntries.indexOf(entry) + 1];
                            if (nextEntry?.state === 'queued') nextEntry.state = 'encrypting';
                        }
                        uploadWorker.postMessage({ type: 'uploaded', token: message.token });
                    } catch (reason) {
                        if (activeJob !== jobId) return;
                        entry.state = 'error';
                        entry.error =
                            reason instanceof Error ? reason.message : 'Chunk upload failed.';
                        for (const reject of waiters.get(jobId) ?? [])
                            reject(
                                reason instanceof Error
                                    ? reason
                                    : new Error('Chunk upload failed.'),
                            );
                        uploadController.abort();
                        discardWorker(uploadWorker);
                    }
                })();
            };
            uploadWorker.addEventListener('message', chunkHandler);
            const encryptRequestId = crypto.randomUUID();
            const finishedResult = waitFor('finished', jobId, encryptRequestId);
            if (uploadEntries[0]) uploadEntries[0].state = 'encrypting';
            uploadWorker.postMessage({
                type: 'encrypt',
                jobId,
                requestId: encryptRequestId,
                transferId: created.data.id,
                serverItems: created.data.items,
                items: uploadEntries.map(({ file, name, type }) => ({ file, name, type })),
                chunkBytes: created.data.chunk_bytes,
                uploadConcurrency: transferConcurrency(
                    config.upload_concurrency,
                    created.data.chunk_bytes,
                ),
                masterKey,
                protocolVersion: 1,
                turbo,
                salt: prepared.salt,
                noteLanguage: options.mode === 'note' ? options.language : null,
                noteTitle: options.mode === 'note' ? options.title : undefined,
                readToken: created.data.read_token,
            });
            const finished = await finishedResult;
            ensureActive(jobId);
            const completed = await request<{ data: ServerTransfer }>(
                `/api/v1/transfers/${created.data.id}/complete`,
                uploadController.signal,
                {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'X-Filebeam-Upload-Token': created.data.upload_token,
                    },
                    body: JSON.stringify({
                        encrypted_manifest: finished.encryptedManifest,
                        ...(encryptedKey ? { encrypted_key: encryptedKey } : {}),
                    }),
                },
            );
            ensureActive(jobId);
            if (!options.recipient) {
                publishShare(completed.data.expires_at);
            }
            turboReservation = undefined;
            for (const entry of uploadEntries) entry.progress = 100;
            progress.value = 100;
            status.value = 'complete';
            activeJob = '';
        } catch (reason) {
            if (activeJob === jobId) {
                uploadController.abort();
                discardWorker(activeWorker);
                if (turbo) revokeTurbo();
                status.value = 'error';
                error.value = reason instanceof Error ? reason.message : 'Upload failed.';
                activeJob = '';
            }
        } finally {
            masterKey?.fill(0);
            if (chunkHandler) activeWorker?.removeEventListener('message', chunkHandler);
            discardWorker(activeWorker);
            waiters.delete(jobId);
            if (controller === uploadController) controller = undefined;
        }
    }

    function cancel(): void {
        const wasTurbo = Boolean(turboReservation);
        if (wasTurbo) revokeTurbo();
        cancelActiveUpload();
        status.value = 'ready';
        error.value = wasTurbo
            ? 'Turbo Transfer cancelled. Link removal requested; any remaining encrypted data will expire automatically.'
            : 'Upload cancelled. Encrypted data may remain until it expires.';
    }

    onBeforeUnmount(() => {
        if (turboReservation) revokeTurbo();
        stopMonitoring();
        cancelActiveUpload();
    });
    return {
        entries,
        status,
        progress,
        error,
        share,
        sessions,
        monitoringUnavailable,
        isUploading,
        totalBytes,
        totalCiphertextBytes,
        addFiles,
        removeFile,
        clear,
        upload,
        cancel,
    };
}
