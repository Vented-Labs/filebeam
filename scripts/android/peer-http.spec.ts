import { createHash } from 'node:crypto';
import { createWriteStream, mkdirSync } from 'node:fs';
import { stat } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { expect, test, type Page } from '@playwright/test';

const run = promisify(execFile);
const root = resolve(import.meta.dirname, '../..');
const results = process.env.PEER_RESULTS!;
const fixtures = join(results, 'fixtures');
const binary = process.env.BROWSER_TEST_CLI_BINARY!;
const large = process.env.PEER_LARGE === 'large';

test.setTimeout(large ? 7_200_000 : 300_000);

async function digest(path: string): Promise<string> {
    const { stdout } = await run('sha256sum', [path]);
    return stdout.split(/\s+/)[0];
}

async function nativeWritable(page: Page, output: string): Promise<() => Promise<string>> {
    mkdirSync(resolve(output, '..'), { recursive: true });
    const hash = createHash('sha256');
    const stream = createWriteStream(output);
    await page.exposeBinding('__peerWrite', async (_source, bytes: Buffer) => {
        const chunk = Buffer.from(bytes);
        hash.update(chunk);
        if (!stream.write(chunk)) await new Promise<void>((done) => stream.once('drain', done));
    });
    await page.exposeBinding('__peerClose', async () => {
        await new Promise<void>((done, fail) => stream.end((error?: Error | null) => (error ? fail(error) : done())));
    });
    await page.addInitScript(() => {
        Object.defineProperty(window, 'showSaveFilePicker', {
            configurable: true,
            value: async () => ({
                createWritable: async () => ({
                    // The native binding consumes one decrypted chunk at a time; no full-file buffer exists.
                    write: async (bytes: Uint8Array) => window.__peerWrite(bytes),
                    close: async () => window.__peerClose(),
                    abort: async () => window.__peerClose(),
                }),
            }),
        });
    });
    return async () => {
        await new Promise<void>((done) => stream.once('close', done));
        return hash.digest('hex');
    };
}

declare global {
    interface Window {
        __peerWrite(bytes: Uint8Array): Promise<void>;
        __peerClose(): Promise<void>;
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

test('CLI and browser exchange encrypted HTTP files with streamed native writable output', async ({ page }) => {
    const webSource = join(fixtures, 'web-to-cli.txt');
    await page.goto('/');
    await page.locator('#filebeam-picker').setInputFiles(webSource);
    await page.getByRole('button', { name: 'Encrypt and share' }).click();
    await expect(page.locator('#share-link')).toBeVisible();
    const webLink = await page.locator('#share-link').inputValue();
    const webOutput = join(results, 'downloads', 'web-to-cli.txt');
    await run(binary, ['--plain', 'down', webLink, '--output', join(results, 'downloads')], {
        env: { ...process.env, FILEBEAM_INSTANCE: process.env.BASE_URL!, FILEBEAM_HOME: join(results, 'cli-home') },
    });
    expect(await digest(webOutput)).toBe(await digest(webSource));

    const cliSource = join(fixtures, 'cli-to-web.txt');
    const cliLink = await uploadFromCli(cliSource, ['--retention-hours', '24']);
    const close = await nativeWritable(page, join(results, 'downloads', 'cli-to-web.browser.txt'));
    await page.goto(cliLink);
    await page.getByRole('button', { name: 'Download files', exact: true }).click();
    await expect(page.getByText('Download finished and integrity verified.')).toBeVisible();
    expect(await close()).toBe(await digest(cliSource));
});

test('browser password, retention, and Turbo HTTP flow decrypts through the native writable adapter', async ({ page }) => {
    const source = join(fixtures, 'web-to-cli.txt');
    await page.goto('/');
    await page.locator('#filebeam-picker').setInputFiles(source);
    await page.getByRole('combobox', { name: 'Retention period' }).click();
    await page.getByRole('option', { name: /24 hours/i }).click();
    await page.getByTestId('prism-password-trigger').click();
    const password = page.getByTestId('prism-password-popover').locator('#transfer-password');
    await password.fill('peer-http-password');
    await page.getByTestId('prism-password-popover').getByRole('button', { name: 'Done' }).click();
    await page.getByRole('button', { name: 'Turbo Transfer' }).click();
    await expect(page.locator('#share-link')).toBeVisible();
    const link = await page.locator('#share-link').inputValue();
    const close = await nativeWritable(page, join(results, 'downloads', 'turbo.browser.txt'));
    await page.goto(link);
    await page.locator('#transfer-password').fill('peer-http-password');
    await page.getByRole('button', { name: 'Unlock' }).click();
    await page.getByRole('button', { name: 'Download files', exact: true }).click();
    await expect(page.getByText('Download finished and integrity verified.')).toBeVisible();
    expect(await close()).toBe(await digest(source));
});

test('513 MiB and 4097 MiB CLI-to-browser files retain streamed hash and bounded browser output', async ({ page }) => {
    test.skip(!large, 'run with peer-http.sh run-large');
    for (const name of ['peer-513MiB.bin', 'peer-4097MiB.bin']) {
        const source = join(fixtures, name);
        const link = await uploadFromCli(source);
        const output = join(results, 'downloads', `${name}.browser`);
        const close = await nativeWritable(page, output);
        await page.goto(link);
        await page.getByRole('button', { name: 'Download files', exact: true }).click();
        await expect(page.getByText('Download finished and integrity verified.')).toBeVisible();
        expect(await close()).toBe(await digest(source));
        expect((await stat(output)).size).toBe((await stat(source)).size);
    }
});
