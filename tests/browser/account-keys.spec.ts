import { expect, test, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';

const accountPassword = 'Browser8!Account';

function uniqueAccount(): { username: string; email: string } {
    const suffix = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;
    return { username: `keys_${suffix}`, email: `keys-${suffix}@example.test` };
}

async function register(page: Page): Promise<{ username: string }> {
    const account = uniqueAccount();
    await page.goto('/register');
    await expect(page.getByRole('heading', { name: 'Create your account' })).toBeVisible();
    await page.getByLabel('Username', { exact: true }).fill(account.username);
    await page.getByLabel('Email', { exact: true }).fill(account.email);
    await page.getByLabel('Password', { exact: true }).fill(accountPassword);
    await page.getByLabel('Confirm password', { exact: true }).fill(accountPassword);
    const registration = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' &&
            new URL(response.url()).pathname === '/register',
    );
    await page.getByRole('button', { name: 'Create account', exact: true }).click();
    expect((await registration).status()).toBeLessThan(400);
    await expect(page).toHaveURL(/\/verify-email$/);
    await page.goto('/account');
    await expect(page.getByRole('heading', { name: 'Account' })).toBeVisible();
    return account;
}

async function openKeySetup(page: Page): Promise<void> {
    await page.getByRole('button', { name: 'Activate secure inbox' }).click();
    await expect(page.getByRole('radiogroup', { name: 'Account key custody' })).toBeVisible();
}

async function activatePasswordCustody(page: Page): Promise<void> {
    await openKeySetup(page);
    await page.getByRole('button', { name: 'Generate account key' }).click();
    await page.getByLabel('Current password', { exact: true }).fill(accountPassword);
    await page.getByLabel('Current password', { exact: true }).press('Enter');
    await expect(page.getByText('Secure inbox active', { exact: true })).toBeVisible();
}

test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
        Object.defineProperty(window, 'showSaveFilePicker', {
            value: undefined,
            configurable: true,
        });
    });
});

test('password setup submits with Enter, does not send its private export, and persists', async ({
    page,
}) => {
    await register(page);
    const keyWrites: string[] = [];
    page.on('request', (request) => {
        if (new URL(request.url()).pathname === '/account/keys' && request.method() === 'POST')
            keyWrites.push(request.postData() ?? '');
    });

    await activatePasswordCustody(page);
    expect(keyWrites).toHaveLength(1);
    const keyWrite = JSON.parse(keyWrites[0]) as Record<string, unknown>;
    expect(keyWrite.current_password).toBe(accountPassword);
    expect(keyWrite.private_key).toBeUndefined();
    expect(keyWrites[0]).not.toContain('fbsk1.');

    await page.reload();
    await expect(page.getByText('Secure inbox active', { exact: true })).toBeVisible();
    const profileCopy = page.getByRole('button', { name: 'Copy profile link' });
    await expect(profileCopy).toBeVisible();
    expect(await profileCopy.innerText()).toBe('');
    expect(await profileCopy.evaluate((button) => getComputedStyle(button).width)).toBe('40px');
});

test('self custody rejects a wrong key and keeps private controls usable on mobile', async ({
    page,
}) => {
    await page.setViewportSize({ width: 375, height: 812 });
    await register(page);
    await openKeySetup(page);
    await page.getByRole('radio', { name: /Keep the key yourself/ }).check({ force: true });
    await page.getByRole('button', { name: 'Generate account key' }).click();

    const privateCopy = page.getByRole('button', { name: 'Copy private key' });
    await expect(privateCopy).toHaveCSS('white-space', 'nowrap');
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
    );

    await page
        .getByLabel('Paste the exported private key to confirm')
        .fill(`fbsk1.${'A'.repeat(43)}`);
    await page.getByRole('button', { name: 'Save account key' }).click();
    await expect(page.getByRole('alert')).toContainText(
        /does not match|could not|authentication failed/i,
    );

    const download = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Download private key' }).click();
    const exportedPath = await (await download).path();
    expect(exportedPath).not.toBeNull();
    const privateExport = (await readFile(exportedPath!)).toString().trim();
    await page.getByLabel('Paste the exported private key to confirm').fill(privateExport);
    await page.getByRole('button', { name: 'Save account key' }).click();
    await expect(page.getByText('Secure inbox active', { exact: true })).toBeVisible();
});

