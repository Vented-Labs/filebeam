import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';

type CreatedTransfer = { id: string; deleteToken: string };
type Custody = 'password' | 'self';

const accountPassword = 'Browser8!Account';
let transfers: CreatedTransfer[];
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

function uniqueAccount(): { username: string; email: string } {
    const suffix = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;
    return { username: `inbox_${suffix}`, email: `inbox-${suffix}@example.test` };
}

async function register(page: Page): Promise<{ username: string; email: string }> {
    const account = uniqueAccount();
    await page.goto('/register');
    await expect(page.getByRole('heading', { name: 'Create your account' })).toBeVisible();
    await page.getByLabel('Username', { exact: true }).fill(account.username);
    await page.getByLabel('Email', { exact: true }).fill(account.email);
    await page.getByLabel('Password', { exact: true }).fill(accountPassword);
    await page.getByLabel('Confirm password', { exact: true }).fill(accountPassword);
    const registration = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' &&
            new URL(response.url()).pathname === '/register',
    );
    await page.getByRole('button', { name: 'Create account', exact: true }).click();
    expect((await registration).status()).toBeLessThan(400);
    await expect(page).toHaveURL(/\/verify-email$/);
    await page.goto('/account');
    await expect(page.getByRole('heading', { name: 'Account' })).toBeVisible();
    return account;
}

async function activateInbox(page: Page, custody: Custody): Promise<string | undefined> {
    await page.getByRole('button', { name: 'Activate secure inbox' }).click();
    if (custody === 'self') {
        const selfCustody = page.getByRole('radio', { name: /Keep the key yourself/ });
        await page.locator('label').filter({ has: selfCustody }).click();
        await expect(selfCustody).toBeChecked();
    }
    await page.getByRole('button', { name: 'Generate account key' }).click();

    if (custody === 'password') {
        await page.getByLabel('Current password', { exact: true }).fill(accountPassword);
        await page.getByLabel('Current password', { exact: true }).press('Enter');
        return undefined;
    }

    const privateCopy = page.getByRole('button', { name: 'Copy private key' });
    await expect(privateCopy).toHaveCSS('white-space', 'nowrap');
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
    );
    const download = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Download private key' }).click();
    const exportedPath = await (await download).path();
    expect(exportedPath).not.toBeNull();
    const exported = await readFile(exportedPath!);
    const privateKey = exported.toString().trim();
    await page.getByLabel('Paste the exported private key to confirm').fill(privateKey);
    await page.getByRole('button', { name: 'Save account key' }).click();
    return privateKey;
}

async function sendToInbox(
    browser: Browser,
    username: string,
    marker: string,
): Promise<{ sender: Page; transfer: CreatedTransfer; filename: string; sentBodies: string[] }> {
    const senderContext = await browser.newContext();
    contexts.push(senderContext);
    const sender = await senderContext.newPage();
    const sentBodies: string[] = [];
    let transfer: CreatedTransfer | undefined;
    sender.on('request', (request) => {
        if (request.url().includes('/api/v1/transfers')) sentBodies.push(request.postData() ?? '');
    });
    sender.on('response', async (response) => {
        if (
            response.status() !== 201 ||
            response.request().method() !== 'POST' ||
            !response.url().endsWith('/api/v1/transfers')
        )
            return;
        const data = (await response.json()) as { data: { id: string; delete_token: string } };
        transfer = { id: data.data.id, deleteToken: data.data.delete_token };
        transfers.push(transfer);
    });
    const filename = `private-${username}.txt`;
    await sender.goto(`/u/${username}`);
    await sender.locator('#filebeam-picker').setInputFiles({
        name: filename,
        mimeType: 'text/plain',
        buffer: Buffer.from(marker),
    });
    await sender.getByRole('button', { name: 'Encrypt and send' }).click();
    await expect(sender.getByRole('heading', { name: 'Files sent' })).toBeVisible({
        timeout: 30_000,
    });
    await expect.poll(() => transfer).toBeDefined();
    return { sender, transfer: transfer!, filename, sentBodies };
}

for (const custody of ['password', 'self'] as const) {
    test(`receives, privately unlocks, and downloads a ${custody}-custody inbox delivery`, async ({
        browser,
        page,
    }) => {
        await page.addInitScript(() => {
            Object.defineProperty(window, 'showSaveFilePicker', {
                value: undefined,
                configurable: true,
            });
        });
        const { username } = await register(page);
        const keyWrites: string[] = [];
        page.on('request', (request) => {
            if (request.url().endsWith('/account/keys')) keyWrites.push(request.postData() ?? '');
        });

        if (custody === 'self') await page.setViewportSize({ width: 375, height: 812 });
        const privateKey = await activateInbox(page, custody);
        await expect(page.getByText('Secure inbox active', { exact: true })).toBeVisible();
        await page.reload();
        await expect(page.getByText('Secure inbox active', { exact: true })).toBeVisible();

        const profileCopy = page.getByRole('button', { name: 'Copy profile link' });
        await expect(profileCopy).toBeVisible();
        expect(await profileCopy.innerText()).toBe('');
        expect(await profileCopy.evaluate((button) => getComputedStyle(button).width)).toBe('40px');
        expect(keyWrites.join('\n')).toContain(
            custody === 'password' ? accountPassword : '"custody_mode":"self"',
        );

        const marker = `INBOX_PRIVATE_MARKER_${username}`;
        const { transfer, filename, sentBodies } = await sendToInbox(browser, username, marker);
        expect(sentBodies.join('\n')).not.toContain(marker);
        expect(sentBodies.join('\n')).not.toContain(filename);

        const outsiderContext = await browser.newContext();
        contexts.push(outsiderContext);
        const outsider = await outsiderContext.newPage();
        expect((await outsider.goto(`/api/v1/transfers/${transfer.id}`))?.status()).toBe(404);

        await page.goto('/account/inbox');
        await expect(page.getByRole('link', { name: '1 encrypted file' })).toBeVisible();
        await page.getByRole('link', { name: '1 encrypted file' }).click();
        await expect(page.getByRole('heading', { name: 'Unlock received files' })).toBeVisible();
        await expect(
            page.getByLabel(custody === 'password' ? 'Key password' : 'Private key export'),
        ).toBeVisible();

        const unlockBodies: string[] = [];
        const chunkRequests: string[] = [];
        page.on('request', (request) => {
            if (request.url().includes('/account/inbox/')) {
                unlockBodies.push(request.postData() ?? '');
                if (request.url().includes('/items/')) chunkRequests.push(request.url());
            }
        });
        const secret = custody === 'password' ? accountPassword : privateKey!;
        const secretInput = page.getByLabel(
            custody === 'password' ? 'Key password' : 'Private key export',
        );
        await secretInput.fill(`${secret}-wrong`);
        await page.getByRole('button', { name: 'Unlock files' }).click();
        await expect(page.getByRole('alert')).toContainText(/could not unlock/i);
        await secretInput.fill(secret);
        await page.getByRole('button', { name: 'Unlock files' }).click();
        await expect(page.getByText(filename, { exact: true })).toBeVisible();
        expect(unlockBodies.join('\n')).not.toContain(secret);

        const download = page.waitForEvent('download');
        await page.getByRole('button', { name: 'Download files' }).click();
        const savedPath = await (await download).path();
        expect(savedPath).not.toBeNull();
        expect(await readFile(savedPath!)).toEqual(Buffer.from(marker));
        expect(chunkRequests).toContainEqual(
            expect.stringMatching(
                new RegExp(`/account/inbox/${transfer.id}/items/[^/]+/chunks/0$`),
            ),
        );
    });
}
