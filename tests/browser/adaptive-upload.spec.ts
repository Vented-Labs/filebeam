import { expect, test } from '@playwright/test';
import { uploadCiphertext, xhrUpload } from '../../ui/src/lib/adaptive-upload';
import { AdaptiveConcurrency } from '../../ui/src/lib/transfer';

type Plan = {
    status?: number;
    data?: unknown;
    error?: boolean;
    progress?: number[];
    responseProgress?: boolean;
};

class FakeXhr {
    static instances: FakeXhr[] = [];
    static plan: Plan[] = [];
    status = 200;
    responseText = '';
    upload: { onprogress?: (event: ProgressEvent) => void; onload?: () => void } = {};
    onprogress?: () => void;
    onload?: () => void;
    onerror?: () => void;
    onabort?: () => void;
    ontimeout?: () => void;
    method = '';
    url = '';
    body: unknown;
    headers = new Map<string, string>();
    constructor() {
        FakeXhr.instances.push(this);
    }
    open(method: string, url: string): void {
        this.method = method;
        this.url = url;
    }
    setRequestHeader(name: string, value: string): void {
        this.headers.set(name, value);
    }
    getResponseHeader(): string | null {
        return null;
    }
    abort(): void {
        this.onabort?.();
    }
    send(body: unknown): void {
        this.body = body;
        const plan = FakeXhr.plan.shift() ?? {};
        this.status = plan.status ?? 200;
        this.responseText = plan.data === undefined ? '' : JSON.stringify(plan.data);
        for (const loaded of plan.progress ?? [])
            this.upload.onprogress?.({ loaded } as ProgressEvent);
        this.upload.onload?.();
        if (plan.responseProgress) this.onprogress?.();
        if (plan.error) this.onerror?.();
        else this.onload?.();
    }
}

const transport = {
    version: 1 as const,
    part_min_bytes: 1,
    part_max_bytes: 2,
    request_target_ms: 10,
    request_budget_ms: 100,
};
const stage = (
    id: string,
    offset: number,
    bytes: number,
    checksum: string,
    state: 'receiving' | 'finalizing' | 'complete' = 'receiving',
) => ({ data: { id, state, offset, ciphertext_bytes: bytes, checksum } });

async function mocked(run: () => Promise<void>): Promise<void> {
    const xhr = globalThis.XMLHttpRequest;
    const window = globalThis.window;
    const timeout = globalThis.setTimeout;
    const clear = globalThis.clearTimeout;
    let id = 0;
    Object.assign(globalThis, {
        window: globalThis,
        XMLHttpRequest: FakeXhr,
        setTimeout: ((callback: () => void, milliseconds?: number) => {
            const timer = ++id;
            if ((milliseconds ?? 0) < 10_000) queueMicrotask(callback);
            return timer;
        }) as typeof setTimeout,
        clearTimeout: (() => undefined) as typeof clearTimeout,
    });
    FakeXhr.instances = [];
    FakeXhr.plan = [];
    try {
        await run();
    } finally {
        Object.assign(globalThis, {
            window,
            XMLHttpRequest: xhr,
            setTimeout: timeout,
            clearTimeout: clear,
        });
    }
}

test('XHR reports upload progress before its acknowledgement', async () =>
    mocked(async () => {
        FakeXhr.plan = [{ status: 201, progress: [1, 3], responseProgress: true }];
        const progress: number[] = [];
        const result = await xhrUpload(
            '/chunk',
            new Uint8Array(3),
            {},
            new AbortController().signal,
            (n) => progress.push(n),
        );
        expect(result.status).toBe(201);
        expect(progress).toEqual([1, 3]);
    }));

test('an already aborted upload makes no request', async () =>
    mocked(async () => {
        const abort = new AbortController();
        abort.abort(new DOMException('cancelled', 'AbortError'));
        await expect(
            uploadCiphertext({
                chunkUrl: '/chunk',
                token: 't',
                ciphertext: new Uint8Array(1),
                transport,
                signal: abort.signal,
                controller: new AdaptiveConcurrency(1),
                onProgress: () => undefined,
            }),
        ).rejects.toThrow('cancelled');
        expect(FakeXhr.instances).toHaveLength(0);
    }));

test('a direct uncertain failure immediately falls back instead of four direct attempts', async () =>
    mocked(async () => {
        FakeXhr.plan = [{ error: true }, { data: stage('id', 0, 2, 'bad') }];
        await expect(
            uploadCiphertext({
                chunkUrl: '/chunk',
                token: 't',
                ciphertext: new Uint8Array([1, 2]),
                transport,
                signal: new AbortController().signal,
                controller: new AdaptiveConcurrency(1),
                onProgress: () => undefined,
            }),
        ).rejects.toThrow('invalid status');
        expect(FakeXhr.instances.filter((xhr) => xhr.url === '/chunk')).toHaveLength(1);
    }));

