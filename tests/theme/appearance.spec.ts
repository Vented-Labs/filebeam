import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { expect, test, type Page } from '@playwright/test';

test('system appearance, explicit preference, and live OS changes preserve the editor', async ({
    page,
}) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await page.goto('/');
    await expect(page.locator('html')).toHaveAttribute('data-fb-theme', 'light');
    await page.getByRole('tab', { name: 'Notes' }).click();
    await page.locator('.cm-content').fill('const privateDraft = "preserve me";');
    const editor = await page.locator('.cm-editor').elementHandle();
    await page.emulateMedia({ colorScheme: 'dark' });
    await expect(page.locator('html')).toHaveAttribute('data-fb-theme', 'dark');
    await page.getByRole('button', { name: 'Appearance', exact: true }).click();
    await page
        .getByRole('radiogroup', { name: 'Theme', exact: true })
        .getByRole('radio', { name: 'Light', exact: true })
        .click();
    await expect(page.locator('html')).toHaveAttribute('data-fb-theme', 'light');
    await page.getByRole('button', { name: 'Done', exact: true }).click();
    expect(await editor!.evaluate((element) => element.isConnected)).toBe(true);
    await expect(page.locator('.cm-content')).toContainText('preserve me');
    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('data-fb-theme', 'light');
});

test('system appearance works when browser storage is unavailable', async ({ page }) => {
    await page.addInitScript(() => {
        Object.defineProperty(window, 'localStorage', {
            get: () => {
                throw new Error('Storage unavailable');
            },
        });
    });
    await page.emulateMedia({ colorScheme: 'light' });
    await page.goto('/');
    await expect(page.locator('html')).toHaveAttribute('data-fb-theme', 'light');
    await expect(page.getByRole('heading', { name: 'Drop your files here' })).toBeVisible();
});

