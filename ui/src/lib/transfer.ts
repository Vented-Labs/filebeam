const ATTEMPTS = 4;
const IDLE_TIMEOUT_MS = 120_000;
const MAX_RETRY_AFTER_MS = 60_000;

class NonRetryableChunkError extends Error {}

type TransferWasm = typeof import('@filebeam/transfer');

let wasm: TransferWasm | undefined;
let wasmInitialising: Promise<void> | undefined;

type NodeProcess = { versions?: { node?: string }; cwd?: () => string };

function nodeProcess(): NodeProcess | undefined {
    return (globalThis as { process?: NodeProcess }).process;
}

function isNodeTestRuntime(): boolean {
    return Boolean(nodeProcess()?.versions?.node);
}

async function loadTransferWasm(): Promise<TransferWasm> {
    if (!isNodeTestRuntime()) return import('@filebeam/transfer');

    // Playwright's test runner does not apply Vite aliases. Load the same generated web module
    // directly and provide bytes because Node's fetch does not support file: URLs.
    const root = nodeProcess()?.cwd?.();
    if (!root) throw new Error('Node transfer policy loader requires a working directory.');
    const urlModule = 'node:url';
    const { pathToFileURL } = (await import(/* @vite-ignore */ urlModule)) as {
        pathToFileURL: (path: string) => URL;
    };
    const packagePath = `${root}/transfer-wasm/pkg`;
    const moduleUrl = pathToFileURL(`${packagePath}/filebeam_transfer_wasm.js`).href;
    const module = (await import(/* @vite-ignore */ moduleUrl)) as TransferWasm;
    const fsModule = 'node:fs/promises';
    const { readFile } = (await import(/* @vite-ignore */ fsModule)) as {
        readFile: (path: URL) => Promise<Uint8Array>;
    };
    // Node's Buffer can be backed by SharedArrayBuffer; wasm-bindgen accepts an ArrayBuffer view.
    const wasmBytes = Uint8Array.from(
        await readFile(pathToFileURL(`${packagePath}/filebeam_transfer_wasm_bg.wasm`)),
    );
    await module.default(wasmBytes);
    return module;
}

/** Initialise scheduling policy before a transfer begins. Network and payload I/O stay in JS. */
export function initialiseTransferPolicy(): Promise<void> {
    wasmInitialising ??= loadTransferWasm().then(async (module) => {
        if (!isNodeTestRuntime()) await module.default();
        wasm = module;
    });
    return wasmInitialising;
}

export function transferPolicy(): TransferWasm {
    if (!wasm)
        throw new Error('Transfer policy is not initialized. Call initialiseTransferPolicy first.');
    return wasm;
}

export function transferConcurrency(configured: number | undefined, chunkBytes: number): number {
    const memory = (navigator as Navigator & { deviceMemory?: number }).deviceMemory;
    const constrained = memory !== undefined && memory <= 4;
    const byteBudget = (constrained ? 64 : 128) * 1024 * 1024;
    return transferPolicy().concurrencyLimit(
        Number.isFinite(configured) ? Math.floor(configured!) : 4,
        chunkBytes,
        byteBudget,
        constrained ? 2 : 8,
    );
}

export class AdaptiveConcurrency {
    partBytes: number | undefined;
    private readonly controller: InstanceType<TransferWasm['AdaptiveConcurrencyController']>;

    constructor(
        maximum: number,
        private readonly onChange?: (limit: number) => void,
    ) {
        this.controller = new (transferPolicy().AdaptiveConcurrencyController)(maximum);
    }

    get limit(): number {
        return this.controller.limit;
    }

    get rate(): number {
        return this.controller.rate;
    }

    sample(key: string, loaded: number): void {
        const before = this.limit;
        this.controller.sample(key, loaded, Date.now());
        this.changed(before);
    }

    forget(key: string): void {
        this.controller.forget(key);
    }

    observe(bytes: number, elapsedMs: number): void {
        const before = this.limit;
        // Rust policy uses integer monotonic milliseconds. Preserve a nonzero sub-ms sample so
        // synthetic fast links and genuinely quick small requests can still establish a baseline.
        this.controller.observe(bytes, Math.max(1, Math.ceil(elapsedMs)), Date.now());
        this.changed(before);
    }

