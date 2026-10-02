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
    await page.getByRole('button', { name: 'About', exact: true }).click();
    await page.getByLabel('Appearance', { exact: true }).selectOption('light');
    await expect(page.locator('html')).toHaveAttribute('data-fb-theme', 'light');
    await page.getByRole('button', { name: 'Close dialog' }).click();
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

async function verifyImage(page: Page, selector: string, attribute: string): Promise<Buffer> {
    const url = await page.locator(selector).getAttribute(attribute);
    expect(url).toContain('/_theme/');
    const response = await page.request.get(url!);
    expect(response.ok()).toBe(true);
    expect(response.headers()['content-type']).toBe('image/png');
    return response.body();
}
