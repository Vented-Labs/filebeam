import { expect, test } from '@playwright/test';
import { visual } from './visual';

test('Filament keeps neutral gray and readable primary roles for cool and warm instance colors', async ({
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
    const grays = new Set<string>();
    try {
        for (const [preset, seed] of [
            ['purple', '#8b35ff'],
            ['amber', '#cf9b35'],
            ['orange', '#da7b40'],
        ]) {
            await page.goto('/admin/instance-settings');
            await page.getByLabel('Primary color', { exact: true }).fill(seed);
            await page.getByRole('button', { name: 'Save settings', exact: true }).click();
            await expect(page.locator('#filebeam-theme')).toHaveAttribute('data-primary', seed);
            for (const mode of ['light', 'dark']) {
                await page.evaluate((mode) => localStorage.setItem('theme', mode), mode);
                await page.reload();
                await expect(page.locator('html')).toHaveAttribute('data-fb-theme', mode);
                await page.evaluate(() => document.fonts.ready);
                grays.add(
                    await page.evaluate(() =>
                        getComputedStyle(document.documentElement)
                            .getPropertyValue('--gray-500')
                            .trim(),
                    ),
                );
                const save = page.getByRole('button', { name: 'Save settings', exact: true });
                const contrast = await save.evaluate((button) => {
                    const canvas = document.createElement('canvas');
                    const ctx = canvas.getContext('2d')!;
                    function rgb(color: string) {
                        ctx.clearRect(0, 0, 1, 1);
                        ctx.fillStyle = color;
                        ctx.fillRect(0, 0, 1, 1);
                        return [...ctx.getImageData(0, 0, 1, 1).data];
                    }
                    function luminance(channels: number[]) {
                        const linear = channels
                            .slice(0, 3)
                            .map((n) => n / 255)
                            .map((n) => (n <= 0.04045 ? n / 12.92 : ((n + 0.055) / 1.055) ** 2.4));
                        return linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
                    }
                    const style = getComputedStyle(button);
                    const fg = luminance(rgb(style.color)),
                        bg = luminance(rgb(style.backgroundColor));
                    return {
                        fg: style.color,
                        bg: style.backgroundColor,
                        ratio: (Math.max(fg, bg) + 0.05) / (Math.min(fg, bg) + 0.05),
                    };
                });
                expect(
                    contrast.ratio,
                    `${preset} ${mode}: ${JSON.stringify(contrast)}`,
                ).toBeGreaterThanOrEqual(4.5);
                await visual(save).toHaveScreenshot(`admin-${preset}-${mode}-action.png`);
                const smtp = page
                    .locator('.fi-section')
                    .filter({ has: page.getByText('SMTP mail', { exact: true }) });
                await visual(smtp).toHaveScreenshot(`admin-${preset}-${mode}-form.png`);
                const guest = await browser.newContext({
                    baseURL,
                    viewport: { width: 1280, height: 900 },
                    reducedMotion: 'reduce',
                });
                try {
                    await guest.addInitScript((mode) => localStorage.setItem('theme', mode), mode);
                    const login = await guest.newPage();
                    await login.goto('/admin/login');
                    await login.evaluate(() => document.fonts.ready);
                    await visual(login.locator('.fi-simple-main')).toHaveScreenshot(
                        `admin-${preset}-${mode}-login.png`,
                    );
                } finally {
                    await guest.close();
                }
            }
        }
        expect(grays.size).toBe(1);
    } finally {
        await page.goto('/admin/instance-settings');
        await page.getByLabel('Primary color', { exact: true }).fill('');
        await page.getByRole('button', { name: 'Save settings', exact: true }).click();
        await expect(page.locator('#filebeam-theme')).toHaveAttribute('data-primary', '#8b35ff');
    }
});

test('standalone error visuals retain semantic status with and without JavaScript', async ({
    browser,
    baseURL,
}) => {
    for (const javaScriptEnabled of [true, false])
        for (const colorScheme of ['light', 'dark'] as const) {
            const context = await browser.newContext({
                baseURL,
                javaScriptEnabled,
                colorScheme,
                reducedMotion: 'reduce',
                viewport: { width: 1280, height: 900 },
            });
            try {
                const page = await context.newPage();
                expect((await page.goto('/theme-missing-page'))?.status()).toBe(404);
                await visual(page).toHaveScreenshot(
                    `error-${colorScheme}-${javaScriptEnabled ? 'js' : 'no-js'}.png`,
                );
            } finally {
                await context.close();
            }
        }
});
