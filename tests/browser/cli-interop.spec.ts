import { expect, test } from '@playwright/test';
import { execFile } from 'node:child_process';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { promisify } from 'node:util';

const run = promisify(execFile);

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
    await writeFile(join(home, 'config.toml'), 'check_updates = false\n');
    const env = { ...process.env, FILEBEAM_INSTANCE: baseURL!, FILEBEAM_HOME: home };
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
        const creation = page.waitForResponse(
            (response) =>
                response.request().method() === 'POST' &&
                new URL(response.url()).pathname === '/api/v1/transfers',
        );
        await page.getByRole('button', { name: 'Encrypt and share' }).click();
        created = (await (await creation).json()).data;
        await expect(page.locator('#share-link')).toBeVisible();
        const link = await page.locator('#share-link').inputValue();
        await run(binary, ['--plain', 'down', link, '--output', join(directory, 'download')], {
            env: { ...env, FILEBEAM_INSTANCE: 'https://filebeam.io' },
            timeout: 60000,
        });
        expect(await readFile(join(directory, 'download/from-browser.txt'))).toEqual(payload);

        const source = join(directory, 'from-cli.txt');
        await writeFile(source, payload);
        const result = await run(binary, ['--plain', 'up', source], { env, timeout: 60000 });
        const cliLink = result.stdout.trim();
        expect(cliLink.startsWith(baseURL!)).toBe(true);
        await page.goto(cliLink);
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
