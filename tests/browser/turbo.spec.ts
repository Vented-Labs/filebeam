import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test';
import { createHash } from 'node:crypto';

type Config = { chunk_bytes: number };
type Transfer = { id: string; deleteToken: string; link: string; digest: string };
type WritableMode = 'plain' | 'bad-descriptor' | 'bad-digest';

const chunkPath = /\/api\/v1\/transfers\/[^/]+\/items\/[^/]+\/chunks\/(\d+)$/;
let transfers: Array<{ id: string; deleteToken: string }>;
let contexts: BrowserContext[];

test.setTimeout(180_000);

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

async function config(page: Page): Promise<Config> {
    await page.goto('/');
    const value = await page.evaluate(() => {
        const source = document.querySelector<HTMLScriptElement>('script[data-page]')?.textContent;
        return source
            ? (JSON.parse(source) as { props?: { filebeam?: Config } }).props?.filebeam
            : undefined;
    });
    if (!value?.chunk_bytes)
        throw new Error('The production page did not expose its chunk policy.');
    return value;
}

async function receiver(browser: Browser, mode: WritableMode = 'plain'): Promise<Page> {
    const context = await browser.newContext();
    contexts.push(context);
    await context.addInitScript((intercept) => {
        const writable = { chunks: [] as ArrayBuffer[], closed: false, aborted: false };
        Object.assign(window, {
            __turboWritable: writable,
            showSaveFilePicker: async () => ({
                createWritable: async () => ({
                    write: async (bytes: Uint8Array) => writable.chunks.push(bytes.slice().buffer),
                    close: async () => {
                        writable.closed = true;
                    },
                    abort: async () => {
                        writable.aborted = true;
                    },
                }),
            }),
        });
        if (intercept === 'plain') return;
        const NativeWorker = window.Worker;
        class InterceptedWorker {
            readonly worker: Worker;
            readonly listeners = new Map<EventListenerOrEventListenerObject, EventListener>();
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
                    const type =
                        intercept === 'bad-descriptor' ? 'descriptor-manifest' : 'manifest';
                    if (data?.type === type) {
                        const manifest = JSON.parse(data.manifest);
                        if (intercept === 'bad-descriptor') manifest.items[0].chunk_count++;
                        else manifest.items[0].digest.value = '0'.repeat(64);
                        const altered = new MessageEvent('message', {
                            data: { ...data, manifest: JSON.stringify(manifest) },
                        });
                        return typeof listener === 'function'
                            ? listener(altered)
                            : listener.handleEvent(altered);
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
        Object.defineProperty(window, 'Worker', { configurable: true, value: InterceptedWorker });
    }, mode);
    return context.newPage();
}

async function startTurbo(page: Page, bytes: Buffer, password?: string): Promise<Transfer> {
    const creation = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' &&
            response.url().endsWith('/api/v1/transfers') &&
            response.status() === 201,
    );
    await page.locator('#filebeam-picker').setInputFiles({
        name: 'turbo-stream.bin',
        mimeType: 'application/octet-stream',
        buffer: bytes,
    });
    if (password) await page.locator('#transfer-password').fill(password);
    await page.getByRole('button', { name: 'Turbo Transfer' }).click();
    const data = (await (await creation).json()).data as {
        id: string;
        delete_token: string;
        monitor_token?: string;
    };
    expect(data.monitor_token).toMatch(/^[A-Za-z0-9]{64}$/);
    transfers.push({ id: data.id, deleteToken: data.delete_token });
    await expect(page.getByRole('heading', { name: 'Your encrypted link is ready' })).toBeVisible();
    return {
        id: data.id,
        deleteToken: data.delete_token,
        link: await page.locator('#share-link').inputValue(),
        digest: createHash('sha256').update(bytes).digest('hex'),
    };
}

test('publishes a Turbo files link before completion and reports anonymous waiting downloads', async ({
    browser,
    page,
    request,
}) => {
    const { chunk_bytes } = await config(page);
    const content = Buffer.alloc(chunk_bytes + 37, 0x5a);
    let releaseLater!: () => void;
    const laterChunks = new Promise<void>((resolve) => {
        releaseLater = resolve;
    });
    const order: string[] = [];
    const monitorTokens: string[] = [];
    page.on('request', (outgoing) => {
        if (outgoing.method() === 'PUT' && outgoing.url().endsWith('/descriptor'))
            order.push('descriptor');
        if (outgoing.method() === 'PUT' && chunkPath.test(outgoing.url())) order.push('chunk');
        if (outgoing.url().endsWith('/monitor'))
            monitorTokens.push(outgoing.headers()['x-filebeam-monitor-token'] ?? '');
    });
    await page.route('**/api/v1/transfers/*/items/*/chunks/*', async (route) => {
        if (route.request().method() !== 'PUT') return route.continue();
        if (Number(route.request().url().match(chunkPath)?.[1]) >= 1) await laterChunks;
        await route.continue();
    });

    try {
        const transfer = await startTurbo(page, content);
        await expect.poll(() => order.includes('chunk')).toBe(true);
        expect(order.indexOf('descriptor')).toBeGreaterThanOrEqual(0);
        expect(order.indexOf('descriptor')).toBeLessThan(order.indexOf('chunk'));
        const uploadProgress = page.getByRole('progressbar', { name: 'Turbo Transfer upload' });
        await expect
            .poll(async () => Number(await uploadProgress.getAttribute('aria-valuenow')))
            .toBeGreaterThan(0);
        const scale = () =>
            uploadProgress.locator('.smooth-progress__fill').evaluate((fill) => {
                const transform = getComputedStyle(fill).transform;
                return Number(transform.match(/^matrix\(([^,]+)/)?.[1] ?? 0);
            });
        const snapshots = [await scale()];
        await page.waitForTimeout(120);
        snapshots.push(await scale());
        await page.waitForTimeout(120);
        snapshots.push(await scale());
        expect(
            snapshots.every(
                (value, index) => value <= 0.99 && (!index || value >= snapshots[index - 1]),
            ),
        ).toBe(true);
        const fill = uploadProgress.locator('.smooth-progress__fill');
        await expect
            .poll(() =>
                fill.evaluate((element) => getComputedStyle(element, '::after').animationName),
            )
            .toContain('progress-sheen');
        const glow = await fill.evaluate((element) => {
            const animation = element
                .getAnimations({ subtree: true })
                .find((entry) =>
                    (entry as CSSAnimation).animationName.startsWith('progress-sheen'),
                )!;
            animation.pause();
            animation.currentTime = 3000;
            const first = getComputedStyle(element, '::after').transform;
            animation.currentTime = 3700;
            const second = getComputedStyle(element, '::after').transform;
            animation.play();
            return { first, second };
        });
        expect(glow.first).not.toBe(glow.second);
        await page.emulateMedia({ reducedMotion: 'reduce' });
        await expect.poll(scale).toBe(0.99);
        await expect
            .poll(() =>
                fill.evaluate((element) => getComputedStyle(element, '::after').animationName),
            )
            .toBe('none');
        const reduced = await scale();
        await page.waitForTimeout(120);
        expect(await scale()).toBe(reduced);
        const readiness = await request.get(`/api/v1/transfers/${transfer.id}/progress`);
        expect(readiness.ok()).toBe(true);
        expect(await readiness.json()).toMatchObject({
            data: { status: 'pending', progress: expect.any(Number), items: [{ ready_chunks: 1 }] },
        });

        const cancelled = await receiver(browser);
        await cancelled.goto(transfer.link);
        await expect(cancelled.getByText('turbo-stream.bin')).toBeVisible();
        await expect(
            cancelled.getByRole('heading', { name: '1 file ready to download' }),
        ).toBeVisible();
        for (const width of [1280, 375]) {
            await cancelled.setViewportSize({ width, height: 812 });
            const senderLabel = await cancelled
                .getByText('Sender is still uploading', { exact: true })
                .boundingBox();
            const receiverLabel = await cancelled
                .getByText('Ready to download', { exact: true })
                .boundingBox();
            expect(Math.abs(senderLabel!.x - receiverLabel!.x)).toBeLessThan(1);
        }
        await cancelled.getByRole('button', { name: 'Download files' }).click();
        await expect(cancelled.getByRole('heading', { name: '1 file downloading' })).toBeVisible();
        await expect
            .poll(() => cancelled.evaluate(() => (window as any).__turboWritable.chunks.length))
            .toBeGreaterThan(0);
        await expect(cancelled.getByText(/Waiting to resume/i)).toBeVisible();
        await expect(page.getByText('Download 1', { exact: true })).toBeVisible({
            timeout: 15_000,
        });
        const firstProgress = page.getByRole('progressbar', { name: 'Download 1', exact: true });
        await expect
            .poll(async () => Number(await firstProgress.getAttribute('aria-valuenow')))
            .toBeGreaterThan(0);
        expect(Number(await firstProgress.getAttribute('aria-valuenow'))).toBeLessThan(100);
        await cancelled.getByRole('button', { name: 'Cancel' }).click();
        await expect
            .poll(() => cancelled.evaluate(() => (window as any).__turboWritable.aborted))
            .toBe(true);
        await expect(
            cancelled.getByRole('button', { name: 'Download files', exact: true }),
        ).toBeVisible();
        await expect(cancelled.getByRole('button', { name: 'Download files again' })).toHaveCount(
            0,
        );
        await page.emulateMedia({ reducedMotion: 'no-preference' });

        const download = await receiver(browser);
        await download.goto(transfer.link);
        await expect(download.getByText('turbo-stream.bin')).toBeVisible();
        await download.getByRole('button', { name: 'Download files' }).click();
        await expect
            .poll(() => download.evaluate(() => (window as any).__turboWritable.chunks.length))
            .toBeGreaterThan(0);
        expect(await download.evaluate(() => (window as any).__turboWritable.closed)).toBe(false);
        await expect(page.getByText('Download 2', { exact: true })).toBeVisible({
            timeout: 15_000,
        });
        await expect(page.getByText(/Waiting to resume|Decrypting download/i)).toBeVisible();
        expect(monitorTokens.some(Boolean)).toBe(true);
        const monitor = await (
            await request.get(`/api/v1/transfers/${transfer.id}/monitor`, {
                headers: { 'X-Filebeam-Monitor-Token': monitorTokens.find(Boolean)! },
            })
        ).json();
        expect(monitor.data.sessions).toEqual(
            expect.arrayContaining([expect.objectContaining({ number: 1, status: 'cancelled' })]),
        );
        expect(JSON.stringify(monitor.data.sessions)).not.toMatch(/token|identity|item_ids/i);
        await page.screenshot({ path: test.info().outputPath('turbo-sender.png'), fullPage: true });
        await download.screenshot({
            path: test.info().outputPath('turbo-receiver.png'),
            fullPage: true,
        });

        releaseLater();
        await expect
            .poll(() => download.evaluate(() => (window as any).__turboWritable.closed))
            .toBe(true);
        await expect(download.getByRole('heading', { name: '1 file downloaded' })).toBeVisible();
        await expect(download.getByText('Download finished and integrity verified.')).toBeVisible();
        await expect(download.getByRole('button', { name: 'Download files again' })).toBeVisible();
        await expect(
            download.getByRole('button', { name: 'Download files', exact: true }),
        ).toHaveCount(0);
        await expect
            .poll(() =>
                download
                    .getByRole('progressbar', { name: 'Download completed' })
                    .locator('.smooth-progress__fill')
                    .evaluate((element) => getComputedStyle(element, '::after').animationName),
            )
            .toBe('none');
        expect(
            await download.evaluate(async () => {
                const chunks = (window as any).__turboWritable.chunks as ArrayBuffer[];
                const bytes = new Uint8Array(
                    chunks.reduce((size, chunk) => size + chunk.byteLength, 0),
                );
                let offset = 0;
                for (const chunk of chunks) {
                    bytes.set(new Uint8Array(chunk), offset);
                    offset += chunk.byteLength;
                }
                return [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))]
                    .map((byte) => byte.toString(16).padStart(2, '0'))
                    .join('');
            }),
        ).toBe(transfer.digest);
        await expect(page.getByText('Completed', { exact: true })).toBeVisible({ timeout: 15_000 });
        await expect(
            page.getByRole('progressbar', { name: 'Download 2', exact: true }),
        ).toHaveAttribute('aria-valuenow', '100');
        const completedBar = page.getByRole('progressbar', { name: 'Download 2', exact: true });
        for (const width of [1280, 375]) {
            await page.setViewportSize({ width, height: 812 });
            const metadata = await completedBar.locator('.download-monitor__meta').boundingBox();
            const track = await completedBar.locator('.smooth-progress__track').boundingBox();
            expect(track!.y - (metadata!.y + metadata!.height)).toBeGreaterThanOrEqual(8);
        }
        await expect
            .poll(() =>
                completedBar
                    .locator('.smooth-progress__fill')
                    .evaluate((element) => getComputedStyle(element, '::after').animationName),
            )
            .toBe('none');
        await download.setViewportSize({ width: 375, height: 812 });
        await download.screenshot({
            path: test.info().outputPath('turbo-completed-mobile.png'),
            fullPage: true,
        });
    } finally {
        releaseLater();
    }
});

