import { expect, test } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

test('transport and homepage feature icons render the approved custom artwork', async ({
    page,
}) => {
    await page.goto('/');
    const icons = [
        ['http-server', page.getByRole('radio', { name: 'HTTP (stored)', exact: true })],
        ['webrtc-p2p', page.getByRole('radio', { name: 'WebRTC (live)', exact: true })],
        [
            'end-to-end-encrypted',
            page.locator('.fb-trust-feature').filter({ hasText: 'End-to-end encrypted' }),
        ],
        [
            'fast-and-simple',
            page.locator('.fb-trust-feature').filter({ hasText: 'Fast and simple' }),
        ],
    ] as const;
    for (const [name, container] of icons) {
        const source = await readFile(resolve(`icons/custom/${name}.svg`), 'utf8');
        const svg = container.locator('svg.fb-icon');
        await expect(svg).toBeVisible();
        expect(
            await svg
                .locator('path')
                .evaluateAll((paths) => paths.map((path) => path.getAttribute('d'))),
        ).toEqual([...source.matchAll(/<path d="([^"]+)"/g)].map((match) => match[1]));
        await expect(svg.locator('g[stroke="currentColor"]')).toHaveAttribute(
            'stroke-width',
            '1.5',
        );
        await expect(svg.locator('g[opacity="0.4"]')).toHaveCount(1);
        await expect(svg).toHaveAttribute('aria-hidden', 'true');
    }
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    await expect(
        page.getByRole('radio', { name: 'HTTP (stored)', exact: true }).locator('svg.fb-icon'),
    ).toBeVisible();
    await expect(
        page.getByRole('radio', { name: 'WebRTC (live)', exact: true }).locator('svg.fb-icon'),
    ).toBeVisible();
});

test('icons retain fixed artwork attributes while inheriting visual classes', async ({ page }) => {
    await page.goto('/');

    const upload = page.locator('[data-testid="file-pond"] .file-pond__card--left svg.fb-icon');
    await expect(upload).toHaveAttribute('viewBox', '0 0 24 24');
    await expect(upload).toHaveAttribute('fill', 'none');
    await expect(upload).toHaveAttribute('focusable', 'false');
    await expect(upload).toHaveAttribute('aria-hidden', 'true');
    await expect(upload).toHaveAttribute('width', '21');
    await expect(upload).toHaveAttribute('height', '21');
    expect(
        await upload.evaluate((icon) => ({
            color: getComputedStyle(icon).color,
            shrink: getComputedStyle(icon).flexShrink,
        })),
    ).toEqual({ color: 'rgb(178, 168, 194)', shrink: '0' });

    const chooseIcon = page
        .getByRole('button', { name: 'Choose files' })
        .locator('svg.fb-icon')
        .first();
    await expect(chooseIcon).toHaveAttribute('width', '17');
    await expect(chooseIcon).toHaveAttribute('height', '17');
    expect(
        await chooseIcon.evaluate((icon) => ({
            width: getComputedStyle(icon).width,
            height: getComputedStyle(icon).height,
        })),
    ).toEqual({ width: '17px', height: '17px' });
});

test('transfer loader is centered and disables animation when reduced motion is requested', async ({
    page,
}) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    let releaseTransfer!: () => void;
    const transferReady = new Promise<void>((resolve) => {
        releaseTransfer = resolve;
    });
    await page.route('**/api/v1/transfers/*', async (route) => {
        await transferReady;
        await route.continue();
    });
    await page.goto('/01AAAAAAAAAAAAAAAAAAAAAAAA');

    const loader = page.locator('svg.fb-icon--loader').first();
    await expect(loader).toBeVisible();
    expect(await loader.evaluate((icon) => getComputedStyle(icon).animationName)).toBe('none');
    expect(
        await loader.evaluate((icon) => {
            const parent = icon.parentElement!.getBoundingClientRect();
            const bounds = icon.getBoundingClientRect();

            return Math.abs(bounds.left + bounds.width / 2 - (parent.left + parent.width / 2));
        }),
    ).toBeLessThan(1);
    releaseTransfer();
});
