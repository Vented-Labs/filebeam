import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { expect, test } from '@playwright/test';

for (const variant of ['home', 'receive', 'transfer']) {
    test(`${variant} matches the shipped PNG and scales to mobile`, async ({ page, request }) => {
        const errors: string[] = [];
        page.on('pageerror', (error) => errors.push(error.message));
        await page.goto(`/?variant=${variant}`);
        await page.evaluate(async () => {
            await document.fonts.ready;
            await Promise.all([...document.images].map((image) => image.decode()));
        });
        const card = page.locator('.og-card');
        await expect(page.locator('.og-description')).toHaveText(
            'Securely send files and notes to others',
        );
        if (variant === 'home') {
            const words = page.locator('h1 span');
            await expect(words).toHaveText(['Share files', 'privately']);
            await expect(words.nth(1)).toHaveCSS('color', 'rgb(231, 161, 255)');
            const first = await words.nth(0).boundingBox();
            const second = await words.nth(1).boundingBox();
            expect(second?.y).toBe(first?.y);
        }
        await expect(card).toHaveCSS('width', '1200px');
        await expect(card).toHaveCSS('height', '630px');
        const png = await card.screenshot({ animations: 'disabled' });
        const hash = createHash('sha256').update(png).digest('hex').slice(0, 16);
        const manifest = JSON.parse(
            await readFile(
                new URL('../../backend/public/build/og/manifest.json', import.meta.url),
                'utf8',
            ),
        );
        expect(manifest[variant]).toBe(`${variant}-${hash}.png`);
        const image = await request.get(`/build/og/${manifest[variant]}`);
        expect(image.ok()).toBe(true);
        expect(image.headers()['content-type']).toContain('image/png');
        expect(await image.body()).toEqual(png);

        await page.setViewportSize({ width: 390, height: 844 });
        const box = await card.boundingBox();
        expect(box?.width).toBeCloseTo(390, 0);
        expect(box?.height).toBeCloseTo(204.75, 0);
        expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(390);
        await expect(page.locator('h1')).toBeVisible();
        expect(errors).toEqual([]);
    });
}

test('unknown variants fail rather than silently render the wrong card', async ({ page }) => {
    const error = page.waitForEvent('pageerror');
    await page.goto('/?variant=private-transfer-name');
    expect((await error).message).toContain('Unknown social card variant');
    await expect(page.locator('.og-card')).toHaveCount(0);
});