test('keeps normal Encrypt and share transfers private until every chunk completes', async ({
    page,
}) => {
    await config(page);
    let release!: () => void;
    const held = new Promise<void>((resolve) => {
        release = resolve;
    });
    let descriptorRequests = 0;
    page.on('request', (request) => {
        if (request.method() === 'PUT' && request.url().endsWith('/descriptor'))
            descriptorRequests++;
    });
    await page.route('**/api/v1/transfers/*/items/*/chunks/*', async (route) => {
        if (route.request().method() === 'PUT') await held;
        await route.continue();
    });
    try {
        await page.locator('#filebeam-picker').setInputFiles({
            name: 'ordinary.bin',
            mimeType: 'application/octet-stream',
            buffer: Buffer.from('ordinary uploads do not publish early'),
        });
        await page.getByRole('button', { name: 'Encrypt and share' }).click();
        await page.waitForTimeout(300);
        expect(descriptorRequests).toBe(0);
        await expect(page.locator('#share-link')).toHaveCount(0);
        release();
        await expect(
            page.getByRole('heading', { name: 'Your encrypted link is ready' }),
        ).toBeVisible();
    } finally {
        release();
    }
});

test('rejects a Turbo descriptor mismatch and a corrupted final digest without closing the file', async ({
    browser,
    page,
}) => {
    await config(page);
    let releaseComplete!: () => void;
    const completion = new Promise<void>((resolve) => {
        releaseComplete = resolve;
    });
    await page.route('**/api/v1/transfers/*/complete', async (route) => {
        await completion;
        await route.continue();
    });
    try {
        const transfer = await startTurbo(page, Buffer.from('turbo integrity check'));
        await expect(page.getByText('Retention starts when uploading finishes.')).toBeVisible();

        const mismatched = await receiver(browser, 'bad-descriptor');
        await mismatched.goto(transfer.link);
        await expect(mismatched.getByRole('alert')).toContainText(
            /descriptor|incomplete|altered|metadata/i,
        );

        const corrupted = await receiver(browser, 'bad-digest');
        await corrupted.goto(transfer.link);
        await expect(corrupted.getByText('turbo-stream.bin')).toBeVisible();
        await corrupted.getByRole('button', { name: 'Download files' }).click();
        await expect
            .poll(() => corrupted.evaluate(() => (window as any).__turboWritable.chunks.length))
            .toBeGreaterThan(0);
        releaseComplete();
        await expect(corrupted.getByRole('alert')).toContainText(/integrity check/i);
        expect(await corrupted.evaluate(() => (window as any).__turboWritable)).toMatchObject({
            closed: false,
            aborted: true,
        });
        await expect(corrupted.getByRole('button', { name: 'Download files again' })).toHaveCount(
            0,
        );
        await expect(corrupted.getByRole('heading', { name: '1 file downloaded' })).toHaveCount(0);
    } finally {
        releaseComplete();
    }
});

