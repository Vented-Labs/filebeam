const ATTEMPTS = 4;
const IDLE_TIMEOUT_MS = 120_000;
const MAX_RETRY_AFTER_MS = 60_000;

class NonRetryableChunkError extends Error {}

export function transferConcurrency(configured: number | undefined, chunkBytes: number): number {
    const memory = (navigator as Navigator & { deviceMemory?: number }).deviceMemory;
    const constrained = memory !== undefined && memory <= 4;
    // Leave room for plaintext, WASM, and browser networking copies as well as ciphertext.
    const byteBudget = (constrained ? 64 : 128) * 1024 * 1024;
    const ceiling = Number.isFinite(configured) ? Math.floor(configured!) : 4;
    const bytes = Number.isSafeInteger(chunkBytes) && chunkBytes > 0 ? chunkBytes + 16 : byteBudget;
    return Math.max(1, Math.min(constrained ? 2 : 8, ceiling, Math.floor(byteBudget / bytes)));
}

/** Measure aggregate body throughput; additional slots are experiments, not a speed promise. */
export class AdaptiveConcurrency {
    private current = 1;
    private readonly maximum: number;
    private requests = new Map<string, number>();
    private windowStart: number | undefined;
    private windowBytes = 0;
    private events = 0;
    private measuredRate = 0;
    private probeRate = 0;
    private cooldownUntil = 0;
    private successes = 0;
    partBytes: number | undefined;

    constructor(
        maximum: number,
        private readonly onChange?: (limit: number) => void,
    ) {
        this.maximum = Number.isFinite(maximum) ? Math.max(1, Math.min(8, Math.floor(maximum))) : 1;
    }

    get limit(): number {
        return this.current;
    }

    /** Bytes per millisecond, excluding idle time between requests. */
    get rate(): number {
        return this.measuredRate;
    }

    sample(key: string, loaded: number): void {
        if (!Number.isFinite(loaded) || loaded < 0) return;
        const now = Date.now();
        if (this.windowStart === undefined) this.windowStart = now;
        const previous = this.requests.get(key) ?? 0;
        this.requests.set(key, loaded);
        if (loaded <= previous) return;
        this.windowBytes += loaded - previous;
        this.events++;
        const elapsed = now - this.windowStart;
        if (elapsed < 2_000 || this.events < 3) return;
        const rate = this.windowBytes / elapsed;
        this.measuredRate = this.measuredRate ? this.measuredRate * 0.5 + rate * 0.5 : rate;
        this.windowStart = now;
        this.windowBytes = 0;
        this.events = 0;
        if (now < this.cooldownUntil) return;
        if (this.probeRate) {
            const improved = rate >= this.probeRate * 1.1;
            this.probeRate = 0;
            if (!improved) {
                this.change(Math.max(1, this.current - 1));
                this.cooldownUntil = now + 8_000;
                return;
            }
        }
        // Low-bandwidth links benefit from finishing the earliest chunk, not splitting its uplink.
        if (rate >= (256 * 1024) / 1_000 && this.current < this.maximum) {
            this.probeRate = rate;
            this.cooldownUntil = now + 2_000;
            this.change(this.current + 1);
        }
    }

    forget(key: string): void {
        this.requests.delete(key);
        if (!this.requests.size) {
            this.windowStart = undefined;
            this.windowBytes = 0;
            this.events = 0;
        }
    }

    observe(bytes: number, elapsedMs: number): void {
        if (!Number.isFinite(bytes) || !Number.isFinite(elapsedMs) || bytes <= 0 || elapsedMs <= 0)
            return;
        const rate = bytes / elapsedMs;
        // Completed short requests provide a baseline even when no sampling window has elapsed.
        if (!this.measuredRate) this.measuredRate = rate;
        this.successes++;
        if (
            this.current === 1 &&
            this.successes >= 2 &&
            Date.now() >= this.cooldownUntil &&
            rate >= (256 * 1024) / 1_000 &&
            this.maximum > 1
        ) {
            this.probeRate = this.measuredRate;
            this.cooldownUntil = Date.now() + 2_000;
            this.change(2);
        }
    }

    congested(): void {
        this.probeRate = 0;
        this.successes = 0;
        this.cooldownUntil = Date.now() + 8_000;
        this.change(Math.max(1, Math.floor(this.current / 2)));
    }

    private change(limit: number): void {
        if (this.current === limit) return;
        this.current = limit;
        this.onChange?.(limit);
    }
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
            if (response.status < 500 && response.status !== 408 && response.status !== 429)
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
            await transferWait(backoff ?? 200 * 2 ** attempt + Math.random() * 200, signal);
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
): Promise<ArrayBuffer | null> {
    if (!Number.isSafeInteger(expectedBytes) || expectedBytes < 16 || expectedBytes > 25_000_000)
        throw new NonRetryableChunkError('Invalid ciphertext chunk size.');
    let failure: unknown;
    for (let attempt = 0; attempt < ATTEMPTS; attempt++) {
        signal.throwIfAborted();
        onProgress(0);
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
            const response = await fetch(url, { ...init, signal: request.signal });
            if (response.status === 202) {
                const pendingDelay = retryAfterMilliseconds(response.headers.get('Retry-After'));
                await response.body?.cancel();
                if (pendingDelay) await transferWait(pendingDelay, signal);
                return null;
            }
            if (!response.ok) {
                backoff = retryAfterMilliseconds(response.headers.get('Retry-After'));
                await response.body?.cancel();
                if (response.status < 500 && response.status !== 408 && response.status !== 429)
                    throw new NonRetryableChunkError(`Chunk transfer failed (${response.status}).`);
                throw new Error(`Chunk transfer failed (${response.status}).`);
            }
            reader = response.body?.getReader();
            if (!reader) throw new NonRetryableChunkError('Chunk response has no body.');
            const length = response.headers.get('Content-Length');
            if (length !== null && (!/^\d+$/.test(length) || Number(length) !== expectedBytes))
                throw new NonRetryableChunkError('Chunk response has an invalid ciphertext size.');
            touch();
            const output = new Uint8Array(expectedBytes);
            let loaded = 0;
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
            await transferWait(backoff ?? 200 * 2 ** attempt + Math.random() * 200, signal);
        }
    }
    throw failure instanceof Error ? failure : new Error('Chunk transfer failed.');
}
