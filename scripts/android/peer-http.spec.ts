import { createHash } from 'node:crypto';
import { mkdirSync } from 'node:fs';
import { appendFile } from 'node:fs/promises';
import { stat } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { chromium, expect, test, type Page } from '@playwright/test';

const run = promisify(execFile);
const results = process.env.PEER_RESULTS!;
const fixtures = join(results, 'fixtures');
const binary = process.env.BROWSER_TEST_CLI_BINARY!;
const large = process.env.PEER_LARGE === 'large';

test.setTimeout(large ? 7_200_000 : 300_000);

async function digest(path: string): Promise<string> {
    const { stdout } = await run('sha256sum', [path]);
    return stdout.split(/\s+/)[0];
}

async function nativeWritable(page: Page, label: string): Promise<() => Promise<string>> {
    const hash = createHash('sha256');
    let peakRss = 0;
    await page.exposeBinding('__peerWrite', async (_source, bytes: Buffer) => {
        const chunk = Buffer.from(bytes);
        hash.update(chunk);
        const { stdout } = await run('ps', ['-C', 'chrome-headless-shell', '-o', 'rss=']);
        const rss = stdout.split(/\s+/).reduce((total, value) => total + Number(value || 0), 0);
        peakRss = Math.max(peakRss, rss);
    });
    await page.addInitScript(() => {
        Object.defineProperty(window, 'showSaveFilePicker', {
            configurable: true,
            value: async () => {
                // OPFS gives headless Chromium a real file handle without retaining the file in JS memory.
                const root = await navigator.storage.getDirectory();
                const name = `peer-${crypto.randomUUID()}`;
                const file = await root.getFileHandle(name, { create: true });
                window.__peerCleanup = () => root.removeEntry(name);
                return {
                    createWritable: async () => {
                        const writable = await file.createWritable();
                        return {
                            write: async (bytes: Uint8Array) => {
                                await window.__peerWrite(bytes);
                                await writable.write(bytes);
                            },
                            close: async () => writable.close(),
                            abort: async () => writable.abort(),
                        };
                    },
                };
            },
        });
    });
    return async () => {
        await appendFile(
            join(results, 'logs', 'browser-rss.tsv'),
            `${label}\tpeak_chromium_rss_kib=${peakRss}\tnode_rss=${process.memoryUsage().rss}\n`,
        );
        await page.evaluate(() => window.__peerCleanup());
        return hash.digest('hex');
    };
}

declare global {
    interface Window {
        __peerWrite(bytes: Uint8Array): Promise<void>;
        __peerClose(): Promise<void>;
        __peerCleanup(): Promise<void>;
    }
}

async function uploadFromCli(file: string, extra: string[] = []): Promise<string> {
    const home = join(results, 'cli-home');
    mkdirSync(home, { recursive: true });
    const { stdout } = await run(binary, ['--plain', '--memory-limit-mib', '128', 'up', file, ...extra], {
        env: { ...process.env, FILEBEAM_INSTANCE: process.env.BASE_URL!, FILEBEAM_HOME: home },
        timeout: large ? 7_000_000 : 240_000,
    });
    return stdout.trim().split(/\s+/).at(-1)!;
}

test('browser uploads and CLI downloads an encrypted HTTP file', async ({ page }) => {
    const webSource = join(fixtures, 'web-to-cli.txt');
    await page.goto('/');
    await page.locator('#filebeam-picker').setInputFiles(webSource);
    await page.getByRole('button', { name: /Encrypt and share|Send encrypted/ }).click();
    await expect(page.locator('#share-link')).toBeVisible();
    const webLink = await page.locator('#share-link').inputValue();
    const webOutput = join(results, 'downloads', 'web-to-cli.txt');
    await run(binary, ['--plain', 'down', webLink, '--output', join(results, 'downloads')], {
        env: { ...process.env, FILEBEAM_INSTANCE: process.env.BASE_URL!, FILEBEAM_HOME: join(results, 'cli-home') },
    });
    expect(await digest(webOutput)).toBe(await digest(webSource));

});