    congested(): void {
        const before = this.limit;
        this.controller.congested(Date.now());
        this.changed(before);
    }

    private changed(before: number): void {
        if (before !== this.limit) this.onChange?.(this.limit);
    }
}

export function retryableStatus(status: number, staging: boolean): boolean {
    return transferPolicy().retryableStatus(status, staging);
}

export function retryDelayMilliseconds(
    attempt: number,
    retryAfter: number | undefined,
    jitterMaximum: number,
): number {
    return transferPolicy().retryDelayMs(
        attempt,
        retryAfter,
        Math.floor(Math.random() * jitterMaximum),
    );
}

export function retryAfterMilliseconds(value: string | null): number | undefined {
    if (!value) return undefined;
    if (/^\d+$/.test(value)) return Math.min(Number(value) * 1_000, MAX_RETRY_AFTER_MS);
    const date = Date.parse(value);
    return Number.isNaN(date)
        ? undefined
        : Math.min(Math.max(0, date - Date.now()), MAX_RETRY_AFTER_MS);
}

export function transferWait(milliseconds: number, signal: AbortSignal): Promise<void> {
    signal.throwIfAborted();
    return new Promise((resolve, reject) => {
        const timer = window.setTimeout(() => {
            cleanup();
            resolve();
        }, milliseconds);
        const abort = () => {
            window.clearTimeout(timer);
            cleanup();
            reject(signal.reason ?? new DOMException('Transfer cancelled.', 'AbortError'));
        };
        function cleanup(): void {
            signal.removeEventListener('abort', abort);
        }
        signal.addEventListener('abort', abort, { once: true });
    });
}

export async function fetchChunkWithRetry<T>(
    url: string,
    init: RequestInit,
    signal: AbortSignal,
    consume: (response: Response) => Promise<T>,
): Promise<T> {
    let failure: unknown;
    for (let attempt = 0; attempt < ATTEMPTS; attempt++) {
        signal.throwIfAborted();
        const request = new AbortController();
        const timeout = window.setTimeout(
            () => request.abort(new DOMException('Transfer request timed out.', 'TimeoutError')),
            IDLE_TIMEOUT_MS,
        );
        const abort = () => request.abort(signal.reason);
        signal.addEventListener('abort', abort, { once: true });
        let backoff: number | undefined;
        try {
            const response = await fetch(url, { ...init, signal: request.signal });
            if (response.ok) return await consume(response);
            backoff = retryAfterMilliseconds(response.headers.get('Retry-After'));
            await response.body?.cancel();
            if (!retryableStatus(response.status, false))
                throw new NonRetryableChunkError(`Chunk transfer failed (${response.status}).`);
            throw new Error(`Chunk transfer failed (${response.status}).`);
        } catch (reason) {
            if (signal.aborted) throw signal.reason ?? reason;
            if (reason instanceof NonRetryableChunkError || reason instanceof SyntaxError)
                throw reason;
            failure = request.signal.aborted ? request.signal.reason : reason;
        } finally {
            window.clearTimeout(timeout);
            signal.removeEventListener('abort', abort);
        }
        if (attempt < ATTEMPTS - 1)
            await transferWait(backoff ?? retryDelayMilliseconds(attempt, undefined, 200), signal);
    }
    throw failure instanceof Error ? failure : new Error('Chunk transfer failed.');
}