test('staged upload derives its URL from the returned generated id and sends incremental offsets', async () =>
    mocked(async () => {
        // The checksum is learned from the create body, independent of crypto implementation.
        const originalSend = FakeXhr.prototype.send;
        let checksum = '';
        FakeXhr.prototype.send = function (body: unknown): void {
            const id = this.url.split('/uploads/')[1]?.split('/')[0] ?? '';
            let plan: Plan;
            if (this.url === '/chunk') plan = { error: true };
            else if (this.method === 'PUT' && !this.url.includes('/parts/')) {
                const parsed = JSON.parse(String(body)) as {
                    checksum: string;
                    ciphertext_bytes: number;
                };
                checksum = parsed.checksum;
                plan = { data: stage(id, 0, parsed.ciphertext_bytes, checksum) };
            } else if (this.url.endsWith('/parts/0')) plan = { data: stage(id, 2, 3, checksum) };
            else if (this.url.endsWith('/parts/2')) plan = { data: stage(id, 3, 3, checksum) };
            else plan = { data: stage(id, 3, 3, checksum, 'complete') };
            FakeXhr.plan.unshift(plan);
            originalSend.call(this, body);
        };
        try {
            await uploadCiphertext({
                chunkUrl: '/chunk',
                token: 't',
                ciphertext: new Uint8Array([1, 2, 3]),
                transport,
                signal: new AbortController().signal,
                controller: new AdaptiveConcurrency(1),
                onProgress: () => undefined,
            });
            expect(FakeXhr.instances.map((xhr) => xhr.url)).toContainEqual(
                expect.stringMatching(/\/chunk\/uploads\/[^/]+\/parts\/0/),
            );
            expect(FakeXhr.instances.map((xhr) => xhr.url)).toContainEqual(
                expect.stringMatching(/\/chunk\/uploads\/[^/]+\/parts\/2/),
            );
        } finally {
            FakeXhr.prototype.send = originalSend;
        }
    }));

test('a saved part after response loss is discovered by status and is not resent', async () =>
    mocked(async () => {
        const originalSend = FakeXhr.prototype.send;
        let checksum = '';
        FakeXhr.prototype.send = function (body: unknown): void {
            const id = this.url.split('/uploads/')[1]?.split('/')[0] ?? '';
            if (this.url === '/chunk') FakeXhr.plan.unshift({ error: true });
            else if (this.method === 'PUT' && !this.url.includes('/parts/')) {
                checksum = (JSON.parse(String(body)) as { checksum: string }).checksum;
                FakeXhr.plan.unshift({ data: stage(id, 0, 2, checksum) });
            } else if (this.url.endsWith('/parts/0')) FakeXhr.plan.unshift({ error: true });
            else if (this.method === 'GET')
                FakeXhr.plan.unshift({ data: stage(id, 2, 2, checksum) });
            else FakeXhr.plan.unshift({ data: stage(id, 2, 2, checksum, 'complete') });
            originalSend.call(this, body);
        };
        try {
            await uploadCiphertext({
                chunkUrl: '/chunk',
                token: 't',
                ciphertext: new Uint8Array([1, 2]),
                transport,
                signal: new AbortController().signal,
                controller: new AdaptiveConcurrency(1),
                onProgress: () => undefined,
            });
            expect(FakeXhr.instances.filter((xhr) => xhr.url.endsWith('/parts/0'))).toHaveLength(1);
        } finally {
            FakeXhr.prototype.send = originalSend;
        }
    }));

test('an unsaved response loss retries a smaller part rather than throwing', async () =>
    mocked(async () => {
        const originalSend = FakeXhr.prototype.send;
        let checksum = '';
        let partAttempts = 0;
        FakeXhr.prototype.send = function (body: unknown): void {
            const id = this.url.split('/uploads/')[1]?.split('/')[0] ?? '';
            if (this.url === '/chunk') FakeXhr.plan.unshift({ error: true });
            else if (this.method === 'PUT' && !this.url.includes('/parts/')) {
                checksum = (JSON.parse(String(body)) as { checksum: string }).checksum;
                FakeXhr.plan.unshift({ data: stage(id, 0, 3, checksum) });
            } else if (this.url.endsWith('/parts/0') && partAttempts++ === 0)
                FakeXhr.plan.unshift({ error: true });
            else if (this.method === 'GET')
                FakeXhr.plan.unshift({ data: stage(id, 0, 3, checksum) });
            else if (this.url.endsWith('/parts/0'))
                FakeXhr.plan.unshift({ data: stage(id, 1, 3, checksum) });
            else if (this.url.endsWith('/parts/1'))
                FakeXhr.plan.unshift({ data: stage(id, 2, 3, checksum) });
            else if (this.url.endsWith('/parts/2'))
                FakeXhr.plan.unshift({ data: stage(id, 3, 3, checksum) });
            else FakeXhr.plan.unshift({ data: stage(id, 3, 3, checksum, 'complete') });
            originalSend.call(this, body);
        };
        try {
            await uploadCiphertext({
                chunkUrl: '/chunk',
                token: 't',
                ciphertext: new Uint8Array([1, 2, 3]),
                transport,
                signal: new AbortController().signal,
                controller: new AdaptiveConcurrency(1),
                onProgress: () => undefined,
            });
            expect(FakeXhr.instances.filter((xhr) => xhr.url.endsWith('/parts/0'))).toHaveLength(2);
            expect(FakeXhr.instances.some((xhr) => xhr.url.endsWith('/parts/1'))).toBe(true);
        } finally {
            FakeXhr.prototype.send = originalSend;
        }
    }));

