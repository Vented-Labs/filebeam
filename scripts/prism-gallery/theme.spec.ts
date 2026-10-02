import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import type { ThemePreset } from '../../ui/src/lib/appearance-types';
import { visual } from '../../tests/theme/visual';

const presets: ThemePreset[] = ['purple', 'blue', 'teal', 'green', 'amber', 'orange', 'rose'];
const expected = JSON.parse(
    execFileSync(
        'php',
        [fileURLToPath(new URL('../themes/palette.php', import.meta.url)), '--contracts'],
        { encoding: 'utf8' },
    ),
).profiles;

for (const preset of presets)
    for (const mode of ['light', 'dark'] as const) {
        test(`${preset} ${mode} production components match the palette and visual contract`, async ({
            page,
        }) => {
            await page.emulateMedia({ reducedMotion: 'reduce' });
            await page.goto('/?theme');
            await page.evaluate(
                ({ preset, mode }) => {
                    window.filebeamAppearance!.set(mode);
                    window.filebeamAppearance!.setPreset(preset);
                },
                { preset, mode },
            );
            await page.evaluate(async () => {
                await document.fonts.ready;
                await Promise.all([...document.images].map((image) => image.decode()));
            });
            await expect(page.locator('.cm-content')).toBeVisible();
            const actual = await page.evaluate(
                (names) =>
                    Object.fromEntries(
                        names.map((name: string) => [
                            name,
                            getComputedStyle(document.documentElement)
                                .getPropertyValue(name)
                                .trim(),
                        ]),
                    ),
                Object.keys(expected[preset][mode]),
            );
            for (const [name, value] of Object.entries(expected[preset][mode]))
                expect(actual[name].toLowerCase(), name).toBe(
                    (value as string)
                        .replace('var(--fb-font-ui)', expected[preset][mode]['--fb-font-ui'])
                        .toLowerCase(),
                );
            for (const amount of [0.4, 50, 100])
                await expect(
                    page.getByRole('progressbar', { name: `Progress ${amount}`, exact: true }),
                ).toHaveAttribute('aria-valuenow', String(amount));
            await visual(page.getByTestId('theme-controls')).toHaveScreenshot(
                `${preset}-${mode}-controls.png`,
                { animations: 'disabled' },
            );
            await visual(page.getByTestId('theme-transfer')).toHaveScreenshot(
                `${preset}-${mode}-transfer.png`,
                { animations: 'disabled' },
            );
            if (['purple', 'teal', 'amber', 'rose'].includes(preset)) {
                const button = page.getByTestId('primary');
                await button.hover();
                await visual(page.getByTestId('theme-controls')).toHaveScreenshot(
                    `${preset}-${mode}-hover.png`,
                );
                await page.mouse.down();
                await visual(page.getByTestId('theme-controls')).toHaveScreenshot(
                    `${preset}-${mode}-active.png`,
                );
                await page.mouse.up();
                await page.keyboard.press('Tab');
                await page.keyboard.press('Shift+Tab');
                await expect(button).toHaveCSS('outline-width', '2px');
                await expect(button).toHaveCSS('outline-offset', '3px');
                await visual(page.getByTestId('theme-controls')).toHaveScreenshot(
                    `${preset}-${mode}-focus.png`,
                );
                await page.getByRole('radio', { name: 'WebRTC (live)' }).click();
                await visual(page.getByTestId('theme-transfer')).toHaveScreenshot(
                    `${preset}-${mode}-live.png`,
                );
                await visual(page.getByTestId('theme-share')).toHaveScreenshot(
                    `${preset}-${mode}-share.png`,
                );
            }
        });
    }