test('the visual color picker works for inherited, cleared, and invalid text values', async ({
    page,
}) => {
    await page.goto('/admin/login');
    await page.getByLabel('Email or username').fill('admin@filebeam.test');
    await page.locator('input[type="password"]').fill('password');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Work overview' })).toBeVisible();
    await page.goto('/admin/instance-settings');
    const input = page.getByLabel('Primary color', { exact: true });
    const picker = page.locator('hex-color-picker');
    try {
        for (const value of [null, '#not-a-color', '']) {
            if (value !== null) {
                await input.fill(value);
                await input.press('Tab');
            }
            if (!(await picker.isVisible()))
                await page.locator('.fi-fo-color-picker-preview').click();
            await expect(picker).toBeVisible();
            await picker
                .getByRole('slider', { name: 'Hue', exact: true })
                .click({ position: { x: 100, y: 10 } });
            await expect(input).toHaveValue(/^#[0-9a-f]{6}$/i);
            await picker.click({ position: { x: 120, y: 40 } });
            await expect(input).toHaveValue(/^#[0-9a-f]{6}$/i);
            const selected = (await input.inputValue()).toLowerCase();
            await input.press('Escape');
            await page.getByRole('button', { name: 'Save settings', exact: true }).click();
            await expect(page.locator('#filebeam-theme')).toHaveAttribute('data-primary', selected);
        }
    } finally {
        await input.fill('');
        await input.press('Tab');
        await page.getByRole('button', { name: 'Save settings', exact: true }).click();
        await expect(page.locator('#filebeam-theme')).toHaveAttribute('data-primary', '#8b35ff');
        await expect(input).toHaveValue('');
    }
});

test('standalone error pages retain readable light branding without JavaScript', async ({
    browser,
    baseURL,
}) => {
    const context = await browser.newContext({
        baseURL,
        javaScriptEnabled: false,
        colorScheme: 'light',
    });
    try {
        const page = await context.newPage();
        const response = await page.goto('/theme-missing-page');
        expect(response?.status()).toBe(404);
        await expect(page.locator('.brand-logo--light')).toBeVisible();
        await expect(page.locator('.brand-logo--dark')).toBeHidden();
        await expect(page.getByRole('heading', { name: 'Page not found' })).toBeVisible();
    } finally {
        await context.close();
    }
});

test('instance colors change every output using the same compiled frontend', async ({
    page,
    context,
}) => {
    const manifest = resolve('backend/public/build/manifest.json');
    const before = await readFile(manifest, 'utf8');
    await page.goto('/admin/login');
    await page.getByLabel('Email or username').fill('admin@filebeam.test');
    await page.locator('input[type="password"]').fill('password');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Work overview' })).toBeVisible();
    const visitor = await context.newPage();
    const backgrounds: string[] = [];
    const images: string[] = [];

    async function saveColor(color: string) {
        await page.goto('/admin/instance-settings');
        await page.getByLabel('Primary color', { exact: true }).fill(color);
        await page.getByRole('button', { name: 'Save settings', exact: true }).click();
        await expect(page.locator('#filebeam-theme')).toHaveAttribute(
            'data-primary',
            color || '#8b35ff',
        );
    }
    try {
        for (const primary of ['#008877', '#cc5500']) {
            await saveColor(primary);
            for (const mode of ['dark', 'light'] as const) {
                await visitor.emulateMedia({ colorScheme: mode });
                await visitor.goto('/');
                await expect(visitor.locator('html')).toHaveAttribute('data-fb-theme', mode);
                await expect(visitor.locator('#filebeam-theme')).toHaveAttribute(
                    'data-primary',
                    primary,
                );
                backgrounds.push(
                    await visitor
                        .locator('.fb-shell')
                        .evaluate((element) => getComputedStyle(element).backgroundColor),
                );
                const logo = visitor.locator('header .fb-brand img');
                await expect(logo).toHaveJSProperty('complete', true);
                expect(
                    await logo.evaluate((image: HTMLImageElement) => image.naturalWidth),
                ).toBeGreaterThan(0);
                await verifyImage(
                    visitor,
                    'link[rel="icon"][type="image/png"][sizes="32x32"]',
                    'href',
                );
            }
            const social = await verifyImage(visitor, 'meta[property="og:image"]', 'content');
            expect(social.readUInt32BE(16)).toBe(1200);
            expect(social.readUInt32BE(20)).toBe(630);
            images.push(createHash('sha256').update(social).digest('hex'));
        }
        expect(new Set(backgrounds).size).toBe(4);
        expect(new Set(images).size).toBe(2);
        expect(await readFile(manifest, 'utf8')).toBe(before);
    } finally {
        await saveColor('');
        await visitor.close();
    }
});

test('the empty desktop homepage fits one viewport in both appearances', async ({ page }) => {
    for (const colorScheme of ['light', 'dark'] as const) {
        await page.emulateMedia({ colorScheme, reducedMotion: 'reduce' });
        for (const [width, height] of [
            [901, 768],
            [1024, 768],
            [1280, 720],
            [1366, 768],
            [1440, 900],
            [1920, 1080],
        ]) {
            await page.setViewportSize({ width, height });
            await page.goto('/');
            await page.evaluate(() => document.fonts.ready);
            await expect
                .poll(() =>
                    page.evaluate(() => document.documentElement.scrollHeight <= innerHeight),
                )
                .toBe(true);
            await expect(
                page.getByRole('button', { name: 'Choose files', exact: true }),
            ).toBeInViewport({ ratio: 1 });
            await expect(page.getByRole('button', { name: 'Paste', exact: true })).toHaveCount(0);
            await expect(
                page.getByRole('button', { name: 'Send encrypted', exact: true }),
            ).toBeInViewport({ ratio: 1 });
            await expect(
                page.getByRole('button', { name: 'Appearance', exact: true }),
            ).toBeInViewport({
                ratio: 1,
            });
        }
    }
});

test('the paintbrush stays between logo and version and its popup supports keyboard selection', async ({
    page,
}) => {
    for (const width of [320, 390, 768, 901, 1024, 1440]) {
        await page.setViewportSize({ width, height: 900 });
        await page.goto('/');
        const trigger = page.getByRole('button', { name: 'Appearance', exact: true });
        const logo = (await page.locator('.fb-footer__identity .fb-brand').boundingBox())!;
        const toggle = (await trigger.boundingBox())!;
        const version = (await page.locator('.fb-footer__version').boundingBox())!;
        expect(toggle.x).toBeGreaterThanOrEqual(logo.x + logo.width);
        expect(version.x).toBeGreaterThanOrEqual(toggle.x + toggle.width);
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
            true,
        );
        const before = await page.evaluate(() => document.documentElement.scrollHeight);
        await trigger.click();
        const popup = page.getByRole('dialog', { name: 'Appearance', exact: true });
        await expect(popup).toBeVisible();
        await expect(popup).toHaveCSS('border-top-width', '1px');
        await expect(popup).toHaveCSS('padding-top', '17px');
        await expect(popup).not.toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
        const box = (await popup.boundingBox())!;
        expect(box.x).toBeGreaterThanOrEqual(0);
        expect(box.x + box.width).toBeLessThanOrEqual(width);
        expect(await page.evaluate(() => document.documentElement.scrollHeight)).toBe(before);
        const group = popup.getByRole('radiogroup', { name: 'Theme', exact: true });
        await group.getByRole('radio', { name: 'System', exact: true }).click();
        await page.keyboard.press('ArrowRight');
        await expect(group.getByRole('radio', { name: 'Light', exact: true })).toBeChecked();
        await expect(page.locator('html')).toHaveAttribute('data-fb-theme', 'light');
        await page.keyboard.press('ArrowRight');
        await expect(group.getByRole('radio', { name: 'Dark', exact: true })).toBeChecked();
        await expect(page.locator('html')).toHaveAttribute('data-fb-theme', 'dark');
        await page.keyboard.press('Escape');
        await expect(popup).toBeHidden();
        await expect(trigger).toBeFocused();
    }
});

async function verifyImage(page: Page, selector: string, attribute: string): Promise<Buffer> {
    const url = await page.locator(selector).getAttribute(attribute);
    expect(url).toContain('/_theme/');
    const response = await page.request.get(url!);
    expect(response.ok()).toBe(true);
    expect(response.headers()['content-type']).toBe('image/png');
    return response.body();
}