test('offers Turbo only for files, to the left of normal sharing at desktop and mobile widths', async ({
    page,
}) => {
    await page.goto('/');
    await page.locator('#filebeam-picker').setInputFiles({
        name: 'turbo-option.txt',
        mimeType: 'text/plain',
        buffer: Buffer.from('turbo option'),
    });
    const turbo = page.getByRole('button', { name: 'Turbo Transfer' });
    const normal = page.getByRole('button', { name: 'Encrypt and share' });
    const tooltip = page.getByText('Share the link while files are still uploading.', {
        exact: true,
    });
    await expect(turbo).toHaveCount(1);
    await expect(normal).toHaveCount(1);
    await expect(tooltip).toHaveCount(0);
    await turbo.hover();
    await expect(tooltip).toBeVisible();
    await page.keyboard.press('Escape');
    await turbo.focus();
    await expect(tooltip).toBeVisible();
    for (const width of [1280, 375]) {
        await page.setViewportSize({ width, height: 812 });
        expect((await turbo.boundingBox())!.x).toBeLessThan((await normal.boundingBox())!.x);
        await expect
            .poll(() =>
                page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
            )
            .toBe(true);
    }
    await page.waitForTimeout(350);
    await page.screenshot({ path: test.info().outputPath('turbo-mobile.png'), fullPage: true });
    await page.getByRole('tab', { name: 'Notes' }).click();
    await expect(turbo).toHaveCount(0);
});