test('password custody is selected by default, supports keyboard navigation, and cancel keeps profile private', async ({
    page,
}) => {
    let releaseKeys = (): void => undefined;
    const keysGate = new Promise<void>((resolve) => {
        releaseKeys = resolve;
    });
    await page.route('**/account/keys', async (route) => {
        await keysGate;
        await route.continue();
    });
    const { username } = await register(page);
    await expect(page.getByText('Loading account keys', { exact: true })).toBeVisible();
    const borderSampling = page.locator('.inbox-settings__state-height').evaluate(async (outer) => {
        let maximumOverflow = Number.NEGATIVE_INFINITY;
        let minimumPaintClearance = Number.POSITIVE_INFINITY;
        let samples = 0;
        const deadline = performance.now() + 700;
        while (performance.now() < deadline) {
            await new Promise(requestAnimationFrame);
            const card = outer.querySelector<HTMLElement>('.inbox-settings__activation');
            if (!card) continue;
            const outerRect = outer.getBoundingClientRect();
            const cardRect = card.getBoundingClientRect();
            const state = card.closest<HTMLElement>('.inbox-settings__state');
            const blur = Number.parseFloat(
                getComputedStyle(state!).filter.match(/blur\(([\d.]+)px\)/)?.[1] ?? '0',
            );
            const clipMargin = Number.parseFloat(
                getComputedStyle(outer).getPropertyValue('overflow-clip-margin'),
            );
            maximumOverflow = Math.max(maximumOverflow, cardRect.bottom - outerRect.bottom);
            minimumPaintClearance = Math.min(
                minimumPaintClearance,
                outerRect.bottom + clipMargin - cardRect.bottom - blur,
            );
            samples++;
        }
        return { maximumOverflow, minimumPaintClearance, samples };
    });
    releaseKeys();
    const borderResult = await borderSampling;
    expect(borderResult.samples).toBeGreaterThan(0);
    expect(borderResult.maximumOverflow).toBeLessThanOrEqual(0.5);
    expect(borderResult.minimumPaintClearance).toBeGreaterThanOrEqual(-0.5);
    await expect(page.getByRole('heading', { name: 'Create your receiving key' })).toBeVisible();
    expect((await page.goto(`/u/${username}`))?.status()).toBe(404);
    await page.goto('/account');
    await openKeySetup(page);

    const passwordCustody = page.getByRole('radio', { name: /Protect with password/ });
    const selfCustody = page.getByRole('radio', { name: /Keep the key yourself/ });
    await expect(passwordCustody).toBeChecked();
    await passwordCustody.focus();
    const focusedPaintClearance = () =>
        page.locator('.inbox-settings__state-height').evaluate((outer) => {
            const card = outer.querySelector<HTMLElement>(
                '.inbox-settings__custody-card:focus-within',
            );
            if (!card) return null;
            const outerRect = outer.getBoundingClientRect();
            const cardRect = card.getBoundingClientRect();
            const cardStyle = getComputedStyle(card);
            const outlineExtent =
                Number.parseFloat(cardStyle.outlineWidth) +
                Number.parseFloat(cardStyle.outlineOffset);
            const clipMargin = Number.parseFloat(
                getComputedStyle(outer).getPropertyValue('overflow-clip-margin'),
            );
            return {
                left: cardRect.left - outlineExtent - (outerRect.left - clipMargin),
                right: outerRect.right + clipMargin - (cardRect.right + outlineExtent),
            };
        });
    expect((await focusedPaintClearance())?.left).toBeGreaterThanOrEqual(-0.5);
    await page.keyboard.press('ArrowDown');
    await expect(selfCustody).toBeChecked();
    expect((await focusedPaintClearance())?.right).toBeGreaterThanOrEqual(-0.5);
    await page.keyboard.press('ArrowUp');
    await expect(passwordCustody).toBeChecked();
    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect(page.getByRole('button', { name: 'Activate secure inbox' })).toBeVisible();
    expect((await page.goto(`/u/${username}`))?.status()).toBe(404);
});

test('a public profile logo returns to home without the retired delivery heading', async ({
    page,
}) => {
    const { username } = await register(page);
    await activatePasswordCustody(page);
    await page.goto(`/u/${username}`);
    await expect(page.getByRole('heading', { name: `Send files to @${username}` })).toBeVisible();
    await page.getByRole('link', { name: /Filebeam home/ }).click();
    await expect(page).toHaveURL(/\/$/);
    await expect(page.getByText('Secure file delivery', { exact: true })).toHaveCount(0);
});

