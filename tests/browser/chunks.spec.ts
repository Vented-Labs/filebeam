import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';

type Config = {
    chunk_bytes: number;
    upload_concurrency?: number;
    download_concurrency?: number;
};
type CreatedTransfer = {
    id: string;
    deleteToken: string;
    link: string;
    content: Buffer;
    digest: string;
};

const chunkPath = /\/api\/v1\/transfers\/[^/]+\/items\/[^/]+\/chunks\/(\d+)$/;

let transfers: Array<{ id: string; deleteToken: string }>;
let contexts: BrowserContext[];

test.beforeEach(() => {
    transfers = [];
    contexts = [];
});

test.afterEach(async ({ request }) => {
    await Promise.all(
        transfers.map(({ id, deleteToken }) =>
            request.delete(`/api/v1/transfers/${id}`, {
                headers: { 'X-Filebeam-Delete-Token': deleteToken },
            }),
        ),
    );
    await Promise.all(contexts.map((context) => context.close()));
});

async function transferConfig(page: Page, smallChunks = true): Promise<Config> {
    await page.goto('/');
    const config = await page.evaluate(() => {
        const encoded = document.querySelector<HTMLScriptElement>('script[data-page]')?.textContent;
        return encoded
            ? (JSON.parse(encoded) as { props?: { filebeam?: Config } }).props?.filebeam
            : undefined;
    });
    if (!config?.chunk_bytes)
        throw new Error('The production page did not expose its chunk policy.');
    // Chunk pipeline coverage is intentionally run against the coordinator's 1 KiB policy.
    test.skip(
        smallChunks && config.chunk_bytes > 16_384,
        'Start the isolated server with CHUNK_MAX_SIZE=1040.',
    );
    return config;
}

async function concurrencyProbe(page: Page, method: 'PUT' | 'GET', outOfOrder = false) {
    let active = 0;
    let maximum = 0;
    let requests = 0;
    const firstAttempt = new Set<string>();
    await page.route('**/api/v1/transfers/*/items/*/chunks/*', async (route) => {
        if (route.request().method() !== method) return route.continue();
        const match = route.request().url().match(chunkPath);
        const index = Number(match?.[1]);
        requests++;
        active++;
        maximum = Math.max(maximum, active);
        await new Promise((resolve) => setTimeout(resolve, outOfOrder && index === 0 ? 200 : 50));
        const key = route.request().url();
        if (method === 'PUT' && !firstAttempt.has(key)) {
            firstAttempt.add(key);
            active--;
            return route.fulfill({ status: 503, body: 'transient test failure' });
        }
        active--;
        return route.continue();
    });
    return { maximum: () => maximum, requests: () => requests };
}

async function uploadChunked(
    page: Page,
    config: Config,
    probePrivacy = false,
): Promise<CreatedTransfer> {
    const content = Buffer.from(
        Array.from({ length: config.chunk_bytes * 4 + 137 }, (_, i) => i % 251),
    );
    const digest = createHash('sha256').update(content).digest('hex');
    const bodies: string[] = [];
    if (probePrivacy) {
        page.on('request', (request) => {
            if (request.url().includes('/api/v1/transfers')) bodies.push(request.postData() ?? '');
        });
    }
    await page.locator('#filebeam-picker').setInputFiles({
        name: 'parallel-private.bin',
        mimeType: 'application/octet-stream',
        buffer: content,
    });
    const created = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' &&
            response.url().endsWith('/api/v1/transfers') &&
            response.status() === 201,
    );
    await page.getByRole('button', { name: 'Encrypt and share' }).click();
    const data = (await (await created).json()).data as {
        id: string;
        delete_token: string;
    };
    transfers.push({ id: data.id, deleteToken: data.delete_token });
    await expect(page.getByRole('heading', { name: 'Your encrypted link is ready' })).toBeVisible();
    if (probePrivacy) expect(bodies.join('\n')).not.toContain(digest);
    return {
        id: data.id,
        deleteToken: data.delete_token,
        link: await page.locator('#share-link').inputValue(),
        content,
        digest,
    };
}