export async function readChunkWithRetry(
    url: string,
    init: RequestInit,
    signal: AbortSignal,
    expectedBytes: number,
    onProgress: (loaded: number) => unknown,
    onRetry?: () => void,
    allowRanges = false,
): Promise<ArrayBuffer | null> {
    if (!Number.isSafeInteger(expectedBytes) || expectedBytes < 16 || expectedBytes > 25_000_000)
        throw new NonRetryableChunkError('Invalid ciphertext chunk size.');
    let failure: unknown;
    let output = new Uint8Array(expectedBytes);
    let loaded = 0;
    let etag: string | undefined;
    for (let attempt = 0; attempt < ATTEMPTS; attempt++) {
        signal.throwIfAborted();
        const resuming = allowRanges && loaded > 0 && etag !== undefined;
        if (!resuming) {
            output = new Uint8Array(expectedBytes);
            loaded = 0;
            onProgress(0);
        }
        const request = new AbortController();
        let idle: number | undefined;
        const touch = () => {
            window.clearTimeout(idle);
            idle = window.setTimeout(
                () =>
                    request.abort(
                        new DOMException('Download stopped making progress.', 'TimeoutError'),
                    ),
                IDLE_TIMEOUT_MS,
            );
        };
        touch();
        const abort = () => request.abort(signal.reason);
        signal.addEventListener('abort', abort, { once: true });
        let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
        let backoff: number | undefined;
        try {
            const headers = new Headers(init.headers);
            if (resuming) {
                headers.set('Range', `bytes=${loaded}-`);
                headers.set('If-Range', etag!);
            }
            const response = await fetch(url, { ...init, headers, signal: request.signal });
            if (response.status === 202) {
                const pendingDelay = retryAfterMilliseconds(response.headers.get('Retry-After'));
                await response.body?.cancel();
                if (pendingDelay) await transferWait(pendingDelay, signal);
                return null;
            }
            if (!response.ok) {
                backoff = retryAfterMilliseconds(response.headers.get('Retry-After'));
                await response.body?.cancel();
                if (!retryableStatus(response.status, false))
                    throw new NonRetryableChunkError(`Chunk transfer failed (${response.status}).`);
                throw new Error(`Chunk transfer failed (${response.status}).`);
            }
            if (resuming && response.status !== 206 && response.status !== 200)
                throw new NonRetryableChunkError('Chunk range response has an invalid status.');
            if (response.status === 206) {
                if (!resuming)
                    throw new NonRetryableChunkError('Chunk range response was not requested.');
                const contentRange = response.headers.get('Content-Range');
                const match = contentRange?.match(/^bytes (\d+)-(\d+)\/(\d+)$/);
                if (
                    !match ||
                    Number(match[1]) !== loaded ||
                    Number(match[2]) !== expectedBytes - 1 ||
                    Number(match[3]) !== expectedBytes ||
                    response.headers.get('ETag') !== etag
                )
                    throw new NonRetryableChunkError(
                        'Chunk range response does not match its prefix.',
                    );
            } else if (resuming) {
                // Servers without range support may ignore Range. Re-download, never append 200 bytes.
                output = new Uint8Array(expectedBytes);
                loaded = 0;
                onProgress(0);
            }
            reader = response.body?.getReader();
            if (!reader) throw new NonRetryableChunkError('Chunk response has no body.');
            const length = response.headers.get('Content-Length');
            const remaining = expectedBytes - loaded;
            if (length !== null && (!/^\d+$/.test(length) || Number(length) !== remaining))
                throw new NonRetryableChunkError('Chunk response has an invalid ciphertext size.');
            if (!resuming && response.status === 200) {
                const candidate = response.headers.get('ETag');
                // If no strong ETag is advertised, retain no prefix across a later interruption.
                etag = candidate && !candidate.startsWith('W/') ? candidate : undefined;
            }
            touch();
            for (;;) {
                const next = await reader.read();
                signal.throwIfAborted();
                if (next.done) break;
                if (!next.value.byteLength) continue;
                if (loaded + next.value.byteLength > expectedBytes)
                    throw new NonRetryableChunkError(
                        'Chunk response exceeds its declared ciphertext size.',
                    );
                output.set(next.value, loaded);
                loaded += next.value.byteLength;
                onProgress(loaded);
                touch();
            }
            if (loaded !== expectedBytes)
                throw new NonRetryableChunkError('Chunk response is truncated.');
            return output.buffer;
        } catch (reason) {
            if (signal.aborted) throw signal.reason ?? reason;
            if (reason instanceof NonRetryableChunkError) throw reason;
            failure = request.signal.aborted ? request.signal.reason : reason;
        } finally {
            window.clearTimeout(idle);
            signal.removeEventListener('abort', abort);
            void reader?.cancel().catch(() => undefined);
            request.abort();
        }
        if (attempt < ATTEMPTS - 1) {
            onRetry?.();
            await transferWait(backoff ?? retryDelayMilliseconds(attempt, undefined, 200), signal);
        }
    }
    throw failure instanceof Error ? failure : new Error('Chunk transfer failed.');
}