test('unlocks a password-protected Turbo link before its small upload completes', async ({
    browser,
    page,
}) => {
    await config(page);
    let releaseComplete!: () => void;
    const completion = new Promise<void>((resolve) => {
        releaseComplete = resolve;
    });
    await page.route('**/api/v1/transfers/*/complete', async (route) => {
        await completion;
        await route.continue();
    });
    try {
        const transfer = await startTurbo(page, Buffer.from('password turbo'), 'turbo-password');
        const download = await receiver(browser);
        await download.goto(transfer.link);
        await expect(download.getByRole('heading', { name: 'Enter the password' })).toBeVisible();
        await download.locator('#transfer-password').fill('turbo-password');
        await download.getByRole('button', { name: 'Unlock' }).click();
        await expect(download.getByText('turbo-stream.bin')).toBeVisible();
        releaseComplete();
        await expect(page.getByText('Retention starts when uploading finishes.')).toHaveCount(0);
    } finally {
        releaseComplete();
    }
});

test('tracks successful file selections and supports downloading them again', async ({
    browser,
    page,
}) => {
    await config(page);
    await page.locator('#filebeam-picker').setInputFiles(
        ['first.txt', 'second.txt'].map((name) => ({
            name,
            mimeType: 'text/plain',
            buffer: Buffer.from(`Contents of ${name}`),
        })),
    );
    const creation = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' &&
            response.url().endsWith('/api/v1/transfers') &&
            response.status() === 201,
    );
    await page.getByRole('button', { name: 'Turbo Transfer' }).click();
    const reservation = (await (await creation).json()).data as {
        id: string;
        delete_token: string;
    };
    transfers.push({ id: reservation.id, deleteToken: reservation.delete_token });
    await expect(page.locator('#share-link')).toBeVisible();
    const download = await receiver(browser);
    await download.goto(await page.locator('#share-link').inputValue());
    await expect(
        download.getByRole('heading', { name: '2 files ready to download' }),
    ).toBeVisible();
    let release!: () => void;
    let chunks = new Promise<void>((resolve) => {
        release = resolve;
    });
    await download.route('**/api/v1/transfers/*/items/*/chunks/*', async (route) => {
        await chunks;
        await route.continue();
    });
    try {
        await download.getByRole('button', { name: 'Download first.txt', exact: true }).click();
        await expect(download.getByRole('heading', { name: '1 file downloading' })).toBeVisible();
        release();
        await expect(
            download.getByRole('heading', { name: '1 of 2 files downloaded' }),
        ).toBeVisible();
        await expect(
            download.getByRole('button', { name: 'Download second.txt', exact: true }),
        ).toBeVisible();
        chunks = new Promise<void>((resolve) => {
            release = resolve;
        });
        await download.getByRole('button', { name: 'Download first.txt again' }).click();
        await expect(download.getByRole('heading', { name: '1 file downloading' })).toBeVisible();
        release();
        await expect(
            download.getByRole('heading', { name: '1 of 2 files downloaded' }),
        ).toBeVisible();
        await download.getByRole('button', { name: 'Download second.txt', exact: true }).click();
        await expect(download.getByRole('heading', { name: '2 files downloaded' })).toBeVisible();
        await expect(
            download.getByRole('button', { name: 'Download second.txt again' }),
        ).toBeVisible();
    } finally {
        release();
    }
});
