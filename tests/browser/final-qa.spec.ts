import { expect, test } from '@playwright/test';

test('radial surfaces and dropzone icon hover match the prototype and respect reduced motion', async ({
    page,
}) => {
    await page.goto('/');
    const pond = page.getByTestId('file-pond');
    await expect(page.locator('.fb-shell')).toHaveCSS('background-image', /radial-gradient/);
    expect(
        await pond.evaluate((node) => getComputedStyle(node, '::before').backgroundImage),
    ).toContain('radial-gradient');
    const cards = pond.locator('.file-pond__card');
    const initial = await cards.evaluateAll((nodes) =>
        nodes.map((node) => getComputedStyle(node).transform),
    );
    const hoverFrames = cards.first().evaluate(async (node) => {
        const frames: string[] = [];
        for (let index = 0; index < 35; index++) {
            await new Promise(requestAnimationFrame);
            frames.push(getComputedStyle(node).transform);
        }
        return frames;
    });
    await pond.hover();
    expect(new Set(await hoverFrames).size).toBeGreaterThan(5);
    await expect
        .poll(() =>
            cards.evaluateAll((nodes) => nodes.map((node) => getComputedStyle(node).transform)),
        )
        .not.toEqual(initial);
    await expect
        .poll(() => pond.evaluate((node) => getComputedStyle(node, '::before').opacity))
        .toBe('1');
    const exitFrames = cards.first().evaluate(async (node) => {
        const frames: string[] = [];
        for (let index = 0; index < 35; index++) {
            await new Promise(requestAnimationFrame);
            frames.push(getComputedStyle(node).transform);
        }
        return frames;
    });
    await page.mouse.move(0, 0);
    expect(new Set(await exitFrames).size).toBeGreaterThan(5);
    await pond.hover();
    await page.emulateMedia({ reducedMotion: 'reduce' });
    for (const card of await cards.all()) {
        expect(
            await card.evaluate((node) =>
                Number.parseFloat(getComputedStyle(node).transitionDuration),
            ),
        ).toBeLessThan(0.001);
        await expect(card).toHaveCSS('transform', 'none');
    }
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    await expect(page.locator('.note-composer__toolbar')).toHaveCSS(
        'background-image',
        /radial-gradient/,
    );
});

test('password generation uses browser cryptographic randomness and preserves separate composer values', async ({
    page,
}) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.addInitScript(() => {
        const random = crypto.getRandomValues.bind(crypto);
        Object.assign(window, { passwordRandomCalls: 0 });
        crypto.getRandomValues = function <T extends ArrayBufferView | null>(array: T): T {
            if (array?.byteLength === 24)
                (window as Window & { passwordRandomCalls: number }).passwordRandomCalls++;
            return random(array);
        };
    });
    await page.goto('/');
    await page.getByTestId('prism-password-trigger').click();
    const popup = page.getByTestId('prism-password-popover');
    await popup.locator('#transfer-password').fill('old-password');
    await page.waitForLoadState('networkidle');
    const requests: string[] = [];
    page.on('request', (request) => requests.push(request.url()));
    await popup.getByRole('button', { name: 'Generate secure password' }).click();
    const filePassword = await popup.locator('#transfer-password').inputValue();
    expect(filePassword).toMatch(/^[A-Za-z0-9_-]{24}$/);
    await expect(popup.locator('#transfer-password')).toHaveAttribute('type', 'password');
    await popup.getByRole('button', { name: 'Done', exact: true }).click();
    await expect(page.getByTestId('prism-password-trigger')).toContainText('Password set');
    await expect(page.getByTestId('prism-password-trigger')).not.toContainText(filePassword);
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    await page.getByTestId('prism-password-trigger').click();
    await expect(popup.locator('#transfer-password')).toHaveValue('');
    await popup.getByRole('button', { name: 'Generate secure password' }).click();
    const notePassword = await popup.locator('#transfer-password').inputValue();
    expect(notePassword).toMatch(/^[A-Za-z0-9_-]{24}$/);
    expect(notePassword).not.toBe(filePassword);
    await popup.getByRole('button', { name: 'Done', exact: true }).click();
    await page.getByRole('tab', { name: 'Files', exact: true }).click();
    await page.getByTestId('prism-password-trigger').click();
    await expect(popup.locator('#transfer-password')).toHaveValue(filePassword);
    expect(
        await page.evaluate(
            () => (window as Window & { passwordRandomCalls: number }).passwordRandomCalls,
        ),
    ).toBe(2);
    expect(requests.filter((url) => !new URL(url).pathname.startsWith('/build/'))).toEqual([]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
    );
});
