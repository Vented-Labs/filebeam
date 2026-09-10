import { expect, test } from '@playwright/test';

test('touch-screen editor fallback uses local monospace typography', async ({
    browser,
    baseURL,
}) => {
    const context = await browser.newContext({
        baseURL,
        hasTouch: true,
        viewport: { width: 320, height: 740 },
    });
    const page = await context.newPage();
    try {
        // Leave the initial app bundle intact, but force lazy CodeMirror imports to fail.
        await page.route('**/build/assets/dist-*.js', (route) => route.abort());
        await page.goto('/');
        await page.getByRole('tab', { name: 'Notes' }).click();
        const editor = page.locator('#private-note');
        await editor.fill('APP_SECRET=literal_value');
        await expect(editor).toHaveValue('APP_SECRET=literal_value');
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
            true,
        );
    } finally {
        await context.close();
    }
});

test('desktop composition, file queue, note editor, branding and auth navigation', async ({
    page,
}) => {
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await page.setViewportSize({ width: 1536, height: 1024 });
    await page.goto('/');
    await expect(page.getByRole('heading', { name: 'Drop your files here' })).toBeVisible();
    await expect(page.getByText('Share what matters. Securely.')).toHaveCount(0);
    await expect(page.locator('footer')).toContainText('\u00a9 2026 Vented');
    const github = page.getByRole('link', { name: 'GitHub' });
    await expect(github).toHaveAttribute('target', '_blank');
    await expect(github).toHaveAttribute('rel', /noopener/);
    await expect(page.locator('link[rel="icon"][type="image/svg+xml"]')).toHaveAttribute(
        'href',
        /\/favicon\.svg$/,
    );
    const logo = page.getByRole('link', { name: 'Filebeam home' }).locator('img');
    await expect(logo).toHaveAttribute('src', /\/brand\/filebeam-logo-header\.svg$/);
    await expect(page.locator('header .fb-brand__wordmark')).toHaveCount(0);
    expect(
        await logo.evaluate((image: HTMLImageElement) => ({
            loaded: image.complete && image.naturalWidth > 0,
            height: image.getBoundingClientRect().height,
            ratio: image.getBoundingClientRect().width / image.getBoundingClientRect().height,
        })),
    ).toEqual({ loaded: true, height: 32, ratio: 4.9375 });
    for (const [asset, type] of [
        ['/favicon.svg', /image\/svg\+xml/],
        ['/favicon.ico', /image\/(?:vnd\.microsoft\.icon|x-icon)/],
        ['/favicon-16x16.png', /image\/png/],
        ['/favicon-32x32.png', /image\/png/],
        ['/apple-touch-icon.png', /image\/png/],
    ] as const) {
        const response = await page.request.get(asset);
        expect(response.ok()).toBe(true);
        expect(response.headers()['content-type']).toMatch(type);
    }

    await page.locator('#filebeam-picker').setInputFiles([
        {
            name: 'Product brief.pdf',
            mimeType: 'application/pdf',
            buffer: Buffer.from('local only'),
        },
        { name: 'Design references.png', mimeType: 'image/png', buffer: Buffer.from('local only') },
    ]);
    await expect(page.getByRole('heading', { name: 'Your files', exact: true })).toBeVisible();
    const pond = page.getByTestId('file-pond');
    await expect(pond).toBeVisible();
    await expect
        .poll(() =>
            pond.evaluate((element) => {
                const container = element.getBoundingClientRect();
                const queue = element.querySelector('.file-queue')!.getBoundingClientRect();
                return (
                    queue.left >= container.left &&
                    queue.right <= container.right &&
                    queue.width > container.width * 0.8
                );
            }),
        )
        .toBe(true);
    await page
        .locator('#filebeam-picker')
        .setInputFiles({ name: 'Notes.txt', mimeType: 'text/plain', buffer: Buffer.from('draft') });
    await expect(page.getByTestId('prism-file-row')).toHaveCount(3);
    await page.getByRole('button', { name: 'Remove Product brief.pdf' }).click();
    await expect(page.getByTestId('prism-file-row')).toHaveCount(2);

    await page.getByRole('tab', { name: 'Notes' }).click();
    await expect(page.getByTestId('file-pond')).toBeHidden();
    const editor = page.locator('.cm-content[contenteditable="true"]');
    await editor.fill('<?php\n\nreturn [\n    "private" => true,\n];');
    await page.getByRole('combobox', { name: 'Note language' }).click();
    await page.getByRole('option', { name: 'PHP', exact: true }).click();
    await expect(page.getByText('untitled.php', { exact: true })).toBeVisible();
    await expect(page.locator('.cm-lineNumbers')).toBeVisible();
    await expect(page.locator('.cm-line span')).not.toHaveCount(0);
    expect(
        await page.evaluate(() => {
            const fonts = performance
                .getEntriesByType('resource')
                .filter((entry) => /\.woff2(?:\?|$)/.test(entry.name));
            return (
                fonts.length > 0 &&
                fonts.every((entry) => new URL(entry.name).origin === location.origin)
            );
        }),
    ).toBe(true);

    await page.locator('.fb-header__actions a[href="/login"]').click();
    await expect(page.getByRole('heading', { name: 'Welcome back' })).toBeVisible();
    await page.getByRole('link', { name: 'Create account', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Create your account' })).toBeVisible();
    expect(errors).toEqual([]);
});

test('auth navigation stays in Vue and validation belongs to its field', async ({ page }) => {
    await page.goto('/');
    await page.evaluate(() => {
        Object.assign(window, { navigationMarker: 'same-document', viewTransitionCount: 0 });
        const original = document.startViewTransition.bind(document);
        document.startViewTransition = ((callback: () => Promise<void>) => {
            (window as any).viewTransitionCount++;
            return original(callback);
        }) as typeof document.startViewTransition;
    });
    await page.locator('.fb-header__actions a[href="/login"]').click();
    await expect(page.getByRole('heading', { name: 'Welcome back' })).toBeVisible();
    await page.getByRole('link', { name: 'Create account', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Create your account' })).toBeVisible();
    expect(await page.evaluate(() => (window as any).navigationMarker)).toBe('same-document');
    expect(await page.evaluate(() => (window as any).viewTransitionCount)).toBe(0);
    await page.getByLabel('Email', { exact: true }).fill('validation-only@example.test');
    await page.getByLabel('Password', { exact: true }).fill('validation-only-password');
    await page.getByLabel('Confirm password', { exact: true }).fill('validation-only-password');
    const submit = page.getByRole('button', { name: 'Create account', exact: true });
    await submit.evaluate((button) => {
        button.closest('form')!.noValidate = true;
    });
    await submit.click();
    const username = page.getByLabel('Username', { exact: true });
    await expect(username).toHaveAttribute('aria-invalid', 'true');
    expect(
        await username.evaluate((input) => ({
            inputBorder: getComputedStyle(input).borderLeftWidth,
            groupBorder: getComputedStyle(input.closest('.fb-form-field')!).borderLeftWidth,
        })),
    ).toEqual({ inputBorder: '1px', groupBorder: '0px' });
    const errorIds = (await username.getAttribute('aria-describedby'))!.split(' ');
    await expect(
        page.locator(`[id="${errorIds.find((id) => id.endsWith('-error'))}"]`),
    ).toContainText('username');
    await username.fill('typed_username');
    await expect(username).toHaveAttribute('aria-invalid', 'false');
});

test('right-side drawers preserve the draft and do not shift or replay the background', async ({
    page,
}) => {
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Notes' }).click();
    await page.locator('.cm-content[contenteditable="true"]').fill('DRAFT_MUST_SURVIVE_AUTH');
    await page.getByLabel('Note title (optional)').fill('Draft title');
    await page.getByRole('combobox', { name: 'Retention period' }).click();
    await page.keyboard.press('Escape');
    const before = await page.evaluate(() => {
        const editor = document.querySelector('.cm-editor');
        Object.assign(window, { originalEditor: editor });
        return document.querySelector('.fb-header__brand')!.getBoundingClientRect().left;
    });
    await page.locator('.fb-header__actions a[href="/login"]').click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    const box = await dialog.boundingBox();
    expect(box!.x).toBeGreaterThan(800);
    expect(box!.y).toBeGreaterThan(0);
    expect(box!.height).toBeLessThan(960);
    const after = await page
        .locator('.fb-header__brand')
        .evaluate((node) => node.getBoundingClientRect().left);
    expect(after).toBeCloseTo(before, 0);
    await page.getByRole('button', { name: 'Close authentication' }).click();
    await expect(dialog).toHaveCount(0);
    await expect(page).toHaveURL(/\/$/);
    await expect(page.getByLabel('Note title (optional)')).toHaveValue('Draft title');
    await expect(page.locator('.cm-content[contenteditable="true"]')).toContainText(
        'DRAFT_MUST_SURVIVE_AUTH',
    );
    expect(
        await page.evaluate(
            () => (window as any).originalEditor === document.querySelector('.cm-editor'),
        ),
    ).toBe(true);
});

test('unavailable state and footer version are centered and branded', async ({ page }) => {
    await page.goto('/01AAAAAAAAAAAAAAAAAAAAAAAA');
    await expect(page.getByRole('heading', { name: 'Transfer unavailable' })).toBeVisible();
    const icon = page.locator('main svg[width="48"]');
    await expect(icon).toBeVisible();
    await expect(page.locator('.fb-footer')).toContainText('v0.1.0');
});

test('CLI instructions stay accessible on mobile and restore header focus', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 });
    await page.goto('/');
    const trigger = page.locator('header').getByRole('button', { name: 'Install CLI' });
    await trigger.click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByRole('heading', { name: 'Install CLI', exact: true })).toBeVisible();
    const platform = dialog.getByRole('combobox', { name: 'Platform' });
    await expect(platform).toHaveValue('');
    await expect(dialog.getByRole('status')).toContainText(
        'Choose a desktop platform to view installer instructions',
    );
    await platform.selectOption('linux');
    await expect(dialog.getByLabel('Installer command')).toHaveValue(
        "curl -fsSL 'https://releases.filebeam.io/cli/install.sh' | sh",
    );
    await expect(dialog).toContainText('beam up');
    await expect(dialog).toContainText('beam down <url or ulid>');
    await expect(dialog).toContainText('https://filebeam.io');
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
    );
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(trigger).toBeFocused();
});

