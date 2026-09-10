import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';

type Transfer = { id: string; deleteToken: string };
type CreatedTransfer = Transfer & { link: string; content: Buffer; digest: string };
type Completion = { data: { id: string } };

const directChunk = /\/api\/v1\/transfers\/[^/]+\/items\/[^/]+\/chunks\/\d+$/;
const stagePart = /\/uploads\/([^/]+)\/parts\/(\d+)$/;
const stageStatus = /\/uploads\/[^/]+$/;
const stageComplete = /\/uploads\/[^/]+\/complete$/;
const transferComplete = /\/api\/v1\/transfers\/[^/]+\/complete$/;

let transfers: Transfer[];
let contexts: BrowserContext[];
let releaseGates: Array<() => void>;

test.beforeEach(() => {
    transfers = [];
    contexts = [];
    releaseGates = [];
});

test.afterEach(async ({ request }) => {
    // Release intercepted requests before closing contexts so failed assertions cannot leak them.
    for (const release of releaseGates.splice(0)) release();
    await Promise.all(
        transfers.map(({ id, deleteToken }) =>
            request.delete(`/api/v1/transfers/${id}`, {
                headers: { 'X-Filebeam-Delete-Token': deleteToken },
            }),
        ),
    );
    await Promise.all(contexts.map((context) => context.close()));
});

function content(bytes: number): Buffer {
    return Buffer.from(Array.from({ length: bytes }, (_, index) => index % 251));
}

async function setFile(page: Page, name: string, buffer: Buffer): Promise<void> {
    await page.goto('/');
    await page.locator('#filebeam-picker').setInputFiles({
        name,
        mimeType: 'application/octet-stream',
        buffer,
    });
}

async function startUpload(page: Page): Promise<Transfer> {
    const created = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' &&
            response.url().endsWith('/api/v1/transfers') &&
            response.status() === 201,
    );
    await page.getByRole('button', { name: 'Encrypt and share' }).click();
    const payload = (await (await created).json()) as {
        data: { id: string; delete_token: string };
    };
    const transfer = { id: payload.data.id, deleteToken: payload.data.delete_token };
    transfers.push(transfer);
    return transfer;
}

async function recipient(browser: Browser): Promise<Page> {
    const context = await browser.newContext();
    contexts.push(context);
    await context.addInitScript(() => {
        Object.defineProperty(window, 'showSaveFilePicker', {
            value: undefined,
            configurable: true,
        });
    });
    return context.newPage();
}

