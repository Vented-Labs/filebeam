import { expect, test } from '@playwright/test';

for (const kind of ['files', 'note']) {
    test(`new ${kind} transfers require eight-character passwords when protected`, async ({
        page,
        request,
    }) => {
        await page.goto('/');
        if (kind === 'note') {
            await page.getByRole('tab', { name: 'Notes' }).click();
            await page
                .locator('.cm-content[contenteditable="true"]')
                .fill('Password minimum boundary test');
        } else {
            await page.locator('#filebeam-picker').setInputFiles({
                name: 'password-policy.txt',
                mimeType: 'text/plain',
                buffer: Buffer.from('Password minimum boundary test'),
            });
        }

        await page.getByTestId('prism-password-trigger').click();
        const popover = page.getByTestId('prism-password-popover');
        const password = popover.locator('#transfer-password');
        await password.fill('\u{1f512}'.repeat(7));
        await popover.getByRole('button', { name: 'Done' }).click();
        await expect(popover.getByText('Use at least 8 characters.')).toBeVisible();
        await expect(page.getByTestId('prism-password-trigger')).toHaveAttribute(
            'aria-invalid',
            'true',
        );
        await expect(page.getByRole('button', { name: 'Encrypt and share' })).toBeDisabled();

        await password.fill('\u{1f512}'.repeat(8));
        await popover.getByRole('button', { name: 'Done' }).click();
        const creation = page.waitForResponse(
            (response) =>
                response.request().method() === 'POST' &&
                response.url().endsWith('/api/v1/transfers'),
        );
        await page.getByRole('button', { name: 'Encrypt and share' }).click();
        const response = await creation;
        expect(response.ok()).toBe(true);
        const { data } = await response.json();
        try {
            await expect(
                page.getByRole('heading', { name: 'Your encrypted link is ready' }),
            ).toBeVisible();
        } finally {
            await request.delete(`/api/v1/transfers/${data.id}`, {
                headers: { 'X-Filebeam-Delete-Token': data.delete_token },
            });
        }
    });
}
