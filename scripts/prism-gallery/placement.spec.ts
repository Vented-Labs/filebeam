import { expect, test } from '@playwright/test';

for (const width of [320, 390, 768, 900, 901, 1023, 1024, 1440]) {
    test(`production install placement at ${width}px`, async ({ page }, testInfo) => {
        await page.setViewportSize({ width, height: 900 });
        await page.goto('/?placement');
        const app = page.locator('header [data-app-install-entry]');
        await expect(app).toHaveCount(1);
        await expect(app).toHaveAccessibleName(
            width > 900 ? 'Install Desktop App' : 'Install Mobile App',
        );
        await expect(app).toBeVisible();
        const logo = (await page.locator('.fb-footer__identity .fb-brand').boundingBox())!;
        const version = (await page.locator('.fb-footer__version').boundingBox())!;
        expect(Math.abs(logo.y + logo.height / 2 - (version.y + version.height / 2))).toBeLessThan(
            1,
        );
        expect(version.x).toBeGreaterThanOrEqual(logo.x + logo.width);
        await expect(app).toHaveAttribute('aria-disabled', 'true');
        await app.dispatchEvent('click');
        await expect(page.getByRole('dialog')).toHaveCount(0);
        const footer = page.locator('.cli-footer-launcher');
        if (width > 900) {
            await expect(footer).toBeVisible();
            const box = (await footer.boundingBox())!;
            const productFooter = (await page.locator('.fb-footer').boundingBox())!;
            expect(
                Math.abs(box.x + box.width / 2 - (productFooter.x + productFooter.width / 2)),
            ).toBeLessThan(1);
            expect(box.height).toBe(38);
        } else {
            await expect(footer).toBeHidden();
            await page.getByRole('button', { name: 'Open navigation' }).click();
            await expect(page.getByRole('menuitem', { name: /Install CLI/ })).toHaveCount(0);
            await page.keyboard.press('Escape');
            await expect(page.getByRole('menu')).toBeHidden();
        }
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
            true,
        );
        await page.screenshot({
            path: testInfo.outputPath(`placement-${width}.png`),
            fullPage: true,
        });
    });
}

for (const reducedMotion of ['reduce', 'no-preference'] as const) {
    test(`footer activation and focus restoration (${reducedMotion})`, async ({ page }) => {
        await page.emulateMedia({ reducedMotion });
        await page.setViewportSize({ width: 1440, height: 1000 });
        await page.goto('/?placement');
        const footer = page.locator('.cli-footer-launcher');
        const dialog = page.getByRole('dialog', { name: 'Install CLI', exact: true });
        const editor = page.locator('.cm-content[contenteditable="true"]');
        await editor.fill('Keep the draft through installation');
        for (const key of ['Enter', 'Space', 'click']) {
            await footer.focus();
            if (key === 'click') await footer.click();
            else await footer.press(key);
            await expect(dialog).toBeVisible();
            for (let index = 0; index < 12; index++) {
                await page.keyboard.press('Tab');
                expect(await dialog.evaluate((node) => node.contains(document.activeElement))).toBe(
                    true,
                );
            }
            await page.keyboard.press('Escape');
            await expect(dialog).toBeHidden();
            await expect(footer).toBeFocused();
            await expect(editor).toHaveText('Keep the draft through installation');
        }
        await footer.click();
        await dialog.getByRole('button', { name: 'Close CLI instructions' }).click();
        await expect(footer).toBeFocused();
        await footer.click();
        await page.locator('.fb-dialog__overlay').click({ position: { x: 5, y: 5 } });
        await expect(dialog).toBeHidden();
        await expect(footer).toBeFocused();
        await footer.click();
        await page.setViewportSize({ width: 390, height: 844 });
        await dialog.getByRole('button', { name: 'Done', exact: true }).click();
        await expect(page.locator('header [data-app-install-entry]')).toBeFocused();
        await expect(footer).toBeHidden();
        await page.getByRole('button', { name: 'Install CLI', exact: true }).click();
        await expect(dialog).toBeVisible();
        await page.keyboard.press('Escape');
        await expect(page.getByRole('button', { name: 'Install CLI', exact: true })).toBeFocused();
    });
}
