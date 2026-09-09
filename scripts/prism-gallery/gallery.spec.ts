import { expect, test } from '@playwright/test';

test('renders every production specimen without API, peer, cookie, or runtime side effects', async ({
    page,
    context,
}) => {
    const errors: string[] = [];
    const apiRequests: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    page.on('request', (request) => {
        const url = new URL(request.url());
        if (url.pathname.startsWith('/api/') || url.origin !== 'http://127.0.0.1:4178')
            apiRequests.push(request.url());
    });
    await page.addInitScript(() => {
        const NativePeer = window.RTCPeerConnection;
        Object.assign(window, { __galleryPeerCount: 0 });
        if (!NativePeer) return;
        Object.defineProperty(window, 'RTCPeerConnection', {
            configurable: true,
            value: class extends NativePeer {
                constructor(...args: ConstructorParameters<typeof RTCPeerConnection>) {
                    (window as typeof window & { __galleryPeerCount: number }).__galleryPeerCount++;
                    super(...args);
                }
            },
        });
    });
    await page.goto('/');
    await expect(page.getByText('Development only', { exact: true })).toBeVisible();
    for (const section of [
        'CLI',
        'Buttons',
        'Inputs & password',
        'Files & Notes',
        'Methods & retention',
        'File rows',
        'Transfers',
        'Copy & notifications',
        'Dialogs',
        'Cards on a table',
    ]) {
        await page.getByRole('button', { name: new RegExp(`^${section}`) }).click();
        await expect(
            page.getByRole('heading', { name: section, exact: false }).last(),
        ).toBeVisible();
    }
    await page.getByRole('button', { name: /^Dialogs/ }).click();
    await page.getByRole('button', { name: 'Open consent fixture' }).click();
    await expect(page.getByRole('dialog', { name: 'WebRTC privacy' })).toBeVisible();
    await page.getByRole('button', { name: 'Not now' }).click();
    await expect(page.getByRole('dialog', { name: 'WebRTC privacy' })).toBeHidden();
    expect((await context.cookies()).some((cookie) => cookie.name === 'webRTCRiskAccepted')).toBe(
        false,
    );
    expect(await page.evaluate(() => (window as any).__galleryPeerCount)).toBe(0);
    expect(apiRequests).toEqual([]);
    expect(errors).toEqual([]);
});

