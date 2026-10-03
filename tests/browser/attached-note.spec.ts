import { expect, test } from '@playwright/test';
import { readFile } from 'node:fs/promises';

const text = '# PRIVATE_ATTACHED_NOTE\nWhitespace  🦀\t\n';

for (const { turbo, password } of [
    { turbo: false, password: '' },
    { turbo: true, password: '' },
    { turbo: false, password: 'private-attachment-password' },
]) {
    test(`encrypted file attachment is readable before downloading (${password ? 'password' : turbo ? 'Turbo' : 'HTTP'})`, async ({
        page,
        browser,
        request,
    }) => {
        const bodies: string[] = [];
        page.on('request', (sent) => {
            if (sent.url().includes('/api/v1/transfers')) bodies.push(sent.postData() ?? '');
        });
        await page.goto('/');
        await page.locator('#filebeam-picker').setInputFiles({
            name: 'attachment-file.txt',
            mimeType: 'text/plain',
            buffer: Buffer.from('File bytes'),
        });
        await page.getByLabel('Attach note', { exact: true }).check();
        await page.locator('#attached-note-title').fill('PRIVATE_NOTE_TITLE');
        await page.getByRole('textbox', { name: 'Secure note editor', exact: true }).fill(text);
        await page
            .getByRole('combobox', { name: 'Note language' })
            .filter({ visible: true })
            .click();
        await page.getByRole('option', { name: 'Markdown', exact: true }).click();
        if (password) {
            await page.getByTestId('prism-password-trigger').click();
            await page
                .getByTestId('prism-password-popover')
                .locator('#transfer-password')
                .fill(password);
            await page
                .getByTestId('prism-password-popover')
                .getByRole('button', { name: 'Done' })
                .click();
        }
        let release!: () => void;
        const gate = new Promise<void>((resolve) => {
            release = resolve;
        });
        if (turbo)
            await page.route('**/api/v1/transfers/*/items/*/chunks/*', async (route) => {
                await gate;
                await route.continue();
            });
        const creation = page.waitForResponse(
            (response) =>
                response.request().method() === 'POST' &&
                new URL(response.url()).pathname === '/api/v1/transfers',
        );
        await page
            .getByRole('button', { name: turbo ? 'Turbo Transfer' : 'Send encrypted', exact: true })
            .click();
        const created = (await (await creation).json()).data;
        const context = await browser.newContext();
        try {
            await expect(page.locator('#share-link')).toBeVisible();
            const recipient = await context.newPage();
            let chunkRequests = 0;
            recipient.on('request', (sent) => {
                if (/\/chunks\/\d+$/.test(new URL(sent.url()).pathname)) chunkRequests++;
            });
            await recipient.goto(await page.locator('#share-link').inputValue());
            if (password) {
                await expect(recipient.getByTestId('attached-note')).toHaveCount(0);
                await recipient.locator('#transfer-password').fill('incorrect-password');
                await recipient.getByRole('button', { name: 'Unlock', exact: true }).click();
                await expect(recipient.getByRole('alert')).toBeVisible();
                await expect(recipient.getByTestId('attached-note')).toHaveCount(0);
                await recipient.locator('#transfer-password').fill(password);
                await recipient.getByRole('button', { name: 'Unlock', exact: true }).click();
            }
            await expect(recipient.getByTestId('attached-note').getByRole('heading')).toHaveText(
                'PRIVATE_NOTE_TITLE',
            );
            await expect(recipient.getByTestId('attached-note')).toContainText(
                'PRIVATE_ATTACHED_NOTE',
            );
            expect(chunkRequests).toBe(0);
            if (turbo)
                expect(
                    (await (await request.get(`/api/v1/transfers/${created.id}`)).json()).data
                        .status,
                ).toBe('pending');
            const download = recipient.waitForEvent('download');
            await recipient.getByRole('button', { name: 'Save note', exact: true }).click();
            expect(await readFile((await (await download).path())!, 'utf8')).toBe(text);
            expect(bodies.join('\n')).not.toContain('PRIVATE_ATTACHED_NOTE');
            expect(bodies.join('\n')).not.toContain('PRIVATE_NOTE_TITLE');
            if (password) expect(bodies.join('\n')).not.toContain(password);
        } finally {
            release();
            await page.unrouteAll({ behavior: 'wait' });
            await context.close();
            await request.delete(`/api/v1/transfers/${created.id}`, {
                headers: { 'X-Filebeam-Delete-Token': created.delete_token },
            });
        }
    });
}

test('invalid attached draft is preserved and cannot start a transfer', async ({ page }) => {
    await page.goto('/');
    await page
        .locator('#filebeam-picker')
        .setInputFiles({ name: 'file.txt', mimeType: 'text/plain', buffer: Buffer.from('file') });
    await page.getByLabel('Attach note', { exact: true }).check();
    await expect(page.getByRole('button', { name: 'Send encrypted', exact: true })).toBeDisabled();
    await page
        .getByRole('textbox', { name: 'Secure note editor', exact: true })
        .fill('🦀'.repeat(16385));
    await expect(page.getByRole('alert')).toContainText('64 KiB');
    await expect(page.getByRole('button', { name: 'Send encrypted', exact: true })).toBeDisabled();
    await page.getByLabel('Attach note', { exact: true }).uncheck();
    await expect(page.getByRole('button', { name: 'Send encrypted', exact: true })).toBeEnabled();
    await page.getByLabel('Attach note', { exact: true }).check();
    await expect(page.getByRole('button', { name: 'Send encrypted', exact: true })).toBeDisabled();
});
