import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';

type CreatedTransfer = { id: string; deleteToken: string };

let created: CreatedTransfer[];
let recipients: BrowserContext[];

test.beforeEach(async () => {
    created = [];
    recipients = [];
});

test.afterEach(async ({ request }) => {
    await Promise.all(
        created.map(({ id, deleteToken }) =>
            request.delete(`/api/v1/transfers/${id}`, {
                headers: { 'X-Filebeam-Delete-Token': deleteToken },
            }),
        ),
    );
    await Promise.all(recipients.map((context) => context.close()));
});

function captureTransfer(page: Page): void {
    page.on('response', async (response) => {
        if (
            !response.ok() ||
            response.request().method() !== 'POST' ||
            !response.url().endsWith('/api/v1/transfers')
        )
            return;
        const payload = (await response.json()) as { data: { id: string; delete_token: string } };
        if (payload.data?.id && payload.data.delete_token)
            created.push({ id: payload.data.id, deleteToken: payload.data.delete_token });
    });
}

async function recipientPage(browser: Browser, initScript?: () => void): Promise<Page> {
    const context = await browser.newContext();
    if (initScript) await context.addInitScript(initScript);
    recipients.push(context);
    return context.newPage();
}

async function completeUpload(page: Page): Promise<void> {
    const ready = page.getByRole('heading', { name: 'Your encrypted link is ready' });
    await page.getByRole('button', { name: 'Encrypt and share' }).click();
    await expect(ready).toBeVisible({ timeout: 30_000 });
}

async function setSenderPassword(page: Page, password: string): Promise<void> {
    await page.getByTestId('prism-password-trigger').click();
    const popover = page.getByTestId('prism-password-popover');
    await popover.locator('#transfer-password').fill(password);
    await popover.getByRole('button', { name: 'Done' }).click();
}

async function uploadFile(
    page: Page,
    options: { includeKey?: boolean; password?: string; name?: string; content?: string } = {},
): Promise<{ link: string; key: string; content: string }> {
    captureTransfer(page);
    const marker = options.content ?? 'PLAYWRIGHT_PRIVATE_FILE_MARKER';
    await page.goto('/');
    await page.locator('#filebeam-picker').setInputFiles({
        name: options.name ?? 'private-marker.txt',
        mimeType: 'text/plain',
        buffer: Buffer.from(marker),
    });
    if (options.password) await setSenderPassword(page, options.password);
    if (options.includeKey === false) await page.getByText('Include key in link').click();
    const sentBodies: string[] = [];
    page.on('request', (request) => {
        if (request.url().includes('/api/v1/transfers')) sentBodies.push(request.postData() ?? '');
    });
    await completeUpload(page);
    await expect.poll(() => created.length).toBe(1);
    if (marker) expect(sentBodies.join('\n')).not.toContain(marker);
    expect(sentBodies.join('\n')).not.toContain(options.name ?? 'private-marker.txt');
    expect(sentBodies.join('\n')).not.toContain(options.password ?? 'unused-password');
    expect(sentBodies.join('\n')).not.toContain('#k=');
    return {
        link: await page.locator('#share-link').inputValue(),
        key: await page.locator('#generated-key').inputValue(),
        content: marker,
    };
}

test('downloads a file through a generated-key fragment without sending plaintext metadata', async ({
    browser,
    page,
}) => {
    const { link, content } = await uploadFile(page);
    const recipient = await recipientPage(browser, () => {
        Object.defineProperty(window, 'showSaveFilePicker', {
            value: undefined,
            configurable: true,
        });
    });
    await recipient.goto(link);
    await expect(recipient.getByText('private-marker.txt')).toBeVisible();
    const download = recipient.waitForEvent('download');
    await recipient.getByRole('button', { name: 'Download files' }).click();
    const saved = await download;
    const path = await saved.path();
    expect(path).not.toBeNull();
    expect(await readFile(path!)).toEqual(Buffer.from(content));
});

test('writes verified plaintext through a streamed save handle', async ({ browser, page }) => {
    const { link, content } = await uploadFile(page);
    const recipient = await recipientPage(browser, () => {
        const chunks: number[][] = [];
        Object.assign(window, {
            __filebeamWritable: { chunks, closed: false, aborted: false },
            showSaveFilePicker: async () => ({
                createWritable: async () => ({
                    write: async (data: Uint8Array) => chunks.push([...data]),
                    close: async () => {
                        (window as any).__filebeamWritable.closed = true;
                    },
                    abort: async () => {
                        (window as any).__filebeamWritable.aborted = true;
                    },
                }),
            }),
        });
    });
    await recipient.goto(link);
    await recipient.getByRole('button', { name: 'Download files' }).click();
    await expect
        .poll(() => recipient.evaluate(() => (window as any).__filebeamWritable.closed))
        .toBe(true);
    const output = await recipient.evaluate(() => (window as any).__filebeamWritable.chunks.flat());
    expect(Buffer.from(output)).toEqual(Buffer.from(content));
});