test('CLI copy states retain full payloads, reject stale results, and offer persistent manual retry', async ({
    page,
}) => {
    await page.goto('/');
    await page.getByRole('button', { name: /^CLI Install/ }).click();
    const card = page.getByRole('region', { name: 'Download with CLI' });
    const field = card.getByRole('textbox', { name: 'Download command' });
    const copy = card.locator('.cli-command__copy');
    const initial = (await copy.boundingBox())!;
    await page.getByRole('switch', { name: 'Hold CLI clipboard' }).click();
    await copy.click();
    await expect(copy).toHaveAttribute('aria-label', 'Copy command: pending');
    await expect(copy).toBeDisabled();
    await page.getByRole('button', { name: 'keyless', exact: true }).click();
    await expect(field).not.toHaveValue(/#/);
    await page.getByRole('button', { name: 'Resolve clipboard' }).click();
    await expect(copy).toHaveAttribute('aria-label', 'Copy command');
    await page.getByRole('switch', { name: 'Hold CLI clipboard' }).click();
    await page.getByRole('switch', { name: 'Reject CLI clipboard' }).click();
    await copy.click();
    await expect(card).toContainText('command is selected');
    expect(
        await field.evaluate(
            (node: HTMLTextAreaElement) =>
                node.selectionStart === 0 && node.selectionEnd === node.value.length,
        ),
    ).toBe(true);
    await page.waitForTimeout(2300);
    await expect(card).toContainText('copy it manually or retry');
    await page.getByRole('switch', { name: 'Reject CLI clipboard' }).click();
    await copy.click();
    await expect(copy).toHaveAttribute('aria-label', 'Copy command: copied');
    const copied = (await copy.boundingBox())!;
    expect(copied.width).toBe(initial.width);
    expect(copied.height).toBe(initial.height);
    await expect(copy).toHaveAttribute('aria-label', 'Copy command', { timeout: 3500 });
    const long = page.getByRole('textbox', { name: 'Long command specimen' });
    const expected = await long.inputValue();
    await page.evaluate(() =>
        Object.defineProperty(navigator, 'clipboard', {
            configurable: true,
            value: {
                writeText: async (text: string) => Object.assign(window, { cliLongCopy: text }),
            },
        }),
    );
    await page.getByRole('button', { name: 'Copy long specimen', exact: true }).click();
    expect(
        await page.evaluate(() => (window as Window & { cliLongCopy?: string }).cliLongCopy),
    ).toBe(expected);
    for (const variant of ['expired', 'unavailable', 'live', 'note']) {
        await page.getByRole('button', { name: variant, exact: true }).click();
        await expect(card.getByRole('textbox')).toHaveCount(0);
        await expect(card.getByRole('button', { name: /^Copy command/ })).toHaveCount(0);
    }
});

test('exercises password, queue, method, notification, dialog, and interrupted motion states', async ({
    page,
}) => {
    await page.goto('/');
    await page.getByRole('button', { name: /^Inputs & password/ }).click();
    await page.getByTestId('prism-password-trigger').click();
    const password = page.locator('#transfer-password');
    await password.fill('short');
    await page.getByTestId('prism-password-popover').getByRole('button', { name: 'Done' }).click();
    await expect(password).toHaveValue('short');
    await expect(page.getByTestId('prism-password-trigger')).toHaveAttribute(
        'aria-invalid',
        'true',
    );
    await password.fill('long-enough-password');
    await page.getByTestId('prism-password-popover').getByRole('button', { name: 'Done' }).click();
    await expect(page.getByTestId('prism-password-popover')).toBeHidden();
    await page.getByTestId('prism-password-trigger').click();
    await expect(page.locator('#transfer-password')).toHaveValue('long-enough-password');
    await page.keyboard.press('Escape');

    await page.getByRole('button', { name: /^File rows/ }).click();
    await expect(page.getByTestId('prism-file-row')).toHaveCount(5);
    await page.getByRole('button', { name: 'Remove ready-document.pdf' }).click();
    await expect(page.getByTestId('prism-file-row')).toHaveCount(4);
    await page.getByRole('button', { name: 'Rotate rows' }).click();

    await page.getByRole('button', { name: /^Methods & retention/ }).click();
    await page.getByRole('radio', { name: 'WebRTC (live)' }).click();
    await expect(page.getByRole('radio', { name: 'WebRTC (live)' })).toBeChecked();
    await page.getByRole('button', { name: 'Notes', exact: true }).click();
    await expect(page.getByRole('switch', { name: 'Burn on read' })).toBeVisible();
    await page.getByRole('combobox', { name: 'Retention period' }).click();
    await page.keyboard.press('Escape');

    await page.getByRole('button', { name: /^Copy & notifications/ }).click();
    await page.getByRole('switch', { name: 'Reject clipboard promise' }).click();
    await page.getByRole('button', { name: 'Copy fixture value' }).click();
    await expect(page.getByRole('button', { name: 'Copy fixture value: error' })).toBeVisible();
    await page.getByRole('button', { name: 'Show notification' }).click();
    await expect(page.getByText('Gallery notification', { exact: true })).toBeVisible();

    await page.getByRole('button', { name: /^Dialogs/ }).click();
    await page.getByRole('button', { name: 'Open confirmation fixture' }).click();
    const dialog = page.getByRole('dialog', { name: 'Restart as stored HTTP?' });
    await expect(dialog).toBeVisible();
    await dialog.evaluate((element) => {
        element.setAttribute('data-prism-dialog-identity', 'preserved');
    });
    await dialog.getByRole('button', { name: 'Confirm fixture' }).click();
    const changedDialog = page.getByRole('dialog', { name: 'Confirmation received' });
    await expect(changedDialog).toBeVisible();
    await expect(changedDialog).toHaveAttribute('data-prism-dialog-identity', 'preserved');
    await page.getByRole('button', { name: 'Done' }).click();

    await page.getByRole('button', { name: /^Cards on a table/ }).click();
    await page.getByRole('button', { name: 'Interrupt 10 times' }).click();
    await expect(page.locator('.gallery-motion-card')).toBeVisible();
    await expect
        .poll(() => page.locator('.gallery-motion-stage').evaluate((node) => node.scrollHeight))
        .toBeGreaterThan(100);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
    );
});

