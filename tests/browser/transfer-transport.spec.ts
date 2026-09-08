import { expect, test } from '@playwright/test';
import {
    fetchChunkWithRetry,
    readChunkWithRetry,
    retryAfterMilliseconds,
} from '../../ui/src/lib/transfer';

function response(
    status: number,
    body: ReadableStream<Uint8Array> | null = null,
    headers: Record<string, string> = {},
): Response {
    return new Response(body, { status, headers });
}

async function withFetch(queue: Array<Response | Error>, run: () => Promise<void>): Promise<void> {
    const originalFetch = globalThis.fetch;
    const originalWindow = globalThis.window;
    const originalTimeout = globalThis.setTimeout;
    let id = 0;
    Object.assign(globalThis, {
        window: globalThis,
        fetch: async () => {
            const next = queue.shift();
            if (!next) throw new Error('missing fetch');
            if (next instanceof Error) throw next;
            return next;
        },
        setTimeout: ((callback: () => void, ms?: number) => {
            if ((ms ?? 0) < 10_000) queueMicrotask(callback);
            return ++id;
        }) as typeof setTimeout,
    });
    try {
        await run();
    } finally {
        Object.assign(globalThis, {
            fetch: originalFetch,
            window: originalWindow,
            setTimeout: originalTimeout,
        });
    }
}

const stream = (...chunks: number[][]) =>
    new ReadableStream<Uint8Array>({
        start(controller) {
            for (const chunk of chunks) controller.enqueue(new Uint8Array(chunk));
            controller.close();
        },
    });

test('Retry-After accepts seconds and HTTP dates', () => {
    const now = Date.now;
    Date.now = () => 1_000;
    try {
        expect(retryAfterMilliseconds('2')).toBe(2_000);
        expect(retryAfterMilliseconds(new Date(4_000).toUTCString())).toBe(3_000);
    } finally {
        Date.now = now;
    }
});
test('fetch retry cancels an error response body', async () =>
    withFetch([response(500), response(200)], async () => {
        expect(
            await fetchChunkWithRetry('/x', {}, new AbortController().signal, async () => 'ok'),
        ).toBe('ok');
    }));
test('202 download returns null', async () =>
    withFetch([response(202)], async () => {
        expect(
            await readChunkWithRetry('/x', {}, new AbortController().signal, 16, () => undefined),
        ).toBeNull();
    }));
test('download reports chunks before EOF and returns exact bytes', async () =>
    withFetch(
        [response(200, stream(Array(8).fill(1), Array(8).fill(2)), { 'Content-Length': '16' })],
        async () => {
            const progress: number[] = [];
            expect([
                ...new Uint8Array(
                    await readChunkWithRetry('/x', {}, new AbortController().signal, 16, (n) =>
                        progress.push(n),
                    )!,
                ),
            ]).toEqual([...Array(8).fill(1), ...Array(8).fill(2)]);
            expect(progress).toEqual([0, 8, 16]);
        },
    ));
test('oversized download rejects before accepting excess bytes', async () =>
    withFetch([response(200, stream(Array(17).fill(1)))], async () => {
        await expect(
            readChunkWithRetry('/x', {}, new AbortController().signal, 16, () => undefined),
        ).rejects.toThrow('exceeds');
    }));
test('malformed content length rejects without progress writes', async () =>
    withFetch(
        [response(200, stream(Array(16).fill(1)), { 'Content-Length': 'bogus' })],
        async () => {
            const progress: number[] = [];
            await expect(
                readChunkWithRetry('/x', {}, new AbortController().signal, 16, (n) =>
                    progress.push(n),
                ),
            ).rejects.toThrow('invalid');
            expect(progress).toEqual([0]);
        },
    ));
test('download retry resets progress bytes', async () =>
    withFetch([new Error('lost'), response(200, stream(Array(16).fill(3)))], async () => {
        const progress: number[] = [];
        await readChunkWithRetry('/x', {}, new AbortController().signal, 16, (n) =>
            progress.push(n),
        );
        expect(progress.filter((n) => n === 0)).toHaveLength(2);
    }));
test('external cancellation remains cancellation rather than timeout', async () =>
    withFetch([new Error('lost')], async () => {
        const controller = new AbortController();
        controller.abort(new DOMException('stop', 'AbortError'));
        await expect(
            fetchChunkWithRetry('/x', {}, controller.signal, async () => undefined),
        ).rejects.toThrow('stop');
    }));