test('unlocks a no-fragment link with an explicitly supplied generated key', async ({
    browser,
    page,
}) => {
    const { link, key } = await uploadFile(page, { includeKey: false });
    expect(link).not.toContain('#');
    const recipient = await recipientPage(browser);
    await recipient.goto(link);
    await recipient.locator('#transfer-key').fill(key);
    await expect(recipient.locator('#transfer-password')).toHaveCount(0);
    await recipient.getByRole('button', { name: 'Unlock' }).click();
    await expect(recipient.getByText('private-marker.txt')).toBeVisible();
});

test('round-trips a password-protected note', async ({ browser, page }) => {
    captureTransfer(page);
    const marker = 'PLAYWRIGHT_PRIVATE_NOTE_MARKER';
    const password = 'dummy-browser-password';
    const sentBodies: string[] = [];
    page.on('request', (request) => {
        if (request.url().includes('/api/v1/transfers')) sentBodies.push(request.postData() ?? '');
    });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Notes' }).click();
    await page
        .locator('[data-testid="note-editor"] .cm-content[contenteditable="true"]')
        .fill(marker);
    await setSenderPassword(page, password);
    await page.getByText('Include key in link').click();
    await completeUpload(page);
    expect(sentBodies.join('\n')).not.toContain(marker);
    expect(sentBodies.join('\n')).not.toContain(password);
    await expect.poll(() => created.length).toBe(1);
    const link = await page.locator('#share-link').inputValue();
    const key = await page.locator('#generated-key').inputValue();
    const recipient = await recipientPage(browser);
    await recipient.goto(link);
    await recipient.locator('#transfer-key').fill(key);
    await expect(recipient.locator('#transfer-password')).toHaveCount(0);
    await recipient.getByRole('button', { name: 'Continue', exact: true }).click();
    await recipient.locator('#transfer-password').fill(password);
    await recipient.getByRole('button', { name: 'Unlock' }).click();
    await recipient.getByRole('button', { name: 'Decrypt note' }).click();
    await expect(recipient.getByText(marker)).toBeVisible();
});

test('uploads a zero-byte file and rejects a tampered ciphertext', async ({ browser, page }) => {
    const { link } = await uploadFile(page, { name: 'empty.txt', content: '' });
    const recipient = await recipientPage(browser, () => {
        Object.defineProperty(window, 'showSaveFilePicker', {
            value: undefined,
            configurable: true,
        });
    });
    await recipient.goto(link);
    await expect(recipient.getByText('empty.txt')).toBeVisible();
    let intercepted = false;
    await recipient.route(
        /\/api\/v1\/transfers\/[^/]+\/items\/[^/]+\/chunks\/\d+$/,
        async (route) => {
            intercepted = true;
            await route.fulfill({
                status: 200,
                contentType: 'application/octet-stream',
                body: Buffer.alloc(16, 7),
            });
        },
    );
    await recipient.getByRole('button', { name: 'Download files' }).click();
    await expect.poll(() => intercepted).toBe(true);
    await expect(recipient.getByText(/truncated|altered|failed/i)).toBeVisible();
});

test('does not overflow on a mobile viewport', async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 375, height: 812 });
    await page.goto('/');
    expect(
        await page.locator('body').evaluate((body) => body.scrollWidth <= window.innerWidth),
    ).toBe(true);
    await page.screenshot({ path: testInfo.outputPath('mobile.png'), fullPage: true });
});

test('does not overflow with the CI version label on a mobile viewport', async ({ page }) => {
    const version = 'dev-bf540ae3addd2eae42188a128b8d1c030f35245e';
    await page.route(/\/$/, async (route) => {
        const response = await route.fetch();
        const body = await response.text();
        const script = /(<script\b[^>]*\bdata-page[^>]*>)([\s\S]*?)(<\/script>)/i;
        const match = body.match(script);
        if (!match) throw new Error('Missing Inertia bootstrap');
        const data = JSON.parse(match[2]);
        data.props.branding.name = 'Filebeam acceptance';
        data.props.branding.version = version;
        await route.fulfill({
            response,
            body: body.replace(script, () => `${match[1]}${JSON.stringify(data)}${match[3]}`),
        });
    });
    await page.setViewportSize({ width: 375, height: 812 });
    await page.goto('/');
    await expect(page.locator('.fb-footer')).toContainText(`v${version}`);
    await expect(page.getByRole('link', { name: 'Filebeam acceptance home' })).toBeVisible();
    expect(
        await page.locator('body').evaluate((body) => body.scrollWidth <= window.innerWidth),
    ).toBe(true);
});