test('progress keeps numeric ARIA across invalid, fractional and decreasing values', async ({
    page,
}) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.goto('/?theme');
    for (const [value, expected] of [
        ['0', 0],
        ['0.4', 0.4],
        ['1', 1],
        ['50', 50],
        ['100', 100],
        ['-10', 0],
        ['NaN', 0],
        ['50', 50],
        ['1', 1],
    ] as const) {
        await page
            .getByTestId('progress-controls')
            .getByRole('button', { name: value, exact: true })
            .click();
        await expect(
            page.getByRole('progressbar', { name: 'Adjustable progress' }),
        ).toHaveAttribute('aria-valuenow', String(expected));
    }
    await expect(
        page.getByRole('progressbar', { name: 'Progress 0.4', exact: true }),
    ).toHaveAttribute('aria-valuetext', 'Progress 0.4: <1%');
});

test('forced colors and 200 percent zoom retain solid control boundaries and content', async ({
    page,
}) => {
    await page.emulateMedia({ reducedMotion: 'reduce', forcedColors: 'active' });
    await page.goto('/?theme');
    await page.getByTestId('primary').focus();
    await expect(page.getByTestId('primary')).toHaveCSS('outline-style', 'solid');
    await expect(page.getByTestId('primary')).toHaveCSS('forced-color-adjust', 'auto');
    await page.emulateMedia({ forcedColors: 'none' });
    await page.setViewportSize({ width: 768, height: 1024 });
    await page.evaluate(() => {
        document.documentElement.style.zoom = '2';
    });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
    );
    await expect(page.getByRole('button', { name: 'Appearance', exact: true })).toBeVisible();
});

test('tooltips, selected menus, notifications and password controls remain legible', async ({
    page,
}) => {
    test.setTimeout(120_000);
    await page.emulateMedia({ reducedMotion: 'reduce' });
    for (const preset of presets)
        for (const mode of ['light', 'dark'] as const) {
            await page.goto('/?theme');
            await page.evaluate(
                ({ preset, mode }) => {
                    window.filebeamAppearance!.set(mode);
                    window.filebeamAppearance!.setPreset(preset);
                },
                { preset, mode },
            );
            await page.getByRole('button', { name: 'Tooltip trigger' }).hover();
            await expect(page.locator('.fb-tooltip')).toBeVisible();
            await visual(page.locator('.fb-tooltip')).toHaveScreenshot(
                `${preset}-${mode}-tooltip.png`,
            );
            await page.getByRole('button', { name: 'Show toast' }).click();
            await expect(
                page.getByText('Your encrypted transfer is ready.', { exact: true }),
            ).toBeVisible();
            await visual(page.locator('.fb-toast')).toHaveScreenshot(`${preset}-${mode}-toast.png`);
            await page.getByRole('button', { name: 'Dismiss notification' }).click();
            await page.getByRole('combobox', { name: 'Note language' }).click();
            await visual(page.getByRole('listbox')).toHaveScreenshot(
                `${preset}-${mode}-dropdown.png`,
            );
            await page.keyboard.press('Escape');
            if (['purple', 'teal', 'amber', 'rose'].includes(preset)) {
                await page.locator('.password-trigger').click();
                await visual(page.locator('.password-popover')).toHaveScreenshot(
                    `${preset}-${mode}-password.png`,
                );
                await page.keyboard.press('Escape');
            }
        }
});

test('touch selection uses the mounted editor and retains selected source after a mode change', async ({
    browser,
    baseURL,
}) => {
    const context = await browser.newContext({
        baseURL,
        hasTouch: true,
        viewport: { width: 390, height: 844 },
    });
    try {
        const page = await context.newPage();
        await page.goto('/?theme');
        const editor = page.locator('.cm-content');
        await editor.tap();
        await editor.press('Control+Home');
        await editor.press('Shift+End');
        const before = await page.evaluate(() => getSelection()?.toString());
        expect(before).toContain('private draft');
        await page.evaluate(() => window.filebeamAppearance!.set('light'));
        expect(await page.evaluate(() => getSelection()?.toString())).toBe(before);
        await expect(page.locator('.cm-selectionBackground').first()).toBeVisible();
    } finally {
        await context.close();
    }
});
