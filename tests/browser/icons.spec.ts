import { expect, test } from '@playwright/test';

test('icons retain fixed artwork attributes while inheriting visual classes', async ({ page }) => {
    await page.goto('/');

    const upload = page.locator('[data-testid="file-pond"] svg.pond-illustration');
    await expect(upload).toHaveAttribute('viewBox', '0 0 24 24');
    await expect(upload).toHaveAttribute('fill', 'none');
    await expect(upload).toHaveAttribute('focusable', 'false');
    await expect(upload).toHaveAttribute('aria-hidden', 'true');
    await expect(upload).toHaveAttribute('width', '96');
    await expect(upload).toHaveAttribute('height', '96');
    expect(
        await upload.evaluate((icon) => ({
            color: getComputedStyle(icon).color,
            shrink: getComputedStyle(icon).flexShrink,
        })),
    ).toEqual({ color: 'rgb(168, 85, 247)', shrink: '0' });

    const chooseIcon = page.getByRole('button', { name: 'Choose files' }).locator('svg.fb-icon');
    await expect(chooseIcon).toHaveAttribute('width', '22');
    await expect(chooseIcon).toHaveAttribute('height', '22');
    expect(
        await chooseIcon.evaluate((icon) => ({
            width: getComputedStyle(icon).width,
            height: getComputedStyle(icon).height,
        })),
    ).toEqual({ width: '22px', height: '22px' });
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