async function recipient(browser: Browser, mode?: 'bad-digest' | 'missing-digest'): Promise<Page> {
    const context = await browser.newContext();
    contexts.push(context);
    await context.addInitScript((manifestMode) => {
        const NativeWorker = window.Worker;
        class ObservedWorker {
            private readonly worker: Worker;
            private readonly listeners = new Map<
                EventListenerOrEventListenerObject,
                EventListener
            >();
            constructor(url: string | URL, options?: WorkerOptions) {
                this.worker = new NativeWorker(url, options);
            }
            postMessage(message: unknown, transfer?: Transferable[]): void {
                this.worker.postMessage(message, transfer ?? []);
            }
            terminate(): void {
                this.worker.terminate();
            }
            addEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
                if (type !== 'message') return this.worker.addEventListener(type, listener);
                const wrapped: EventListener = (event) => {
                    const data = (event as MessageEvent).data;
                    if (data?.type === 'manifest') {
                        (window as any).__chunkWorkerManifest = data.manifest;
                        if (manifestMode) {
                            const manifest = JSON.parse(data.manifest);
                            if (manifestMode === 'bad-digest')
                                manifest.items[0].digest.value = '0'.repeat(64);
                            else delete manifest.items[0].digest;
                            return typeof listener === 'function'
                                ? listener(
                                      new MessageEvent('message', {
                                          data: { ...data, manifest: JSON.stringify(manifest) },
                                      }),
                                  )
                                : listener.handleEvent(
                                      new MessageEvent('message', {
                                          data: { ...data, manifest: JSON.stringify(manifest) },
                                      }),
                                  );
                        }
                    }
                    return typeof listener === 'function'
                        ? listener(event)
                        : listener.handleEvent(event);
                };
                this.listeners.set(listener, wrapped);
                this.worker.addEventListener(type, wrapped);
            }
            removeEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
                this.worker.removeEventListener(
                    type,
                    this.listeners.get(listener) ?? (listener as EventListener),
                );
                this.listeners.delete(listener);
            }
        }
        Object.defineProperty(window, 'Worker', {
            configurable: true,
            value: ObservedWorker,
        });
        const unhandled: string[] = [];
        window.addEventListener('unhandledrejection', (event) =>
            unhandled.push(String(event.reason)),
        );
        (window as any).__chunkUnhandled = unhandled;
        const writable = {
            chunks: [] as number[][],
            closed: false,
            aborted: false,
        };
        Object.assign(window, {
            __chunkWritable: writable,
            showSaveFilePicker: async () => ({
                createWritable: async () => ({
                    write: async (data: Uint8Array) => writable.chunks.push([...data]),
                    close: async () => {
                        writable.closed = true;
                    },
                    abort: async () => {
                        writable.aborted = true;
                    },
                }),
            }),
        });
    }, mode);
    return context.newPage();
}

test('round-trips a full-size encrypted chunk and its final tail', async ({ page, browser }) => {
    const config = await transferConfig(page, false);
    expect(config.chunk_bytes + 16).toBeLessThanOrEqual(25_000_000);
    const content = Buffer.alloc(config.chunk_bytes + 137, 0x5a);
    const lengths: number[] = [];
    page.on('request', (request) => {
        if (request.method() === 'PUT' && chunkPath.test(request.url()))
            lengths.push(request.postDataBuffer()!.length);
    });
    await page.locator('#filebeam-picker').setInputFiles({
        name: 'maximum-chunk.bin',
        mimeType: 'application/octet-stream',
        buffer: content,
    });
    const creation = page
        .waitForResponse(
            (response) =>
                response.request().method() === 'POST' &&
                response.url().endsWith('/api/v1/transfers'),
        )
        .then(async (response) => {
            expect(response.status()).toBe(201);
            const { data } = await response.json();
            transfers.push({ id: data.id, deleteToken: data.delete_token });
            return data;
        });
    // Creating the transfer does not mean the 25 MB upload has completed.
    const completion = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' &&
            /^\/api\/v1\/transfers\/[^/]+\/complete$/.test(new URL(response.url()).pathname),
        { timeout: 90_000 },
    );
    const [data, completed] = await Promise.all([
        creation,
        completion,
        page.getByRole('button', { name: 'Encrypt and share' }).click(),
    ]);
    expect(completed.status()).toBe(200);
    expect(new URL(completed.url()).pathname).toBe(`/api/v1/transfers/${data.id}/complete`);
    await expect(page.getByRole('heading', { name: 'Your encrypted link is ready' })).toBeVisible();
    expect(lengths.sort((a, b) => a - b)).toEqual([153, config.chunk_bytes + 16]);
    const context = await browser.newContext();
    contexts.push(context);
    await context.addInitScript(() =>
        Object.defineProperty(window, 'showSaveFilePicker', { value: undefined }),
    );
    const downloadPage = await context.newPage();
    await downloadPage.goto(await page.locator('#share-link').inputValue());
    const downloaded = downloadPage.waitForEvent('download');
    await downloadPage.getByRole('button', { name: 'Download files' }).click();
    const path = await (await downloaded).path();
    expect(await readFile(path!)).toEqual(content);
});