test('retention and burn-on-read protect a titled note until successful decryption', async ({
    page,
    browser,
    request,
}) => {
    captureTransfer(page);
    const title = 'PRIVATE_NOTE_TITLE';
    const content = 'PRIVATE_BURN_NOTE';
    const bodies: string[] = [];
    page.on('request', (outgoing) => {
        if (outgoing.url().includes('/api/v1/transfers')) bodies.push(outgoing.postData() ?? '');
    });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Notes' }).click();
    await page.getByLabel('Note title (optional)').fill(title);
    await page.locator('.cm-content[contenteditable="true"]').fill(content);
    await page.getByRole('combobox', { name: 'Retention period' }).click();
    await page.getByRole('option', { name: '7 days', exact: true }).click();
    await page.getByRole('switch', { name: 'Burn on read' }).click();
    await page.getByRole('switch', { name: 'Include key in link' }).click();
    await setSenderPassword(page, 'private-burn-password');
    await completeUpload(page);
    const link = await page.locator('#share-link').inputValue();
    const key = await page.locator('#generated-key').inputValue();
    const id = new URL(link).pathname.slice(1);
    expect(new URL(link).origin).toBe(new URL(page.url()).origin);
    const metadata = await (await request.get(`/api/v1/transfers/${id}`)).json();
    expect(metadata.data.retention_hours).toBe(168);
    expect(metadata.data.burn_on_read).toBe(true);
    expect(metadata.data.read_token).toBeUndefined();
    expect(bodies.join('\n')).not.toContain(title);
    expect(bodies.join('\n')).not.toContain(content);
    expect(bodies.join('\n')).not.toContain('private-burn-password');
    const recipient = await recipientPage(browser);
    await recipient.goto(link);
    await recipient.locator('#transfer-key').fill(key);
    await recipient.getByRole('button', { name: 'Continue', exact: true }).click();
    await recipient.locator('#transfer-password').fill('incorrect-password');
    await recipient.getByRole('button', { name: 'Unlock', exact: true }).click();
    await expect(recipient.getByRole('alert')).toBeVisible();
    expect((await request.get(`/api/v1/transfers/${id}`)).status()).toBe(200);
    await recipient.locator('#transfer-password').fill('private-burn-password');
    await recipient.getByRole('button', { name: 'Unlock', exact: true }).click();
    await expect(recipient.getByRole('heading', { name: title })).toBeVisible();
    expect((await request.get(`/api/v1/transfers/${id}`)).status()).toBe(200);
    await recipient.getByRole('button', { name: 'Decrypt note' }).click();
    await expect(recipient.getByText('Removed from the server.', { exact: false })).toBeVisible();
    await expect(recipient.getByText(content, { exact: true })).toBeVisible();
    expect((await request.get(`/api/v1/transfers/${id}`)).status()).toBe(404);
});

test('a password remains required even when the URL includes the encryption key', async ({
    page,
    browser,
}) => {
    const { link } = await uploadFile(page, { password: 'another-private-password' });
    expect(link).toContain('#k=v1.');
    const recipient = await recipientPage(browser);
    await recipient.goto(link);
    await expect(recipient.getByRole('heading', { name: 'Enter the password' })).toBeVisible();
    await expect(recipient.locator('#transfer-key')).toHaveCount(0);
    await expect(recipient.getByText('private-marker.txt')).toHaveCount(0);
    await recipient.getByRole('button', { name: 'Unlock', exact: true }).click();
    await expect(recipient.getByRole('alert')).toContainText('password');
    await recipient.locator('#transfer-password').fill('wrong-password');
    await recipient.getByRole('button', { name: 'Unlock', exact: true }).click();
    await expect(recipient.getByRole('alert')).toContainText('failed');
    await recipient.locator('#transfer-password').fill('another-private-password');
    await recipient.getByRole('button', { name: 'Unlock', exact: true }).click();
    await expect(recipient.getByText('private-marker.txt')).toBeVisible();
});

test('invalid key errors use plain language and allow a corrected key', async ({
    page,
    browser,
}) => {
    const { link, key } = await uploadFile(page, { includeKey: false });
    const recipient = await recipientPage(browser);
    await recipient.goto(`${link}#k=v1.not%25base64`);
    const message = 'Invalid decryption key - please enter the decryption key';
    await expect(recipient.getByRole('alert')).toHaveText(message);
    await expect(recipient.getByText(/WorkerGlobalScope|Failed to execute 'atob'/)).toHaveCount(0);
    await recipient.locator('#transfer-key').fill('not-a-key');
    await recipient.getByRole('button', { name: 'Unlock', exact: true }).click();
    await expect(recipient.getByRole('alert')).toHaveText(message);
    await recipient.locator('#transfer-key').fill(key);
    await recipient.getByRole('button', { name: 'Unlock', exact: true }).click();
    await expect(recipient.getByText('private-marker.txt')).toBeVisible();
});
