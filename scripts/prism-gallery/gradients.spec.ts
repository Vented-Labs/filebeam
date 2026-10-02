import { expect, test } from '@playwright/test';
import { visual } from '../../tests/theme/visual';
import { paintedContrast } from '../../tests/theme/paint';
import { profiles, cssRgb } from '../../tests/theme/palette';

for (const preset of ['purple', 'blue', 'teal', 'green', 'amber', 'orange', 'rose'] as const) {
    for (const mode of ['light', 'dark'] as const) {
        test(`${preset} ${mode} restores visible gradient layers and composed contrast`, async ({
            page,
        }) => {
            test.setTimeout(90_000);
            await page.emulateMedia({ reducedMotion: 'reduce' });
            await page.goto('/?gradients');
            await page.evaluate(
                ({ preset, mode }) => {
                    window.filebeamAppearance!.set(mode);
                    window.filebeamAppearance!.setPreset(preset);
                },
                { preset, mode },
            );
            await expect(page.locator('.cm-editor')).toBeVisible();
            await page.evaluate(async () => {
                await document.fonts.ready;
                await Promise.all([...document.images].map((image) => image.decode()));
            });
            const layers = await page.evaluate(() => {
                const style = (selector: string, pseudo?: string) =>
                    getComputedStyle(document.querySelector(selector)!, pseudo);
                return {
                    ambient: style('.fb-shell').backgroundImage,
                    footprint: style('.fb-shell').backgroundSize,
                    pond: style('.file-pond', '::before').backgroundImage,
                    opacity: style('.file-pond', '::before').opacity,
                    pointer: style('.file-pond', '::before').pointerEvents,
                    toolbar: style('.note-composer__toolbar').backgroundImage,
                    emblem: style('.share-ready__emblem').backgroundImage,
                    og: style('.og-card').backgroundImage,
                    card: style('.file-pond__card').backgroundImage,
                    front: style('.file-pond__card--main').backgroundImage,
                    editor: style('.cm-editor').backgroundColor,
                };
            });
            expect(layers.ambient.match(/radial-gradient/g)).toHaveLength(2);
            expect(layers.footprint).toMatch(/^100% 832px(?:, 100% 832px)?$/);
            for (const image of [
                layers.ambient,
                layers.pond,
                layers.toolbar,
                layers.emblem,
                layers.og,
            ]) {
                expect(image).toContain('radial-gradient');
                expect(image).toMatch(/rgba?\([^)]+(?:0\.[1-9]|0\.0[1-9]|\d, \d)/);
            }
            expect(layers.opacity).toBe('0.65');
            expect(layers.pointer).toBe('none');
            expect(layers.card).toContain('linear-gradient(145deg');
            expect(layers.front).toContain('linear-gradient(140deg');
            expect(layers.editor).toBe(cssRgb(profiles[preset][mode]['--fb-editor-bg']));
            for (const [selector, rule] of [
                ['[data-testid="gradient-ambient"]', '.fb-shell'],
                ['.file-pond', '.file-pond::before'],
                ['.note-composer__toolbar', '.note-composer__toolbar'],
                ['.share-ready__emblem', '.share-ready__emblem'],
                ['.og-card', '.og-card'],
            ]) {
                const target = page.locator(selector);
                const painted = await target.screenshot({ animations: 'disabled' });
                const flat = await page.addStyleTag({
                    content: `${rule} { background-image: none !important; }`,
                });
                try {
                    expect(
                        painted.equals(await target.screenshot({ animations: 'disabled' })),
                        `${preset}/${mode} visible tint at ${selector}`,
                    ).toBe(false);
                } finally {
                    await flat.evaluate((node) => node.parentNode?.removeChild(node));
                }
            }
            for (const selector of [
                '[data-testid="gradient-ambient"] p',
                '.file-pond__empty > p',
                '.fb-note-title',
                '.share-ready__emblem',
                '.og-description',
                '.og-copy h1 span:last-child',
            ]) {
                await paintedContrast(page, page.locator(selector));
            }
            for (const width of [1440, 390]) {
                await page.setViewportSize({ width, height: 1000 });
                await page.evaluate(() => (document.activeElement as HTMLElement)?.blur());
                await page.mouse.move(0, 0);
                await visual(page.getByTestId('gradient-ambient')).toHaveScreenshot(
                    `${preset}-${mode}-${width}-ambient.png`,
                );
                const pond = page.getByTestId('file-pond');
                await expect
                    .poll(() => pond.evaluate((node) => getComputedStyle(node, '::before').opacity))
                    .toBe('0.65');
                await visual(pond).toHaveScreenshot(`${preset}-${mode}-${width}-pond-idle.png`);
                await pond.hover();
                expect(
                    await pond.evaluate((node) => getComputedStyle(node, '::before').opacity),
                ).toBe('1');
                await visual(pond).toHaveScreenshot(`${preset}-${mode}-${width}-pond-hover.png`);
                await page.mouse.move(0, 0);
                await page.getByRole('button', { name: 'Choose files', exact: true }).focus();
                expect(
                    await pond.evaluate((node) => getComputedStyle(node, '::before').opacity),
                ).toBe('1');
                await visual(pond).toHaveScreenshot(`${preset}-${mode}-${width}-pond-focus.png`);
                await visual(page.locator('.note-composer__toolbar')).toHaveScreenshot(
                    `${preset}-${mode}-${width}-toolbar.png`,
                );
                await visual(page.locator('.share-ready__emblem')).toHaveScreenshot(
                    `${preset}-${mode}-${width}-emblem.png`,
                );
                await visual(page.locator('.gradient-fixture__social')).toHaveScreenshot(
                    `${preset}-${mode}-${width}-og.png`,
                );
            }
        });
    }
}

test('the compact dropzone retains its glow exception', async ({ page }) => {
    await page.goto('/?gradients&compact');
    const pond = page.locator('.file-pond--compact');
    await expect(pond).toBeVisible();
    expect(await pond.evaluate((node) => getComputedStyle(node, '::before').display)).toBe('none');
});