test('honors reduced motion on the gallery motion surface', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.goto('/');
    await page.getByRole('button', { name: /^Cards on a table/ }).click();
    await page.getByRole('button', { name: 'Progress', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Encryption progress' })).toBeVisible();
    const longAnimations = await page.locator('.gallery-motion-stage').evaluate((element) =>
        element
            .getAnimations({ subtree: true })
            .filter((animation) => animation.playState === 'running')
            .map((animation) => Number(animation.effect?.getTiming().duration ?? 0))
            .filter((duration) => Number.isFinite(duration) && duration > 25),
    );
    expect(longAnimations).toEqual([]);
});

test('keeps material, policy, keyboard, collision, and toast contracts exact', async ({ page }) => {
    await page.goto('/');
    expect(
        await page.evaluate(() => ({
            toastIn: getComputedStyle(document.documentElement)
                .getPropertyValue('--fb-duration-toast-in')
                .trim(),
            toastOut: getComputedStyle(document.documentElement)
                .getPropertyValue('--fb-duration-toast-out')
                .trim(),
            dialogContent: getComputedStyle(document.documentElement)
                .getPropertyValue('--fb-duration-dialog-content')
                .trim(),
        })),
    ).toEqual({ toastIn: '280ms', toastOut: '180ms', dialogContent: '400ms' });

    await page.getByRole('button', { name: /^Files & Notes/ }).click();
    await expect(page.getByTestId('file-pond')).toHaveCSS('background-image', 'none');
    expect(
        await page
            .getByTestId('file-pond')
            .evaluate((node) => getComputedStyle(node, '::before').backgroundImage),
    ).toContain('radial-gradient');
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    await expect(page.locator('.note-composer__toolbar')).toHaveCSS(
        'background-image',
        /radial-gradient/,
    );
    await expect(page.locator('.note-composer__footer')).toHaveCSS('background-image', 'none');

    await page.getByRole('button', { name: /^Methods & retention/ }).click();
    await page.getByRole('button', { name: 'HTTP only' }).click();
    await expect(page.getByRole('radio', { name: 'HTTP (stored)' })).toBeChecked();
    await expect(page.getByRole('radio', { name: 'WebRTC (live)' })).toHaveCount(0);
    await page.getByRole('button', { name: 'WebRTC only' }).click();
    await expect(page.getByRole('radio', { name: 'WebRTC (live)' })).toBeChecked();
    await expect(page.getByRole('radio', { name: 'HTTP (stored)' })).toHaveCount(0);

    await page.getByRole('button', { name: 'Both finite' }).click();
    await page.setViewportSize({ width: 390, height: 844 });
    const live = page.getByRole('radio', { name: 'WebRTC (live)' });
    await live.click();
    await page.waitForTimeout(400);
    const [indicator, selected] = await Promise.all([
        page.getByTestId('prism-driver-indicator').boundingBox(),
        live.boundingBox(),
    ]);
    expect(indicator).not.toBeNull();
    expect(selected).not.toBeNull();
    expect(Math.abs(indicator!.x - selected!.x)).toBeLessThanOrEqual(1);
    expect(Math.abs(indicator!.width - selected!.width)).toBeLessThanOrEqual(1);

    await page.getByRole('button', { name: 'Notes', exact: true }).click();
    const burn = page.getByRole('switch', { name: 'Burn on read' });
    await burn.focus();
    await burn.press('Space');
    await expect(burn).toBeChecked();
    const retention = page.getByRole('combobox', { name: 'Retention period' });
    await retention.click();
    const menu = page.getByRole('listbox');
    const bounds = await menu.boundingBox();
    expect(bounds).not.toBeNull();
    expect(bounds!.x).toBeGreaterThanOrEqual(8);
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(382);
    await page.keyboard.press('Escape');
    await expect(retention).toBeFocused();

    await page.getByRole('button', { name: /^Copy & notifications/ }).click();
    const show = page.getByRole('button', { name: 'Show notification' });
    await show.click();
    const toast = page.getByText('Gallery notification', { exact: true });
    await expect(toast).toBeVisible();
    await expect(toast.locator('..').locator('..')).toHaveCSS('animation-duration', '0.28s');
    await show.click();
    await expect(page.getByText('Gallery notification', { exact: true })).toHaveCount(1);
    await page.getByRole('button', { name: 'Dismiss notification' }).click();
    await expect(toast).toBeHidden();
});