test('a 410 reset is bounded and starts a fresh staging id', async () =>
    mocked(async () => {
        const originalSend = FakeXhr.prototype.send;
        let creates = 0;
        FakeXhr.prototype.send = function (body: unknown): void {
            const id = this.url.split('/uploads/')[1]?.split('/')[0] ?? '';
            if (this.url === '/chunk') FakeXhr.plan.unshift({ error: true });
            else if (this.method === 'PUT' && !this.url.includes('/parts/')) {
                if (creates++ === 0) FakeXhr.plan.unshift({ status: 410 });
                else {
                    const parsed = JSON.parse(String(body)) as {
                        checksum: string;
                        ciphertext_bytes: number;
                    };
                    FakeXhr.plan.unshift({
                        data: stage(
                            id,
                            parsed.ciphertext_bytes,
                            parsed.ciphertext_bytes,
                            parsed.checksum,
                            'complete',
                        ),
                    });
                }
            } else FakeXhr.plan.unshift({ status: 204 });
            originalSend.call(this, body);
        };
        try {
            await uploadCiphertext({
                chunkUrl: '/chunk',
                token: 't',
                ciphertext: new Uint8Array([1, 2]),
                transport,
                signal: new AbortController().signal,
                controller: new AdaptiveConcurrency(1),
                onProgress: () => undefined,
            });
            const createsUrls = FakeXhr.instances
                .filter((xhr) => xhr.method === 'PUT' && /\/uploads\/[^/]+$/.test(xhr.url))
                .map((xhr) => xhr.url);
            expect(new Set(createsUrls).size).toBe(2);
        } finally {
            FakeXhr.prototype.send = originalSend;
        }
    }));

test('a finalizing completion retries after bounded status failures', async () =>
    mocked(async () => {
        const originalSend = FakeXhr.prototype.send;
        let id = '';
        let checksum = '';
        let completionPosts = 0;
        let statusGets = 0;
        FakeXhr.prototype.send = function (body: unknown): void {
            if (this.url === '/chunk') FakeXhr.plan.unshift({ error: true });
            else if (this.method === 'PUT' && !this.url.includes('/parts/')) {
                id = this.url.split('/uploads/')[1] ?? '';
                checksum = (JSON.parse(String(body)) as { checksum: string }).checksum;
                FakeXhr.plan.unshift({ data: stage(id, 0, 2, checksum) });
            } else if (this.url.endsWith('/parts/0')) {
                FakeXhr.plan.unshift({ data: stage(id, 2, 2, checksum) });
            } else if (this.method === 'GET') {
                statusGets++;
                FakeXhr.plan.unshift({ status: 500 });
            } else {
                completionPosts++;
                FakeXhr.plan.unshift({
                    data: stage(
                        id,
                        2,
                        2,
                        checksum,
                        completionPosts === 1 ? 'finalizing' : 'complete',
                    ),
                });
            }
            originalSend.call(this, body);
        };
        try {
            await uploadCiphertext({
                chunkUrl: '/chunk',
                token: 't',
                ciphertext: new Uint8Array([1, 2]),
                transport,
                signal: new AbortController().signal,
                controller: new AdaptiveConcurrency(1),
                onProgress: () => undefined,
            });
            expect(completionPosts).toBe(2);
            expect(statusGets).toBe(4);
        } finally {
            FakeXhr.prototype.send = originalSend;
        }
    }));

test('malformed staging ids, checksums, and offsets are terminal', async () =>
    mocked(async () => {
        for (const value of [
            stage('wrong', 0, 2, 'x'),
            stage('id', 3, 2, 'x'),
            stage('id', 0, 2, 'wrong'),
        ]) {
            FakeXhr.plan = [{ error: true }, { data: value }];
            await expect(
                uploadCiphertext({
                    chunkUrl: '/chunk',
                    token: 't',
                    ciphertext: new Uint8Array([1, 2]),
                    transport,
                    signal: new AbortController().signal,
                    controller: new AdaptiveConcurrency(1),
                    onProgress: () => undefined,
                }),
            ).rejects.toThrow('invalid status');
        }
    }));