test('recovers a persisted staged part, reports progress before final storage, and round-trips one ciphertext chunk', async ({
    browser,
    page,
}) => {
    const plaintext = content(3 * 1024 * 1024 + 37);
    const digest = createHash('sha256').update(plaintext).digest('hex');
    const partRequests = new Map<number, number>();
    const partSizes = new Map<number, number>();
    const persistedOffsets: number[] = [];
    const downloads: number[] = [];
    let lostFirstPart = false;
    let stageCompleteReached = false;
    let releaseStageComplete!: () => void;
    const stageCompleteReleased = new Promise<void>((resolve) => {
        releaseStageComplete = resolve;
        releaseGates.push(resolve);
    });
    let lostTransferComplete = false;
    const completions: Completion[] = [];

    await page.route(directChunk, async (route) => {
        if (route.request().method() !== 'PUT') return route.continue();
        await route.fulfill({ status: 413, body: 'use staged upload' });
    });
    await page.route(stageStatus, async (route) => {
        if (route.request().method() !== 'GET') return route.continue();
        const response = await route.fetch();
        const payload = (await response.json()) as { data?: { offset?: number } };
        if (typeof payload.data?.offset === 'number') persistedOffsets.push(payload.data.offset);
        await route.fulfill({ response, json: payload });
    });
    await page.route(stagePart, async (route) => {
        if (route.request().method() !== 'PUT') return route.continue();
        const match = route.request().url().match(stagePart);
        const offset = Number(match?.[2]);
        partRequests.set(offset, (partRequests.get(offset) ?? 0) + 1);
        partSizes.set(offset, route.request().postDataBuffer()?.byteLength ?? 0);
        if (offset === 0 && !lostFirstPart) {
            lostFirstPart = true;
            await route.fetch(); // Laravel has accepted the bytes; only this acknowledgement is lost.
            return route.abort();
        }
        await route.continue();
    });
    await page.route(stageComplete, async (route) => {
        if (route.request().method() !== 'POST') return route.continue();
        stageCompleteReached = true;
        await stageCompleteReleased;
        await route.continue();
    });
    await page.route(transferComplete, async (route) => {
        if (route.request().method() !== 'POST' || lostTransferComplete) return route.continue();
        lostTransferComplete = true;
        const response = await route.fetch();
        completions.push((await response.json()) as Completion);
        // Verify the client retries a response lost after idempotent completion.
        await route.abort();
    });

    await setFile(page, 'staged-roundtrip.bin', plaintext);
    await startUpload(page);
    await expect.poll(() => stageCompleteReached, { timeout: 30_000 }).toBe(true);
    const progress = page.getByRole('progressbar', { name: 'staged-roundtrip.bin progress' });
    await expect(progress).toBeVisible();
    expect(Number(await progress.getAttribute('aria-valuenow'))).toBeGreaterThan(0);
    expect(lostFirstPart).toBe(true);
    expect(persistedOffsets.some((offset) => offset > 0)).toBe(true);
    expect(partRequests.get(0)).toBe(1);

    releaseStageComplete();
    await expect(page.getByRole('heading', { name: 'Your encrypted link is ready' })).toBeVisible({
        timeout: 30_000,
    });
    expect(lostTransferComplete).toBe(true);
    expect(completions).toHaveLength(1);
    expect(completions[0].data.id).toBe(transfers[0].id);
    // Rust owns initial sizing; verify the advertised transport bounds and no duplicate ranges.
    expect(partRequests.size).toBeGreaterThan(1);
    expect([...partRequests.values()].every((requests) => requests === 1)).toBe(true);
    expect(
        [...partSizes.values()].every((size) => size >= 64 * 1024 && size <= 4 * 1024 * 1024),
    ).toBe(true);
    const created: CreatedTransfer = {
        ...transfers[0],
        link: await page.locator('#share-link').inputValue(),
        content: plaintext,
        digest,
    };

    const receiver = await recipient(browser);
    let stagedDownloadRequests = 0;
    receiver.on('request', (request) => {
        if (request.method() === 'GET' && stagePart.test(request.url())) stagedDownloadRequests++;
    });
    await receiver.route(directChunk, async (route) => {
        if (route.request().method() !== 'GET') return route.continue();
        const response = await route.fetch();
        downloads.push(Number(response.headers()['content-length'] ?? 0));
        await route.fulfill({ response });
    });
    await receiver.goto(created.link);
    const download = receiver.waitForEvent('download');
    await receiver.getByRole('button', { name: 'Download files' }).click();
    const saved = await download;
    const path = await saved.path();
    expect(path).not.toBeNull();
    expect(
        createHash('sha256')
            .update(await readFile(path!))
            .digest('hex'),
    ).toBe(created.digest);
    expect(downloads).toHaveLength(1);
    expect(downloads[0]).toBeGreaterThanOrEqual(3 * 1024 * 1024);
    expect(stagedDownloadRequests).toBe(0);
});