test('editor selection stays visible when switching modes', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Notes' }).click();
    const editor = page.locator('.cm-content[contenteditable="true"]');
    await editor.fill('<div class="private">\n  private selection\n</div>');
    await page.getByRole('combobox', { name: 'Note language' }).click();
    await page.getByRole('option', { name: 'HTML', exact: true }).click();
    await editor.click();
    await editor.press('ControlOrMeta+Home');
    await editor.press('Shift+End');
    await expect(page.locator('.cm-selectionBackground')).toBeVisible();
    await page.getByRole('tab', { name: 'Files', exact: true }).scrollIntoViewIfNeeded();
    await page.getByRole('tab', { name: 'Files', exact: true }).click();
});

test('mobile queue and editor remain usable with reduced motion', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 });
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.goto('/');
    await page.locator('#filebeam-picker').setInputFiles({
        name: 'A very long filename that must not overflow on a phone.pdf',
        mimeType: 'application/pdf',
        buffer: Buffer.from('test'),
    });
    await expect(page.getByRole('heading', { name: 'Your files', exact: true })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
    );
    await page.getByRole('tab', { name: 'Notes' }).click();
    await page
        .locator('.cm-content[contenteditable="true"]')
        .fill('API_KEY="a local-only test value"\nDEBUG=false');
    await page.getByRole('combobox', { name: 'Note language' }).click();
    await page.getByRole('option', { name: '.ENV', exact: true }).click();
    await expect(page.getByTestId('file-pond')).toBeHidden();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
    );
});
