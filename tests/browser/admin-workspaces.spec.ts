import { expect, test, type Page } from '@playwright/test';

let createdTransfer: { id: string; delete_token: string } | undefined;

async function configuredChunkBytes(page: Page): Promise<number> {
    await page.goto('/');

    const chunkBytes = await page.evaluate(() => {
        const source = document.querySelector<HTMLScriptElement>('script[data-page]')?.textContent;
        const page = source
            ? (JSON.parse(source) as { props?: { filebeam?: { chunk_bytes?: number } } })
            : undefined;

        return page?.props?.filebeam?.chunk_bytes;
    });

    if (!chunkBytes) throw new Error('The production page did not expose its chunk policy.');

    return chunkBytes;
}

test.afterEach(async ({ request }) => {
    if (createdTransfer) {
        await request.delete(`/api/v1/transfers/${createdTransfer.id}`, {
            headers: { 'X-Filebeam-Delete-Token': createdTransfer.delete_token },
        });
    }
});

test('staff investigate a report, add a private note, and navigate contextual records', async ({
    page,
    request,
}, testInfo) => {
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    const marker = `Workspace review ${Date.now()}`;
    const chunkBytes = await configuredChunkBytes(page);
    const created = await request.post('/api/v1/transfers', {
        data: {
            kind: 'files',
            protocol_version: 1,
            chunk_bytes: chunkBytes,
            items: [{ ciphertext_bytes: 17, chunk_count: 1 }],
        },
    });
    expect(created.ok()).toBe(true);
    const { data: transfer } = await created.json();
    createdTransfer = transfer;

    const chunk = await request.put(
        `/api/v1/transfers/${transfer.id}/items/${transfer.items[0].id}/chunks/0`,
        {
            headers: {
                'Content-Type': 'application/octet-stream',
                'X-Filebeam-Upload-Token': transfer.upload_token,
            },
            data: Buffer.alloc(17),
        },
    );
    expect(chunk.ok()).toBe(true);
    const completed = await request.post(`/api/v1/transfers/${transfer.id}/complete`, {
        headers: { 'X-Filebeam-Upload-Token': transfer.upload_token },
        data: { encrypted_manifest: 'WORKSPACE_TEST_OPAQUE_MANIFEST' },
    });
    expect(completed.ok()).toBe(true);

    await page.goto(`/reports/create?transfer_id=${transfer.id}`);
    await page.getByLabel('Category').selectOption('other');
    await page.getByLabel('Description').fill(marker);
    await page.getByRole('button', { name: 'Submit report' }).click();
    await expect(page.getByRole('heading', { name: 'Thank you' })).toBeVisible();

    await page.goto('/admin/login');
    await page
        .getByLabel('Email or username')
        .fill(process.env.ADMIN_EMAIL ?? 'admin@filebeam.test');
    await page.locator('input[type="password"]').fill(process.env.ADMIN_PASSWORD ?? 'password');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Work overview' })).toBeVisible();
    await expect(page.getByText('Oldest active reports', { exact: true })).toBeVisible();
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.screenshot({ path: testInfo.outputPath('work-overview.png'), fullPage: true });

    await page.getByRole('searchbox', { name: 'Global search' }).fill(transfer.id);
    await expect(page.getByRole('link', { name: transfer.id, exact: true })).toBeVisible();

    await page.goto('/admin/file-reports?tab=unassigned');
    await page.getByRole('searchbox', { name: 'Search', exact: true }).fill(transfer.id);
    const row = page.getByRole('row').filter({ hasText: marker });
    await row.getByRole('link', { name: 'Open case' }).click();
    await expect(page).toHaveURL(/\/admin\/file-reports\/[^?]+\?queue=unassigned$/);
    await page.getByRole('button', { name: 'Assign to me', exact: true }).click();
    await page.getByRole('button', { name: 'Start review', exact: true }).click();
    await page.getByRole('tab', { name: 'Staff notes' }).click();
    await expect(page.getByRole('button', { name: 'Add note', exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Add note', exact: true }).click();
    const noteDialog = page.getByRole('dialog');
    await noteDialog
        .locator('textarea')
        .fill('Investigated the report; no policy violation found.');
    await noteDialog.getByRole('button', { name: 'Save note', exact: true }).click();
    await expect(
        page.getByText('Investigated the report; no policy violation found.', { exact: true }),
    ).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath('report-workspace.png'), fullPage: true });

    await page.getByRole('button', { name: 'Decisions', exact: true }).click();
    await page.getByText('Resolve without takedown', { exact: true }).click();
    const resolveDialog = page.getByRole('dialog');
    await resolveDialog
        .locator('textarea')
        .fill('Browser workflow verified without removing the upload.');
    await resolveDialog.getByRole('button', { name: 'Resolve report', exact: true }).click();
    await expect(
        page.getByText('Browser workflow verified without removing the upload.', { exact: true }),
    ).toBeVisible();
    await page.getByRole('tab', { name: 'Case activity' }).click();
    await expect(page.getByText('Review decision recorded', { exact: true }).first()).toBeVisible();

    await page.setViewportSize({ width: 390, height: 844 });
    await page.reload();
    await expect(page.getByRole('heading', { name: /^Report / })).toBeVisible();
    await page.screenshot({
        path: testInfo.outputPath('report-mobile.png'),
        fullPage: true,
        animations: 'disabled',
    });
    expect(
        await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
    await page.getByRole('link', { name: 'Open upload', exact: true }).click();
    await expect(page.getByRole('heading', { name: /^Upload / })).toBeVisible();
    await expect(page.getByText('WORKSPACE_TEST_OPAQUE_MANIFEST')).toHaveCount(0);
    await page.getByRole('tab', { name: 'Items', exact: true }).scrollIntoViewIfNeeded();
    await page.getByRole('button', { name: 'Diagnostics', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Diagnostics', exact: true })).toBeVisible();
    await page
        .getByRole('dialog')
        .getByRole('button', { name: 'Close', exact: true })
        .last()
        .click();

    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.goto('/admin/plans');
    await page.getByRole('link', { name: 'Edit limits', exact: true }).first().click();
    await expect(page.getByRole('heading', { name: /Edit/ })).toBeVisible();
    await expect(page.getByText('Current setting', { exact: true }).first()).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath('plan-settings.png'), fullPage: true });
    expect(errors).toEqual([]);
});
