import {
    AdaptiveConcurrency,
    retryAfterMilliseconds,
    retryDelayMilliseconds,
    retryableStatus,
    transferWait,
} from './transfer';
import * as transportPolicy from './transfer-wasm-policy';

export type UploadTransport = {
    version: 1;
    part_min_bytes: number;
    part_max_bytes: number;
    request_target_ms: number;
    request_budget_ms: number;
    part_max_count?: number;
};
export type UploadPhase = 'uploading' | 'retrying' | 'storing' | 'offline';
type XhrResult = {
    status: number;
    data: unknown;
    retryAfter: number | undefined;
    elapsedMs: number;
};

const ATTEMPTS = 4;
const IDLE_TIMEOUT = 120_000;

export class UploadError extends Error {
    constructor(
        message: string,
        readonly status?: number,
        readonly uncertain = false,
        readonly retryAfter?: number,
    ) {
        super(message);
    }
}

async function hash(bytes: Uint8Array): Promise<string> {
    const digest = await crypto.subtle.digest('SHA-256', bytes as Uint8Array<ArrayBuffer>);
    return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

async function online(signal: AbortSignal): Promise<void> {
    signal.throwIfAborted();
    if (typeof navigator === 'undefined' || navigator.onLine !== false) return;
    // Connectivity hints can be wrong; a bounded pause also permits a probe if no event arrives.
    await new Promise<void>((resolve, reject) => {
        const finish = () => {
            cleanup();
            resolve();
        };
        const abort = () => {
            cleanup();
            reject(signal.reason);
        };
        const timer = window.setTimeout(finish, 5_000);
        function cleanup(): void {
            window.clearTimeout(timer);
            window.removeEventListener('online', finish);
            signal.removeEventListener('abort', abort);
        }
        window.addEventListener('online', finish, { once: true });
        signal.addEventListener('abort', abort, { once: true });
    });
}

/** One request; uploading and waiting for server storage have independent inactivity timers. */
export function xhrUpload(
    url: string,
    body: Document | XMLHttpRequestBodyInit | null,
    headers: Record<string, string>,
    signal: AbortSignal,
    onProgress: (loaded: number) => void,
    method = 'PUT',
    hardBudget?: number,
): Promise<XhrResult> {
    signal.throwIfAborted();
    return new Promise((resolve, reject) => {
        const xhr = new XMLHttpRequest();
        const started = performance.now();
        let settled = false;
        let loaded = 0;
        let idle: number | undefined;
        let hard: number | undefined;
        const cleanup = () => {
            window.clearTimeout(idle);
            window.clearTimeout(hard);
            signal.removeEventListener('abort', aborted);
        };
        const fail = (reason: unknown) => {
            if (settled) return;
            settled = true;
            cleanup();
            xhr.abort();
            reject(reason);
        };
        const aborted = () =>
            fail(signal.reason ?? new DOMException('Upload cancelled.', 'AbortError'));
        const touch = (message: string) => {
            window.clearTimeout(idle);
            idle = window.setTimeout(
                () => fail(new UploadError(message, undefined, true)),
                IDLE_TIMEOUT,
            );
        };
        signal.addEventListener('abort', aborted, { once: true });
        if (signal.aborted) return aborted();
        touch('Upload request stopped making progress.');
        if (hardBudget)
            hard = window.setTimeout(
                () =>
                    fail(
                        new UploadError(
                            'Upload exceeded the request transmission budget.',
                            undefined,
                            true,
                        ),
                    ),
                hardBudget,
            );
        try {
            xhr.open(method, url);
            for (const [name, value] of Object.entries(headers)) xhr.setRequestHeader(name, value);
            xhr.upload.onprogress = (event) => {
                if (settled || signal.aborted || event.loaded <= loaded) return;
                loaded = event.loaded;
                touch('Upload stopped making byte progress.');
                try {
                    onProgress(loaded);
                } catch (reason) {
                    fail(reason);
                }
            };
            xhr.upload.onload = () => {
                if (settled) return;
                window.clearTimeout(hard);
                touch('Server did not acknowledge the upload.');
            };
            xhr.onprogress = () => {
                if (!settled) touch('Server response stopped making progress.');
            };
            xhr.onload = () => {
                if (settled) return;
                settled = true;
                cleanup();
                let data: unknown;
                try {
                    data = xhr.responseText ? JSON.parse(xhr.responseText) : undefined;
                } catch {
                    // The status is still useful on non-JSON proxy error responses.
                }
                resolve({
                    status: xhr.status,
                    data,
                    retryAfter: retryAfterMilliseconds(xhr.getResponseHeader('Retry-After')),
                    elapsedMs: performance.now() - started,
                });
            };
            xhr.onerror = () =>
                fail(new UploadError('Upload network request failed.', undefined, true));
            xhr.onabort = () => {
                if (!settled)
                    fail(new UploadError('Upload request was interrupted.', undefined, true));
            };
            xhr.ontimeout = () =>
                fail(new UploadError('Upload request timed out.', undefined, true));
            xhr.send(body);
        } catch (reason) {
            fail(reason);
        }
    });
}

function retryable(reason: unknown): reason is UploadError {
    return (
        reason instanceof UploadError &&
        ((reason.status === undefined && reason.uncertain) ||
            (reason.status !== undefined && retryableStatus(reason.status, true)))
    );
}

async function request(
    url: string,
    body: XMLHttpRequestBodyInit | null,
    headers: Record<string, string>,
    signal: AbortSignal,
    progress: (loaded: number) => void,
    method = 'PUT',
    budget?: number,
    attempts = ATTEMPTS,
    stopOnUncertain = false,
    onRetry?: () => void,
): Promise<XhrResult> {
    for (let attempt = 0; ; attempt++) {
        await online(signal);
        signal.throwIfAborted();
        progress(0);
        try {
            const result = await xhrUpload(url, body, headers, signal, progress, method, budget);
            if (result.status >= 200 && result.status < 300) return result;
            throw new UploadError(
                `Upload request failed (${result.status}).`,
                result.status,
                true,
                result.retryAfter,
            );
        } catch (reason) {
            signal.throwIfAborted();
            if (
                !retryable(reason) ||
                attempt >= attempts - 1 ||
                (stopOnUncertain && (reason.status === undefined || reason.status === 408))
            )
                throw reason;
            onRetry?.();
            await transferWait(
                reason.retryAfter ?? retryDelayMilliseconds(attempt, undefined, 250),
                signal,
            );
        }
    }
}

function abandon(url: string, headers: Record<string, string>): void {
    void xhrUpload(url, null, headers, AbortSignal.timeout(8_000), () => undefined, 'DELETE').catch(
        () => undefined,
    );
}

export async function uploadCiphertext(options: {
    chunkUrl: string;
    token: string;
    ciphertext: Uint8Array;
    transport?: UploadTransport;
    signal: AbortSignal;
    onProgress: (loaded: number) => void;
    controller: AdaptiveConcurrency;
    onPhase?: (phase: UploadPhase) => void;
}): Promise<void> {
    const { chunkUrl, token, ciphertext, transport, signal, onProgress, controller } = options;
    signal.throwIfAborted();
    options.onPhase?.(
        typeof navigator !== 'undefined' && navigator.onLine === false ? 'offline' : 'uploading',
    );
    try {
        if (transport) transportPolicy.validateUploadTransport(transport);
    } catch {
        throw new UploadError('The server supplied an invalid upload transport policy.', 422);
    }
    const headers = {
        'Content-Type': 'application/octet-stream',
        'X-Filebeam-Upload-Token': token,
    };
    const key = `${chunkUrl}:${crypto.randomUUID()}`;
    const partSize = (rate: number) => transportPolicy.partBytes(transport!, rate);
    const predictedSlow =
        transport &&
        controller.rate > 0 &&
        transportPolicy.shouldStage(
            transport,
            ciphertext.byteLength,
            controller.rate / controller.limit,
        );
    if (!transport || (!predictedSlow && controller.partBytes === undefined)) {
        let started = performance.now();
        try {
            const direct = await request(
                chunkUrl,
                ciphertext as Uint8Array<ArrayBuffer>,
                headers,
                signal,
                (loaded) => {
                    if (loaded === 0) {
                        started = performance.now();
                        controller.forget(key);
                    }
                    controller.sample(key, loaded);
                    onProgress(loaded);
                    if (loaded > 0)
                        options.onPhase?.(
                            loaded >= ciphertext.byteLength ? 'storing' : 'uploading',
                        );
                    const elapsed = performance.now() - started;
                    // Only abandon a body still being sent; a completed body may be waiting for storage.
                    if (
                        transport &&
                        elapsed >= 3_000 &&
                        loaded > 0 &&
                        loaded < ciphertext.byteLength &&
                        transportPolicy.shouldAbandonDirect(
                            transport,
                            ciphertext.byteLength,
                            loaded,
                            elapsed,
                        )
                    ) {
                        controller.partBytes = partSize(loaded / elapsed);
                        throw new UploadError(
                            'Using smaller requests for this connection.',
                            undefined,
                            true,
                        );
                    }
                },
                'PUT',
                transport?.request_budget_ms,
                ATTEMPTS,
                Boolean(transport),
                () => options.onPhase?.('retrying'),
            );
            controller.observe(ciphertext.byteLength, direct.elapsedMs);
            return;
        } catch (reason) {
            signal.throwIfAborted();
            if (
                !transport ||
                (!retryable(reason) && !(reason instanceof UploadError && reason.status === 413))
            )
                throw reason;
            controller.congested();
            options.onPhase?.('retrying');
        } finally {
            controller.forget(key);
        }
    }
    if (!transport) throw new UploadError('Upload failed.');
    let size = Math.max(
        transport.part_min_bytes,
        Math.min(
            transport.part_max_bytes,
            controller.partBytes ??
                (controller.rate > 0
                    ? partSize(controller.rate / controller.limit)
                    : transportPolicy.initialPartBytes(transport, 0)),
        ),
    );
    controller.partBytes = size;
    transportPolicy.validatePartCount(transport, ciphertext.byteLength, size);
    const checksum = await hash(ciphertext);
    signal.throwIfAborted();
    let session: import('./transfer-wasm-policy').StageSession | undefined;
    const reconcile = (value: unknown) => {
        try {
            return session!.reconcile(value);
        } catch {
            throw new UploadError('Upload staging returned invalid status.', 422);
        }
    };
    for (;;) {
        const id = crypto.randomUUID();
        const url = `${chunkUrl}/uploads/${id}`;
        try {
            if (session) session.reset(id);
            else session = new transportPolicy.StageSession(id, ciphertext.byteLength, checksum);
        } catch {
            throw new UploadError('Upload staging exceeded its recovery limit.', 422);
        }
        const status = async () =>
            reconcile((await request(url, null, headers, signal, () => undefined, 'GET')).data);
        try {
            let state = reconcile(
                (
                    await request(
                        url,
                        JSON.stringify({
                            ciphertext_bytes: ciphertext.byteLength,
                            checksum,
                        }),
                        { ...headers, 'Content-Type': 'application/json' },
                        signal,
                        () => undefined,
                    )
                ).data,
            );
            while (state.state === 'receiving' && state.offset < ciphertext.byteLength) {
                const offset = state.offset;
                const part = ciphertext.subarray(
                    offset,
                    Math.min(ciphertext.byteLength, offset + size),
                );
                try {
                    controller.sample(key, 0);
                    const result = await request(
                        `${url}/parts/${offset}`,
                        part as Uint8Array<ArrayBuffer>,
                        { ...headers, 'X-Filebeam-Part-Checksum': await hash(part) },
                        signal,
                        (loaded) => {
                            controller.sample(key, loaded);
                            onProgress(offset + loaded);
                            if (loaded > 0) options.onPhase?.('uploading');
                        },
                        'PUT',
                        transport.request_budget_ms,
                        1,
                    );
                    try {
                        state = session.acknowledgePart(offset, part.byteLength, result.data);
                    } catch {
                        throw new UploadError('Upload staging returned invalid status.', 422);
                    }
                    controller.observe(part.byteLength, result.elapsedMs);
                    size = transportPolicy.growPart(
                        transport,
                        size,
                        part.byteLength,
                        result.elapsedMs,
                    );
                    controller.partBytes = size;
                    onProgress(state.offset);
                } catch (reason) {
                    signal.throwIfAborted();
                    if (
                        !retryable(reason) &&
                        !(reason instanceof UploadError && reason.status === 409)
                    )
                        throw reason;
                    controller.congested();
                    options.onPhase?.('retrying');
                    try {
                        session.recordRetry();
                    } catch {
                        throw new UploadError(
                            'Chunk upload could not make progress after repeated retries.',
                        );
                    }
                    // No new range until status confirms what the server actually accepted.
                    await transferWait(
                        reason.retryAfter ??
                            retryDelayMilliseconds(Math.min(session.retries, 4), undefined, 250),
                        signal,
                    );
                    state = await status();
                    if (state.offset > offset) {
                        session.reprobeAfterRecovery();
                        onProgress(state.offset);
                        continue;
                    }
                    if (state.offset !== offset || reason.status === 409)
                        throw new UploadError('Upload staging has conflicting ciphertext.', 409);
                    if (reason.status === undefined || reason.status === 408) {
                        size = transportPolicy.shrinkPart(transport, size);
                        controller.partBytes = size;
                    }
                    onProgress(offset);
                } finally {
                    controller.forget(key);
                }
            }
            if (state.state === 'complete') return;
            options.onPhase?.('storing');
            const deadline = Date.now() + 15 * 60_000;
            for (let poll = 0; poll < 300 && Date.now() < deadline; poll++) {
                try {
                    const result = await request(
                        `${url}/complete`,
                        null,
                        headers,
                        signal,
                        () => undefined,
                        'POST',
                        undefined,
                        1,
                    );
                    state = reconcile(result.data);
                    if (state.state === 'complete') return;
                    await transferWait(result.retryAfter ?? 2_000, signal);
                } catch (reason) {
                    signal.throwIfAborted();
                    if (!retryable(reason)) throw reason;
                    await transferWait(
                        reason.retryAfter ??
                            retryDelayMilliseconds(Math.min(poll, 4), undefined, 500),
                        signal,
                    );
                }
                try {
                    state = await status();
                    if (state.state === 'complete') return;
                } catch (reason) {
                    signal.throwIfAborted();
                    if (!retryable(reason)) throw reason;
                    await transferWait(reason.retryAfter ?? 2_000, signal);
                }
            }
            throw new UploadError('The server could not finish storing the chunk.');
        } catch (reason) {
            abandon(url, headers);
            signal.throwIfAborted();
            if (reason instanceof UploadError && reason.status === 410) {
                onProgress(0);
                continue;
            }
            throw reason;
        }
    }
}
