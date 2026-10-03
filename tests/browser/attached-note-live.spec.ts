import { expect, test } from '@playwright/test';
import { readFile } from 'node:fs/promises';

test('live file attachment unlocks without contacting a peer', async ({
    page,
    browser,
    request,
}) => {
    test.skip(process.env.FILEBEAM_WEBRTC_TESTS !== '1', 'Requires the isolated WebRTC backend.');
    await page.goto('/');
    await page.locator('#filebeam-picker').setInputFiles({
        name: 'live-file.txt',
        mimeType: 'text/plain',
        buffer: Buffer.from('Live file bytes'),
    });
    await page.getByLabel('Attach note', { exact: true }).check();
    await page
        .getByRole('textbox', { name: 'Secure note editor', exact: true })
        .fill('Live encrypted attachment 🦀\n');
    await page.getByRole('radio', { name: 'WebRTC (live)' }).click();
    const creation = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' &&
            new URL(response.url()).pathname === '/api/v1/transfers',
    );
    await page.getByRole('button', { name: 'Send encrypted', exact: true }).click();
    await page
        .getByRole('dialog', { name: 'WebRTC privacy' })
        .getByRole('button', { name: 'Accept and continue' })
        .click();
    const created = (await (await creation).json()).data;
    const context = await browser.newContext();
    try {
        await expect(page.locator('#share-link')).toBeVisible();
        await context.addInitScript(() => {
            Object.assign(window, { attachmentPeerCount: 0 });
            const Original = window.RTCPeerConnection;
            Object.defineProperty(window, 'RTCPeerConnection', {
                value: function (...args: ConstructorParameters<typeof RTCPeerConnection>) {
                    (window as unknown as { attachmentPeerCount: number }).attachmentPeerCount++;
                    return new Original(...args);
                },
            });
            Object.defineProperty(window, 'showSaveFilePicker', { value: undefined });
        });
        const recipient = await context.newPage();
        await recipient.goto(await page.locator('#share-link').inputValue());
        await expect(recipient.getByTestId('attached-note')).toContainText(
            'Live encrypted attachment',
        );
        expect(
            await recipient.evaluate(
                () => (window as unknown as { attachmentPeerCount: number }).attachmentPeerCount,
            ),
        ).toBe(0);
        const download = recipient.waitForEvent('download');
        await recipient.getByRole('button', { name: 'Download files', exact: true }).click();
        await recipient
            .getByRole('dialog', { name: 'WebRTC privacy' })
            .getByRole('button', { name: 'Accept and continue' })
            .click();
        expect(await readFile((await (await download).path())!, 'utf8')).toBe('Live file bytes');
    } finally {
        await context.close();
        await request.delete(`/api/v1/transfers/${created.id}`, {
            headers: { 'X-Filebeam-Delete-Token': created.delete_token },
        });
    }
});
