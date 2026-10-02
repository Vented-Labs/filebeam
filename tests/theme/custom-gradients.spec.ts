import { expect, test } from '@playwright/test';
import { visual } from './visual';

test('custom instance gradients retain raw seeds, readable artwork and opaque editors', async ({
    page,
    browser,
    baseURL,
}) => {
    test.setTimeout(180_000);
    await page.goto('/admin/login');
    await page.getByLabel('Email or username').fill('admin@filebeam.test');
    await page.locator('input[type="password"]').fill('password');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Work overview' })).toBeVisible();
    const context = await browser.newContext({
        baseURL,
        reducedMotion: 'reduce',
        viewport: { width: 1440, height: 1080 },
    });
    const visitor = await context.newPage();
    try {
        for (const seed of ['#000000', '#ffffff', '#808080', '#ffff00', '#271339']) {
            await page.goto('/admin/instance-settings');
            await page.getByLabel('Primary color', { exact: true }).fill(seed);
            await page.getByRole('button', { name: 'Save settings', exact: true }).click();
            await expect(page.locator('#filebeam-theme')).toHaveAttribute('data-primary', seed);
            for (const mode of ['light', 'dark'] as const) {
                await visitor.emulateMedia({ colorScheme: mode });
                await visitor.goto('/');
                await expect(visitor.locator('#filebeam-theme')).toHaveAttribute(
                    'data-primary',
                    seed,
                );
                await visitor.evaluate(async () => {
                    await document.fonts.ready;
                    await Promise.all([...document.images].map((image) => image.decode()));
                });
                await expect(visitor.locator('.fb-shell')).toHaveCSS(
                    'background-image',
                    /radial-gradient/,
                );
                expect(
                    await visitor
                        .locator('header .fb-brand img')
                        .evaluate((image: HTMLImageElement) => image.naturalWidth),
                ).toBeGreaterThan(0);
                await visual(visitor).toHaveScreenshot(
                    `custom-${seed.slice(1)}-${mode}-files.png`,
                    { mask: [visitor.locator('.fb-footer__version')] },
                );
                await visitor.getByRole('tab', { name: 'Notes', exact: true }).click();
                await expect(visitor.locator('.cm-editor')).toBeVisible();
                expect(
                    await visitor.locator('.cm-editor').evaluate((node) => {
                        const hex = getComputedStyle(node)
                            .getPropertyValue('--fb-editor-bg')
                            .trim();
                        const rgb = `rgb(${[1, 3, 5].map((offset) => Number.parseInt(hex.slice(offset, offset + 2), 16)).join(', ')})`;
                        return getComputedStyle(node).backgroundColor === rgb;
                    }),
                ).toBe(true);
                await expect(visitor.locator('.cm-editor')).toHaveCSS('background-image', 'none');
                await visual(visitor.locator('.note-composer__toolbar')).toHaveScreenshot(
                    `custom-${seed.slice(1)}-${mode}-toolbar.png`,
                );
            }
        }
    } finally {
        await context.close();
        await page.goto('/admin/instance-settings');
        await page.getByLabel('Primary color', { exact: true }).fill('');
        await page.getByRole('button', { name: 'Save settings', exact: true }).click();
        await expect(page.locator('#filebeam-theme')).toHaveAttribute('data-primary', '#8b35ff');
    }
});
