const ATTEMPTS = 4;
const ATTEMPT_TIMEOUT_MS = 120_000;
const MAX_RETRY_AFTER_MS = 60_000;

class NonRetryableChunkError extends Error {}

export function transferConcurrency(configured: number | undefined, chunkBytes: number): number {
    const memory = (navigator as Navigator & { deviceMemory?: number }).deviceMemory;
    const constrained = memory !== undefined && memory <= 4;
    // Bound buffered payloads separately from WASM and browser networking copies.
    const byteBudget = (constrained ? 64 : 128) * 1024 * 1024;
    return Math.max(
        1,
        Math.min(constrained ? 2 : 8, configured ?? 4, Math.floor(byteBudget / (chunkBytes + 16))),
    );
}

function retryAfterMilliseconds(value: string | null): number | undefined {
    if (!value) return undefined;
    if (/^\d+$/.test(value)) return Math.min(Number(value) * 1_000, MAX_RETRY_AFTER_MS);
    const date = Date.parse(value);
    return Number.isNaN(date)
        ? undefined
        : Math.min(Math.max(0, date - Date.now()), MAX_RETRY_AFTER_MS);
}

function wait(milliseconds: number, signal: AbortSignal): Promise<void> {
    signal.throwIfAborted();
    return new Promise((resolve, reject) => {
        const timer = window.setTimeout(() => {
            cleanup();
            resolve();
        }, milliseconds);
        const abort = () => {
            window.clearTimeout(timer);
            cleanup();
            reject(new DOMException('Aborted', 'AbortError'));
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
        const attemptController = new AbortController();
        const timeout = window.setTimeout(() => attemptController.abort(), ATTEMPT_TIMEOUT_MS);
        const abort = () => attemptController.abort();
        signal.addEventListener('abort', abort, { once: true });
        try {
            const response = await fetch(url, { ...init, signal: attemptController.signal });
            if (response.ok) return await consume(response);
            if (response.status < 500 && response.status !== 408 && response.status !== 429)
                throw new NonRetryableChunkError(`Chunk transfer failed (${response.status}).`);
            failure = new Error(`Chunk transfer failed (${response.status}).`);
            if (attempt === ATTEMPTS - 1) break;
            await wait(
                retryAfterMilliseconds(response.headers.get('Retry-After')) ??
                    200 * 2 ** attempt + Math.floor(Math.random() * 200),
                signal,
            );
        } catch (reason) {
            if (signal.aborted) throw reason;
            // A response-status error is permanent; network and timed-out attempts retry.
            if (reason instanceof NonRetryableChunkError) throw reason;
            failure = reason;
            if (attempt === ATTEMPTS - 1) break;
            await wait(200 * 2 ** attempt + Math.floor(Math.random() * 200), signal);
        } finally {
            window.clearTimeout(timeout);
            signal.removeEventListener('abort', abort);
        }
    }
    throw failure instanceof Error ? failure : new Error('Chunk transfer failed.');
}