test('account crypto worker protects password and recipient envelopes', async ({ page }) => {
    let workerUrl: string | undefined;
    page.on('worker', (worker) => {
        workerUrl ??= worker.url();
    });
    await register(page);
    await openKeySetup(page);
    await page.getByRole('button', { name: 'Generate account key' }).click();
    await expect.poll(() => workerUrl).toBeTruthy();

    const checks = await page.evaluate(async (url) => {
        const worker = new Worker(url, { type: 'module' });
        const call = (message: Record<string, unknown>): Promise<Record<string, unknown>> =>
            new Promise((resolve) => {
                worker.addEventListener('message', (event) => resolve(event.data), { once: true });
                worker.postMessage(message);
            });
        const failed = (result: Record<string, unknown>) => result.ok === false;

        try {
            const first = await call({ type: 'generate' });
            const second = await call({ type: 'generate' });
            if (first.ok !== true || second.ok !== true) return { complete: false };
            const privateKey = String(first.privateKey);
            const publicKey = String(first.publicKey);
            const envelope = await call({
                type: 'password-envelope',
                privateKey,
                publicKey,
                userId: 41,
                password: 'worker-password',
            });
            if (envelope.ok !== true) return { complete: false };
            const unwrapped = await call({
                type: 'unwrap-password',
                envelope: envelope.envelope,
                publicKey,
                userId: 41,
                password: 'worker-password',
            });
            const wrongPassword = await call({
                type: 'unwrap-password',
                envelope: envelope.envelope,
                publicKey,
                userId: 41,
                password: 'wrong-password',
            });
            const tampered = JSON.parse(String(envelope.envelope)) as {
                kdf: { iterations: number };
            };
            tampered.kdf.iterations++;
            const tamperedKdf = await call({
                type: 'unwrap-password',
                envelope: JSON.stringify(tampered),
                publicKey,
                userId: 41,
                password: 'worker-password',
            });
            const validSelf = await call({ type: 'validate-self', privateKey, publicKey });
            const wrongSelf = await call({
                type: 'validate-self',
                privateKey: second.privateKey,
                publicKey,
            });
            const sealed = await call({
                type: 'seal-recipient',
                masterKey: 'AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE',
                publicKey,
                transferId: 'transfer-a',
                recipientId: 41,
                bundleId: 7,
            });
            if (sealed.ok !== true) return { complete: false };
            const opened = await call({
                type: 'open-recipient',
                privateKey,
                encryptedKey: sealed.encryptedKey,
                transferId: 'transfer-a',
                recipientId: 41,
                bundleId: 7,
            });
            const wrongPrivate = await call({
                type: 'open-recipient',
                privateKey: second.privateKey,
                encryptedKey: sealed.encryptedKey,
                transferId: 'transfer-a',
                recipientId: 41,
                bundleId: 7,
            });
            const wrongTransfer = await call({
                type: 'open-recipient',
                privateKey,
                encryptedKey: sealed.encryptedKey,
                transferId: 'transfer-b',
                recipientId: 41,
                bundleId: 7,
            });
            const wrongRecipient = await call({
                type: 'open-recipient',
                privateKey,
                encryptedKey: sealed.encryptedKey,
                transferId: 'transfer-a',
                recipientId: 42,
                bundleId: 7,
            });
            const wrongBundle = await call({
                type: 'open-recipient',
                privateKey,
                encryptedKey: sealed.encryptedKey,
                transferId: 'transfer-a',
                recipientId: 41,
                bundleId: 8,
            });
            return {
                complete:
                    unwrapped.ok === true &&
                    unwrapped.privateKey === privateKey &&
                    failed(wrongPassword) &&
                    failed(tamperedKdf) &&
                    validSelf.ok === true &&
                    failed(wrongSelf) &&
                    opened.ok === true &&
                    opened.masterKey === 'AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE' &&
                    failed(wrongPrivate) &&
                    failed(wrongTransfer) &&
                    failed(wrongRecipient) &&
                    failed(wrongBundle),
            };
        } finally {
            worker.terminate();
        }
    }, workerUrl!);
    expect(checks.complete).toBe(true);
});
