import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { ThemePreset } from '../../ui/src/lib/appearance-types';
import { visual } from './visual';

const contract = JSON.parse(
    readFileSync(resolve('backend/tests/Fixtures/theme-color-contract.json'), 'utf8'),
);
const presets: ThemePreset[] = ['purple', 'blue', 'teal', 'green', 'amber', 'orange', 'rose'];

for (const preset of presets)
    for (const mode of ['light', 'dark'] as const) {
        test(`${preset} ${mode} production first paint, editor and appearance visuals`, async ({
            page,
        }) => {
            await page.setViewportSize({ width: 1440, height: 1080 });
            await page.emulateMedia({ reducedMotion: 'reduce', colorScheme: mode });
            await page.addInitScript(
                ({ preset, mode }) =>
                    localStorage.setItem(
                        'filebeam.guestAppearance',
                        JSON.stringify({ preset, mode }),
                    ),
                { preset, mode },
            );
            await page.goto('/');
            await expect(page.locator('html')).toHaveAttribute('data-fb-preset', preset);
            await page.evaluate(async () => {
                await document.fonts.ready;
                await Promise.all([...document.images].map((image) => image.decode()));
            });
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
                Object.keys(contract.profiles[preset][mode]),
            );
            for (const [name, value] of Object.entries(contract.profiles[preset][mode]))
                expect(actual[name].toLowerCase(), name).toBe(
                    (value as string)
                        .replace(
                            'var(--fb-font-ui)',
                            contract.profiles[preset][mode]['--fb-font-ui'],
                        )
                        .toLowerCase(),
                );
            await visual(page).toHaveScreenshot(`${preset}-${mode}-files.png`, {
                mask: [page.locator('.fb-footer__version')],
                animations: 'disabled',
            });
            await page.getByRole('tab', { name: 'Notes', exact: true }).click();
            await page.getByRole('combobox', { name: 'Note language' }).click();
            await page.getByRole('option', { name: 'JavaScript', exact: true }).click();
            await page.locator('.cm-content').click();
            await page.keyboard.insertText(
                '// Private message\nconst parcel = { message: "hello", count: 42, ready: true };\nconsole.log(parcel.message);',
            );
            await expect(page.locator('.cm-content')).toContainText('console.log(parcel.message);');
            await expect(page.locator('.fb-syn-string').first()).toBeVisible();
            await page.locator('.cm-content').press('Control+Home');
            await page.keyboard.press('Shift+End');
            await visual(page.locator('.note-composer')).toHaveScreenshot(
                `${preset}-${mode}-notes.png`,
                { animations: 'disabled' },
            );
            await page.getByRole('button', { name: 'Appearance', exact: true }).click();
            await visual(
                page.getByRole('dialog', { name: 'Appearance', exact: true }),
            ).toHaveScreenshot(`${preset}-${mode}-appearance.png`, { animations: 'disabled' });
        });
    }

test('stored blue light preferences remain coherent with a delayed or failed preset stylesheet', async ({
    page,
}) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.addInitScript(() => {
        localStorage.setItem(
            'filebeam.guestAppearance',
            JSON.stringify({ mode: 'light', preset: 'blue' }),
        );
        const frames: string[] = [];
        Object.assign(window, { themeFrames: frames });
        const sample = () => {
            if (document.querySelector('.fb-shell'))
                frames.push(
                    getComputedStyle(document.documentElement).getPropertyValue('--fb-bg').trim(),
                );
            if (frames.length < 20) requestAnimationFrame(sample);
        };
        requestAnimationFrame(sample);
    });
    await page.route('**/_theme/palettes/*.css', async (route) => {
        await new Promise((resolve) => setTimeout(resolve, 1000));
        await route.continue();
    });
    await page.goto('/');
    await expect(page.locator('html')).toHaveAttribute('data-fb-theme', 'light');
    await expect
        .poll(() =>
            page.evaluate(() => (window as Window & { themeFrames: string[] }).themeFrames.length),
        )
        .toBe(20);
    expect(
        await page.evaluate(() => [
            ...new Set((window as Window & { themeFrames: string[] }).themeFrames),
        ]),
    ).toEqual(['#f6f7fa']);
    await page.unroute('**/_theme/palettes/*.css');
    await page.route('**/_theme/palettes/*.css', (route) => route.abort());
    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('data-fb-theme', 'light');
    await expect(page.locator('.fb-shell')).toHaveCSS('background-color', 'rgb(247, 247, 250)');
    await expect(page.locator('.fb-shell')).toHaveCSS('color', 'rgb(38, 34, 48)');
});

test('narrow layouts retain readable controls and unclipped appearance focus in both modes', async ({
    page,
}) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    for (const preset of ['purple', 'amber'] as const)
        for (const mode of ['light', 'dark'] as const) {
            for (const [width, height] of [
                [320, 812],
                [390, 844],
                [768, 1024],
            ]) {
                await page.setViewportSize({ width, height });
                await page.goto('/');
                await page.evaluate(
                    ({ preset, mode }) => {
                        window.filebeamAppearance!.set(mode);
                        window.filebeamAppearance!.setPreset(preset);
                    },
                    { preset, mode },
                );
                await page.evaluate(() => document.fonts.ready);
                expect(
                    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
                ).toBe(true);
                await page.getByRole('button', { name: 'Appearance', exact: true }).click();
                const popup = page.getByRole('dialog', { name: 'Appearance', exact: true });
                const box = (await popup.boundingBox())!;
                expect(box.x).toBeGreaterThanOrEqual(0);
                expect(box.x + box.width).toBeLessThanOrEqual(width);
                await visual(page).toHaveScreenshot(`${preset}-${mode}-${width}.png`, {
                    mask: [page.locator('.fb-footer__version')],
                });
            }
        }
});