test(
    'uses bounded parallel PUTs with retries and never sends the plaintext SHA-256',
    { tag: '@small-chunks' },
    async ({ page }) => {
        const config = await transferConfig(page);
        const probe = await concurrencyProbe(page, 'PUT');
        await uploadChunked(page, config, true);
        expect(probe.maximum()).toBeGreaterThanOrEqual(2);
        expect(probe.maximum()).toBeLessThanOrEqual(
            Math.max(1, Math.min(8, config.upload_concurrency ?? 4)),
        );
        // Each of five chunks is retried once after a transient 503.
        expect(probe.requests()).toBeGreaterThanOrEqual(10);
    },
);

test(
    'streams out-of-order parallel GETs into ordered plaintext and keeps the digest private',
    { tag: '@small-chunks' },
    async ({ browser, page, request }) => {
        const config = await transferConfig(page);
        const uploaded = await uploadChunked(page, config, true);
        const metadata = await (await request.get(`/api/v1/transfers/${uploaded.id}`)).text();
        expect(metadata).not.toContain(uploaded.digest);
        const download = await recipient(browser);
        const probe = await concurrencyProbe(download, 'GET', true);
        await download.goto(uploaded.link);
        await expect(download.getByText('parallel-private.bin')).toBeVisible();
        expect(await download.evaluate(() => (window as any).__chunkWorkerManifest)).toContain(
            uploaded.digest,
        );
        await download.getByRole('button', { name: 'Download files' }).click();
        await expect
            .poll(() => download.evaluate(() => (window as any).__chunkWritable.closed))
            .toBe(true);
        expect(
            Buffer.from(
                await download.evaluate(() => (window as any).__chunkWritable.chunks.flat()),
            ),
        ).toEqual(uploaded.content);
        expect(probe.maximum()).toBeGreaterThanOrEqual(2);
        expect(probe.maximum()).toBeLessThanOrEqual(
            Math.max(1, Math.min(8, config.download_concurrency ?? 4)),
        );
        expect(await download.evaluate(() => (window as any).__chunkUnhandled)).toEqual([]);
    },
);

test(
    'rejects malformed v1 manifests and aborts a mismatched digest without closing a download',
    { tag: '@small-chunks' },
    async ({ browser, page }) => {
        const config = await transferConfig(page);
        const uploaded = await uploadChunked(page, config);
        const malformed = await recipient(browser, 'missing-digest');
        await malformed.goto(uploaded.link);
        await expect(malformed.getByRole('alert')).toContainText(/invalid item metadata/i);
        const mismatched = await recipient(browser, 'bad-digest');
        await mismatched.goto(uploaded.link);
        await expect(mismatched.getByRole('button', { name: 'Download files' })).toBeVisible();
        await mismatched.getByRole('button', { name: 'Download files' }).click();
        await expect(mismatched.getByRole('alert')).toContainText(/integrity check/i);
        expect(await mismatched.evaluate(() => (window as any).__chunkWritable)).toMatchObject({
            closed: false,
            aborted: true,
        });
        expect(await mismatched.evaluate(() => (window as any).__chunkUnhandled)).toEqual([]);
    },
);

test(
    'cancels delayed work and handles a failed queued chunk without unhandled rejections',
    { tag: '@small-chunks' },
    async ({ browser, page }) => {
        const config = await transferConfig(page);
        const uploaded = await uploadChunked(page, config);
        const cancelled = await recipient(browser);
        await cancelled.route('**/api/v1/transfers/*/items/*/chunks/*', async (route) => {
            if (route.request().method() !== 'GET') return route.continue();
            await new Promise((resolve) => setTimeout(resolve, 250));
            return route.continue();
        });
        await cancelled.goto(uploaded.link);
        await cancelled.getByRole('button', { name: 'Download files' }).click();
        await cancelled.getByRole('button', { name: 'Cancel' }).click();
        await expect
            .poll(() => cancelled.evaluate(() => (window as any).__chunkWritable.aborted))
            .toBe(true);
        expect(await cancelled.evaluate(() => (window as any).__chunkUnhandled)).toEqual([]);

        const failedFuture = await recipient(browser);
        await failedFuture.route('**/api/v1/transfers/*/items/*/chunks/*', async (route) => {
            if (route.request().method() !== 'GET') return route.continue();
            const index = Number(route.request().url().match(chunkPath)?.[1]);
            await new Promise((resolve) => setTimeout(resolve, index === 0 ? 500 : 50));
            if (index === 3) return route.fulfill({ status: 404, body: 'future chunk failed' });
            return route.continue();
        });
        await failedFuture.goto(uploaded.link);
        await failedFuture.getByRole('button', { name: 'Download files' }).click();
        await expect(failedFuture.getByRole('alert')).toContainText(/chunk transfer failed/i);
        expect(await failedFuture.evaluate(() => (window as any).__chunkWritable)).toMatchObject({
            closed: false,
            aborted: true,
        });
        expect(await failedFuture.evaluate(() => (window as any).__chunkUnhandled)).toEqual([]);
    },
);