test('CLI uploads and browser downloads an encrypted HTTP file through OPFS', async ({ page }) => {
    const cliSource = join(fixtures, 'cli-to-web.txt');
    const cliLink = await uploadFromCli(cliSource, ['--retention-hours', '24']);
    const close = await nativeWritable(page, 'cli-to-web');
    await page.goto(cliLink);
    await page.getByRole('button', { name: 'Download files', exact: true }).click();
    await expect(page.getByRole('heading', { name: '1 file downloaded' })).toBeVisible();
    expect(await close()).toBe(await digest(cliSource));
});

test('browser password, retention, and Turbo downloader starts before sender finalization', async ({
    page,
    browser,
}) => {
    const source = join(fixtures, 'web-to-cli.txt');
    let releaseCompletion!: () => void;
    const completion = new Promise<void>((done) => {
        releaseCompletion = done;
    });
    await page.route('**/api/v1/transfers/*/complete', async (route) => {
        await completion;
        await route.continue();
    });
    await page.goto('/');
    await page.locator('#filebeam-picker').setInputFiles(source);
    await page.getByRole('combobox', { name: 'Retention period' }).click();
    await page.getByRole('option', { name: '1 day', exact: true }).click();
    await page.getByTestId('prism-password-trigger').click();
    const password = page.getByTestId('prism-password-popover').locator('#transfer-password');
    await password.fill('peer-http-password');
    await page.getByTestId('prism-password-popover').getByRole('button', { name: 'Done' }).click();
    await page.getByRole('button', { name: 'Turbo Transfer' }).click();
    await expect(page.locator('#share-link')).toBeVisible();
    const link = await page.locator('#share-link').inputValue();
    const receiver = await browser.newPage();
    const close = await nativeWritable(receiver, 'turbo');
    await receiver.goto(link);
    await receiver.locator('#transfer-password').fill('peer-http-password');
    await receiver.getByRole('button', { name: 'Unlock' }).click();
    await receiver.getByRole('button', { name: 'Download files', exact: true }).click();
    await expect(receiver.getByRole('heading', { name: '1 file downloading' })).toBeVisible();
    releaseCompletion();
    await expect(receiver.getByText('Download finished and integrity verified.')).toBeVisible();
    expect(await close()).toBe(await digest(source));
    await receiver.close();
});

test('513 MiB and 4097 MiB browser-to-CLI files retain streamed hashes', async ({ page }) => {
    test.skip(!large, 'run with peer-http.sh run-large');
    for (const name of ['peer-513MiB.bin', 'peer-4097MiB.bin']) {
        const source = join(fixtures, name);
        await page.goto('/');
        await page.locator('#filebeam-picker').setInputFiles(source);
        await page.getByRole('button', { name: /Encrypt and share|Send encrypted/ }).click();
        await expect(page.locator('#share-link')).toBeVisible({ timeout: 7_000_000 });
        const link = await page.locator('#share-link').inputValue();
        const outputDirectory = join(results, 'downloads', `browser-to-cli-${name}`);
        const output = join(outputDirectory, name);
        mkdirSync(outputDirectory, { recursive: true });
        await run(binary, ['--plain', 'down', link, '--output', outputDirectory], {
            env: { ...process.env, FILEBEAM_INSTANCE: process.env.BASE_URL!, FILEBEAM_HOME: join(results, 'cli-home') },
            timeout: 7_000_000,
        });
        expect(await digest(output)).toBe(await digest(source));
        expect((await stat(output)).size).toBe((await stat(source)).size);
    }
});

test('513 MiB and 4097 MiB CLI-to-browser files retain streamed OPFS hashes', async () => {
    test.skip(!large, 'run with peer-http.sh run-large');
    const context = await chromium.launchPersistentContext(join(results, 'opfs-profile'), {
        headless: true,
    });
    try {
        for (const name of ['peer-513MiB.bin', 'peer-4097MiB.bin']) {
            const page = await context.newPage();
            const source = join(fixtures, name);
            const link = await uploadFromCli(source);
            const close = await nativeWritable(page, `cli-${name}`);
            await page.goto(link);
            await page.getByRole('button', { name: 'Download files', exact: true }).click();
            await expect(page.getByRole('heading', { name: '1 file downloaded' })).toBeVisible({
                timeout: 7_000_000,
            });
            expect(await close()).toBe(await digest(source));
            await page.close();
        }
    } finally {
        await context.close();
    }
});
