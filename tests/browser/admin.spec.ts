import { expect, test } from '@playwright/test';

function relativeLuminance(color: string): number {
    const channels = color
        .match(/\d+(?:\.\d+)?/g)
        ?.slice(0, 3)
        .map(Number);

    if (!channels || channels.length !== 3) {
        throw new Error(`Expected an RGB color, received ${color}.`);
    }

    return channels
        .map((channel) => {
            const value = channel / 255;

            return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
        })
        .reduce(
            (luminance, channel, index) => luminance + channel * [0.2126, 0.7152, 0.0722][index],
            0,
        );
}

function contrastRatio(foreground: string, background: string): number {
    const [lighter, darker] = [relativeLuminance(foreground), relativeLuminance(background)].sort(
        (a, b) => b - a,
    );

    return (lighter + 0.05) / (darker + 0.05);
}

test('admin login defaults to dark and respects saved theme choices', async ({ page }) => {
    await page.goto('/admin/login');
    await expect(page.locator('html')).toHaveClass(/dark/);
    await expect(page.locator('.fi-logo-dark')).toBeVisible();

    const heading = page.getByRole('heading', { name: 'Sign in' });
    const [foreground, background] = await Promise.all([
        heading.evaluate((element) => getComputedStyle(element).color),
        page
            .locator('.fi-simple-main')
            .evaluate((element) => getComputedStyle(element).backgroundColor),
    ]);
    expect(contrastRatio(foreground, background)).toBeGreaterThanOrEqual(4.5);

    await page.evaluate(() => localStorage.setItem('theme', 'light'));
    await page.reload();
    await expect(page.locator('html')).not.toHaveClass(/dark/);
    await expect(page.locator('.fi-logo-light')).toBeVisible();

    await page.evaluate(() => localStorage.setItem('theme', 'dark'));
    await page.reload();
    await expect(page.locator('html')).toHaveClass(/dark/);
});

test('scheduler alert stays within the admin layout at desktop and mobile widths', async ({
    page,
}) => {
    test.skip(
        process.env.BROWSER_TEST_UNHEALTHY_SCHEDULER !== 'true',
        'Requires the isolated unhealthy scheduler fixture.',
    );

    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto('/admin/login');
    await page
        .getByLabel('Email or username')
        .fill(process.env.ADMIN_EMAIL ?? 'admin@filebeam.test');
    await page.locator('input[type="password"]').fill(process.env.ADMIN_PASSWORD ?? 'password');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Work overview' })).toBeVisible();

    const alert = page.getByRole('alert');
    await expect(alert).toBeVisible();
    await expect(
        alert.getByRole('heading', { name: 'Scheduler heartbeat is unhealthy.' }),
    ).toBeVisible();

    for (const width of [1440, 390]) {
        await page.setViewportSize({ width, height: 900 });
        const details = alert.locator('details');

        if (!(await details.evaluate((element) => element.open))) {
            await alert.getByText('Enable the scheduler with cron', { exact: true }).click();
        }

        await expect(alert.getByText('php artisan schedule:run', { exact: false })).toBeVisible();

        const box = await alert.boundingBox();

        expect(box).not.toBeNull();
        expect(box!.x).toBeGreaterThanOrEqual(0);
        expect(box!.x + box!.width).toBeLessThanOrEqual(width);
        expect(
            await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
        ).toBe(true);
    }

    const dashboardHeading = page.getByRole('heading', { name: 'Work overview' });
    const [foreground, background] = await Promise.all([
        dashboardHeading.evaluate((element) => getComputedStyle(element).color),
        page.locator('.fi-body').evaluate((element) => getComputedStyle(element).backgroundColor),
    ]);
    expect(contrastRatio(foreground, background)).toBeGreaterThanOrEqual(4.5);
});

for (const width of [1440, 390]) {
    test(`admin login and public reporting work at ${width}px`, async ({ page }) => {
        const errors: string[] = [];
        page.on('pageerror', (error) => errors.push(error.message));
        await page.setViewportSize({ width, height: 900 });
        await page.goto('/admin');
        await expect(page).toHaveURL(/\/admin\/login$/);
        await expect(page.getByRole('heading', { name: 'Sign in' })).toBeVisible();
        await page.getByLabel('Email or username').fill('not-an-admin@example.test');
        await page.locator('input[type="password"]').fill('invalid-password');
        await page.getByRole('button', { name: 'Sign in', exact: true }).click();
        await expect(page.getByText('These credentials do not match our records.')).toBeVisible();

        const transferId = '01K4Y8N9P0Q1R2S3T4V5W6X7Y8';
        await page.goto(`/reports/create?transfer_id=${transferId}`);
        await expect(page.getByRole('heading', { name: 'Report a transfer' })).toBeVisible();
        await expect(page.getByLabel('Transfer identifier')).toHaveValue(transferId);
        await page.getByLabel('Category').selectOption('other');
        await page.getByLabel('Description').fill('Browser smoke test for a nonexistent transfer.');
        await page.getByRole('button', { name: 'Submit report' }).click();
        await expect(page.getByRole('heading', { name: 'Thank you' })).toBeVisible();

        expect(
            await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
        ).toBe(true);
        expect(errors).toEqual([]);
    });
}
