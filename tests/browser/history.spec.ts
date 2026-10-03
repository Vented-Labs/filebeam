import { expect, test } from '@playwright/test';

for (const width of [1280, 390]) {
    test(`account history manages opaque outgoing transfers at ${width}px`, async ({
        page,
        context,
    }) => {
        await page.setViewportSize({ width, height: 900 });
        const suffix = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
        const request = context.request;
        const registered = await request.post('/api/native/v1/register', {
            headers: { 'Sec-Fetch-Site': 'same-origin', Accept: 'application/json' },
            data: {
                username: `hist_${suffix}`,
                email: `history-${suffix}@example.test`,
                password: 'Browser8!History',
                password_confirmation: 'Browser8!History',
            },
        });
        expect(registered.status()).toBe(201);
        const info = (await (await request.get('/api/v1/info')).json()).data;
        const reserved = await request.post('/api/v1/transfers', {
            headers: { 'Sec-Fetch-Site': 'same-origin' },
            data: {
                kind: 'files',
                driver: 'http',
                protocol_version: 1,
                chunk_bytes: info.chunk_bytes,
                retention_hours: 1,
                items: [{ ciphertext_bytes: 32, chunk_count: 1 }],
            },
        });
        expect(reserved.status()).toBe(201);
        const transfer = (await reserved.json()).data;
        try {
            const upload = await request.put(
                `/api/v1/transfers/${transfer.id}/items/${transfer.items[0].id}/chunks/0`,
                {
                    headers: {
                        'Content-Type': 'application/octet-stream',
                        'X-Filebeam-Upload-Token': transfer.upload_token,
                    },
                    data: Buffer.alloc(32, 42),
                },
            );
            expect(upload.ok()).toBeTruthy();
            const complete = await request.post(`/api/v1/transfers/${transfer.id}/complete`, {
                headers: { 'X-Filebeam-Upload-Token': transfer.upload_token },
                data: { encrypted_manifest: 'opaque-history-browser-fixture' },
            });
            expect(complete.ok()).toBeTruthy();
            await page.goto('/account/history');
            await expect(page.getByRole('heading', { name: 'Transfer history' })).toBeVisible();
            const row = page.locator('.history-entry').filter({ hasText: transfer.id });
            await expect(row).toContainText('Available');
            await expect(row).not.toContainText('opaque-history-browser-fixture');
            await row.getByRole('button', { name: 'Extend', exact: true }).click();
            await row.getByLabel('Total retention (hours)').fill('2');
            await row.getByRole('button', { name: 'Save retention' }).click();
            await expect(row.getByRole('button', { name: 'Extend', exact: true })).toBeVisible();
            const metadata = await request.get('/api/native/v1/history');
            expect(
                (await metadata.json()).data.find(
                    (entry: { id: string }) => entry.id === transfer.id,
                ).retention_hours,
            ).toBe(2);
            await page.getByLabel('Type', { exact: true }).selectOption('note');
            await expect(
                page.getByText('No outgoing transfers match these filters.'),
            ).toBeVisible();
            await expect(page.getByLabel('Type', { exact: true })).toHaveCount(1);
            await page.getByLabel('Type', { exact: true }).selectOption('files');
            await expect(row).toBeVisible();
            await row.getByRole('button', { name: 'Delete', exact: true }).click();
            await row.getByRole('button', { name: 'Confirm deletion' }).click();
            await expect(row).toContainText(/Awaiting cleanup|Deleted/);
            await expect(row.getByRole('button', { name: 'Extend', exact: true })).toHaveCount(0);
            expect(
                await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
            ).toBe(true);
        } finally {
            await request.delete(`/api/v1/transfers/${transfer.id}`, {
                headers: { 'X-Filebeam-Delete-Token': transfer.delete_token },
            });
        }
    });
}