test('retries a lost transfer completion acknowledgement without changing the published share', async ({
    page,
}) => {
    let completionRequests = 0;
    const completions: Completion[] = [];
    await page.route(transferComplete, async (route) => {
        if (route.request().method() !== 'POST') return route.continue();
        completionRequests++;
        const response = await route.fetch();
        const payload = (await response.json()) as Completion;
        completions.push(payload);
        if (completionRequests === 1) {
            return route.abort();
        }
        await route.fulfill({ response, json: payload });
    });

    await setFile(page, 'completion-retry.bin', content(128 * 1024 + 19));
    await startUpload(page);
    await expect(page.getByRole('heading', { name: 'Your encrypted link is ready' })).toBeVisible({
        timeout: 30_000,
    });
    const share = await page.locator('#share-link').inputValue();
    expect(completionRequests).toBe(2);
    expect(completions[1].data).toEqual(completions[0].data);
    expect(new URL(share).pathname).toBe(`/${completions[0].data.id}`);
    expect(share).toMatch(/#k=v1\./);
});

test('a cancelled download cannot abort the writable from its replacement download', async ({
    browser,
    page,
}) => {
    test.setTimeout(60_000);
    await setFile(page, 'download-cancel-race.bin', content(128 * 1024 + 19));
    await startUpload(page);
    await expect(page.getByRole('heading', { name: 'Your encrypted link is ready' })).toBeVisible({
        timeout: 30_000,
    });
    const link = await page.locator('#share-link').inputValue();
    const context = await browser.newContext();
    contexts.push(context);
    await context.addInitScript(() => {
        let writableIndex = 0;
        (
            window as Window & {
                __downloadWritables: Array<{ aborted: boolean; closed: boolean }>;
                __firstWriteEntered: boolean;
                __releaseFirstWrite?: () => void;
            }
        ).__downloadWritables = [];
        Object.defineProperty(window, 'showSaveFilePicker', {
            configurable: true,
            value: async () => {
                const index = writableIndex++;
                const writable = { aborted: false, closed: false };
                (
                    window as Window & {
                        __downloadWritables: Array<{ aborted: boolean; closed: boolean }>;
                    }
                ).__downloadWritables.push(writable);
                return {
                    createWritable: async () => ({
                        write: async () => {
                            if (index !== 0) return;
                            const state = window as Window & {
                                __firstWriteEntered: boolean;
                                __releaseFirstWrite?: () => void;
                            };
                            state.__firstWriteEntered = true;
                            await new Promise<void>((resolve) => {
                                state.__releaseFirstWrite = resolve;
                            });
                        },
                        close: async () => {
                            writable.closed = true;
                        },
                        abort: async () => {
                            writable.aborted = true;
                        },
                    }),
                };
            },
        });
    });
    const receiver = await context.newPage();
    let chunkGets = 0;
    let releaseSecondGet!: () => void;
    const secondGet = new Promise<void>((resolve) => {
        releaseSecondGet = resolve;
        releaseGates.push(resolve);
    });
    await receiver.route(directChunk, async (route) => {
        if (route.request().method() !== 'GET') return route.continue();
        chunkGets++;
        if (chunkGets === 2) await secondGet;
        await route.continue();
    });
    await receiver.goto(link);
    await receiver.getByRole('button', { name: 'Download files' }).click();
    await expect.poll(() => receiver.evaluate(() => window.__firstWriteEntered)).toBe(true);
    await receiver.getByRole('button', { name: 'Cancel' }).click();
    await receiver.getByRole('button', { name: 'Download files' }).click();
    await expect.poll(() => chunkGets).toBe(2);
    await receiver.evaluate(() => window.__releaseFirstWrite?.());
    await expect
        .poll(() => receiver.evaluate(() => window.__downloadWritables[0]?.aborted))
        .toBe(true);
    releaseSecondGet();
    await expect
        .poll(() => receiver.evaluate(() => window.__downloadWritables[1]?.closed), {
            timeout: 30_000,
        })
        .toBe(true);
    expect(await receiver.evaluate(() => window.__downloadWritables[1]?.aborted)).toBe(false);
});

test('a throttled uplink switches to smaller requests and keeps byte progress visible', async ({
    page,
}) => {
    test.setTimeout(90_000);
    await setFile(page, 'slow-uplink.bin', content(512 * 1024 + 13));
    // A short advertised proxy budget makes this a practical real-network regression test.
    await page.route('**/api/v1/transfers', async (route) => {
        if (route.request().method() !== 'POST') return route.continue();
        const response = await route.fetch();
        const payload = await response.json();
        payload.data.upload_transport.request_target_ms = 1_000;
        payload.data.upload_transport.request_budget_ms = 3_000;
        await route.fulfill({ response, json: payload });
    });
    let parts = 0;
    let acknowledgedChunks = 0;
    page.on('request', (request) => {
        if (request.method() === 'PUT' && stagePart.test(request.url())) parts++;
    });
    page.on('response', (response) => {
        if (response.ok() && directChunk.test(response.url())) acknowledgedChunks++;
    });
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Network.enable');
    await cdp.send('Network.emulateNetworkConditions', {
        offline: false,
        latency: 50,
        downloadThroughput: -1,
        uploadThroughput: 64 * 1024,
    });
    try {
        await startUpload(page);
        await expect
            .poll(
                async () =>
                    Number(
                        await page
                            .getByRole('progressbar', {
                                name: 'slow-uplink.bin progress',
                            })
                            .getAttribute('aria-valuenow'),
                    ),
                { timeout: 15_000 },
            )
            .toBeGreaterThan(0);
        expect(acknowledgedChunks).toBe(0);
        await expect(
            page.getByRole('heading', { name: 'Your encrypted link is ready' }),
        ).toBeVisible({ timeout: 60_000 });
        expect(parts).toBeGreaterThan(0);
    } finally {
        await cdp.send('Network.emulateNetworkConditions', {
            offline: false,
            latency: 0,
            downloadThroughput: -1,
            uploadThroughput: -1,
        });
        await cdp.detach();
    }
});
