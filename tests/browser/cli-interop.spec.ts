import { expect, test } from '@playwright/test';
import { execFile, type ChildProcess } from 'node:child_process';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { createServer, request as httpRequest } from 'node:http';
import { request as httpsRequest } from 'node:https';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { promisify } from 'node:util';

const run = promisify(execFile);

test('CLI verifies pending and completed turbo downloads without Content-Length', async ({
    page,
    request,
    baseURL,
}) => {
    test.skip(
        !process.env.BROWSER_TEST_CLI_BINARY,
        'Set BROWSER_TEST_CLI_BINARY to a locally built beam binary.',
    );
    const binary = resolve(process.env.BROWSER_TEST_CLI_BINARY!);
    const directory = await mkdtemp(join(tmpdir(), 'filebeam-cli-turbo-'));
    const payload = Buffer.from('Streamed turbo browser to CLI transfer\n'.repeat(2000));
    const env = { ...process.env, FILEBEAM_DISABLE_UPDATE_CHECK: '1' };
    let pendingChunks = 0;
    let receivedChunks = 0;
    let child: ChildProcess | undefined;
    let completion: Promise<unknown> | undefined;
    let created: { id: string; delete_token: string } | undefined;
    let releaseChunks!: () => void;
    let releaseCompletion!: () => void;
    const chunksGate = new Promise<void>((resolve) => {
        releaseChunks = resolve;
    });
    const completionGate = new Promise<void>((resolve) => {
        releaseCompletion = resolve;
    });
    const proxy = createServer((incoming, outgoing) => {
        const url = new URL(incoming.url!, baseURL!);
        const upstream = (url.protocol === 'https:' ? httpsRequest : httpRequest)(
            url,
            {
                method: incoming.method,
                headers: { ...incoming.headers, host: url.host },
            },
            (response) => {
                const headers = { ...response.headers };
                if (/\/items\/[^/]+\/chunks\/\d+$/.test(url.pathname)) {
                    if (response.statusCode === 202) pendingChunks++;
                    if (response.statusCode === 200 || response.statusCode === 206) {
                        delete headers['content-length'];
                        response.on('end', () => {
                            receivedChunks++;
                        });
                    }
                }
                outgoing.writeHead(response.statusCode!, headers);
                response.pipe(outgoing);
            },
        );
        upstream.on('error', (error) => outgoing.destroy(error));
        incoming.pipe(upstream);
    });
    try {
        await new Promise<void>((resolve) => proxy.listen(0, '127.0.0.1', resolve));
        const address = proxy.address();
        if (!address || typeof address === 'string') throw new Error('Missing proxy address');
        await page.route('**/api/v1/transfers/*/items/*/chunks/*', async (route) => {
            if (route.request().method() === 'PUT') await chunksGate;
            await route.continue();
        });
        await page.route('**/api/v1/transfers/*/complete', async (route) => {
            await completionGate;
            await route.continue();
        });
        await page.goto('/');
        await page.locator('#filebeam-picker').setInputFiles({
            name: 'turbo-cli.bin',
            mimeType: 'application/octet-stream',
            buffer: payload,
        });
        await page.getByLabel('Attach note', { exact: true }).check();
        await page
            .getByRole('textbox', { name: 'Secure note editor', exact: true })
            .fill('Turbo attachment 🦀\n');
        const creation = page.waitForResponse(
            (response) =>
                response.request().method() === 'POST' &&
                new URL(response.url()).pathname === '/api/v1/transfers',
        );
        await page.getByRole('button', { name: 'Turbo Transfer' }).click();
        created = (await (await creation).json()).data;
        await expect(page.locator('#share-link')).toBeVisible();
        const link = new URL(await page.locator('#share-link').inputValue());
        link.host = `127.0.0.1:${address.port}`;
        link.protocol = 'http:';
        const output = join(directory, 'pending');
        const download = run(
            binary,
            [
                '--home',
                join(directory, 'home'),
                '--plain',
                'down',
                link.href,
                '--output',
                output,
                '--note-output',
                join(directory, 'turbo-note.txt'),
            ],
            {
                env,
                timeout: 90_000,
            },
        );
        child = download.child;
        completion = download.then(
            () => undefined,
            (error: unknown) => error,
        );
        await expect.poll(() => pendingChunks, { timeout: 30_000 }).toBeGreaterThan(0);
        child.kill('SIGINT');
        expect(await completion).toBeDefined();
        const catalog = await run(
            binary,
            ['--home', join(directory, 'home'), '--plain', 'transfers'],
            { env, timeout: 30_000 },
        );
        const jobID = catalog.stdout.trim().split('\n')[0].split('\t')[0];
        expect(jobID).toMatch(/^[a-f0-9-]{36}$/);
        const resumed = run(
            binary,
            [
                '--home',
                join(directory, 'home'),
                '--plain',
                'resume',
                jobID,
                '--note-output',
                join(directory, 'turbo-note.txt'),
            ],
            { env, timeout: 90_000 },
        );
        child = resumed.child;
        completion = resumed.then(
            () => undefined,
            (error: unknown) => error,
        );
        releaseChunks();
        await expect.poll(() => receivedChunks, { timeout: 30_000 }).toBeGreaterThan(0);
        const metadata = await request.get(`/api/v1/transfers/${created!.id}`);
        expect((await metadata.json()).data.status).toBe('pending');
        await expect(readFile(join(output, 'turbo-cli.bin'))).rejects.toThrow();
        releaseCompletion();
        expect(await completion).toBeUndefined();
        expect(await readFile(join(output, 'turbo-cli.bin'))).toEqual(payload);
        expect(await readFile(join(directory, 'turbo-note.txt'), 'utf8')).toBe(
            'Turbo attachment 🦀\n',
        );
        await run(
            binary,
            [
                '--home',
                join(directory, 'home'),
                '--plain',
                'down',
                link.href,
                '--output',
                join(directory, 'completed'),
            ],
            {
                env,
                timeout: 60_000,
            },
        );
        expect(await readFile(join(directory, 'completed/turbo-cli.bin'))).toEqual(payload);
    } finally {
        releaseChunks();
        releaseCompletion();
        child?.kill('SIGKILL');
        await completion;
        await page.unrouteAll({ behavior: 'wait' });
        proxy.closeAllConnections();
        await new Promise<void>((resolve) => proxy.close(() => resolve()));
        if (created)
            await request.delete(`/api/v1/transfers/${created.id}`, {
                headers: { 'X-Filebeam-Delete-Token': created.delete_token },
            });
        await rm(directory, { recursive: true, force: true });
    }
});

test('built CLI and browser exchange real encrypted files through the Laravel API', async ({
    page,
    request,
    baseURL,
}) => {
    test.skip(
        !process.env.BROWSER_TEST_CLI_BINARY,
        'Set BROWSER_TEST_CLI_BINARY to a compatible locally built beam binary.',
    );
    const binary = resolve(process.env.BROWSER_TEST_CLI_BINARY!);
    const directory = await mkdtemp(join(tmpdir(), 'filebeam-cli-interop-'));
    const home = join(directory, 'home');
    await mkdir(home);
    await writeFile(
        join(home, 'config.toml'),
        `schema_version = 1\n[server]\nurl = ${JSON.stringify(baseURL!)}\n[updates]\nauto_update = false\nchannel = "stable"\n`,
    );
    const env = { ...process.env };
    const payload = Buffer.from('Real browser to CLI interoperability\n'.repeat(500));
    let created: { id: string; delete_token: string } | undefined;
    try {
        await page.addInitScript(() =>
            Object.defineProperty(window, 'showSaveFilePicker', {
                value: undefined,
                configurable: true,
            }),
        );
        await page.goto('/');
        await page
            .locator('#filebeam-picker')
            .setInputFiles({ name: 'from-browser.txt', mimeType: 'text/plain', buffer: payload });
        await page.getByLabel('Attach note', { exact: true }).check();
        await page
            .getByRole('textbox', { name: 'Secure note editor', exact: true })
            .fill('Browser attachment 🦀\n');
        const creation = page.waitForResponse(
            (response) =>
                response.request().method() === 'POST' &&
                new URL(response.url()).pathname === '/api/v1/transfers',
        );
        await page.getByRole('button', { name: 'Send encrypted' }).click();
        created = (await (await creation).json()).data;
        await expect(page.locator('#share-link')).toBeVisible();
        const link = await page.locator('#share-link').inputValue();
        await run(
            binary,
            [
                '--home',
                home,
                '--plain',
                'down',
                link,
                '--output',
                join(directory, 'download'),
                '--note-output',
                join(directory, 'note.txt'),
            ],
            {
                env,
                timeout: 60000,
            },
        );
        expect(await readFile(join(directory, 'download/from-browser.txt'))).toEqual(payload);
        expect(await readFile(join(directory, 'note.txt'), 'utf8')).toBe('Browser attachment 🦀\n');

        const source = join(directory, 'from-cli.txt');
        await writeFile(source, payload);
        const note = join(directory, 'from-cli.md');
        await writeFile(note, '# CLI attachment 🦀\n');
        const result = await run(
            binary,
            [
                '--home',
                home,
                '--plain',
                'up',
                source,
                '--note-file',
                note,
                '--note-title',
                'CLI title',
                '--note-language',
                'markdown',
            ],
            {
                env,
                timeout: 60000,
            },
        );
        const cliLink = result.stdout.trim();
        expect(cliLink.startsWith(baseURL!)).toBe(true);
        await page.goto(cliLink);
        await expect(page.getByTestId('attached-note').getByRole('heading')).toHaveText(
            'CLI title',
        );
        await expect(page.getByTestId('attached-note')).toContainText('CLI attachment');
        const download = page.waitForEvent('download');
        await page.getByRole('button', { name: 'Download files', exact: true }).click();
        expect(await readFile((await (await download).path())!)).toEqual(payload);
    } finally {
        if (created)
            await request.delete(`/api/v1/transfers/${created.id}`, {
                headers: { 'X-Filebeam-Delete-Token': created.delete_token },
            });
        await rm(directory, { recursive: true, force: true });
    }
});
