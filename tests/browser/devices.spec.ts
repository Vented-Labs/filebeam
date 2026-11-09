import { expect, test, type Browser, type Page, type StorageState } from '@playwright/test';

const password = 'Browser8!Devices';
let authenticatedState: StorageState;

async function register(browser: Browser): Promise<StorageState> {
    const context = await browser.newContext();
    const page = await context.newPage();
    const suffix = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
    await page.goto('/register');
    await page.getByLabel('Username', { exact: true }).fill(`devices_${suffix}`);
    await page.getByLabel('Email', { exact: true }).fill(`devices-${suffix}@example.test`);
    await page.getByLabel('Password', { exact: true }).fill(password);
    await page.getByLabel('Confirm password', { exact: true }).fill(password);
    await page.getByRole('button', { name: 'Create account', exact: true }).click();
    await expect(page).toHaveURL(/\/verify-email$/);
    const state = await context.storageState();
    await context.close();
    return state;
}

async function installDevicesApi(
    page: Page,
): Promise<Array<{ method: string; body: Record<string, unknown> }>> {
    const writes: Array<{ method: string; body: Record<string, unknown> }> = [];
    const inventory = {
        revision: 4,
        generation: 1,
        devices: [
            {
                id: '01J00000000000000000000000',
                display_name: 'Studio Mac',
                platform: 'macos',
                public_key: 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=',
                fingerprint: '66687aadf862bd776c8fc18b8e9f8e20089714856ee233b3902a591d0d5f2925',
                key_version: 1,
                status: 'active',
                generation: 1,
                last_seen_at: null,
                presence_expires_at: null,
                revoked_at: null,
            },
        ],
    };
    await page.route(/\/api\/native\/v1\/devices(?:\/[^?]+)?$/, async (route) => {
        const request = route.request();
        if (request.method() === 'GET') return route.fulfill({ json: { data: inventory } });
        const body = (request.postDataJSON() ?? {}) as Record<string, unknown>;
        writes.push({ method: request.method(), body });
        if (request.method() === 'PATCH')
            inventory.devices[0].display_name = String(body.display_name);
        if (request.method() === 'DELETE') inventory.devices[0].status = 'revoked';
        return route.fulfill({ json: { data: { device: inventory.devices[0], revision: 5 } } });
    });
    return writes;
}

test.beforeAll(async ({ browser }) => {
    authenticatedState = await register(browser);
});

test('devices inventory renames and password-confirms a revoke without exposing file metadata', async ({
    browser,
}) => {
    const context = await browser.newContext({ storageState: authenticatedState });
    const page = await context.newPage();
    const writes = await installDevicesApi(page);
    await page.goto('/account/devices');
    await expect(page.getByRole('heading', { name: 'My devices' })).toBeVisible();
    await expect(page.getByText('Studio Mac')).toBeVisible();
    await expect(page.getByText(/key v1/)).toBeVisible();
    await expect(page.getByText('6668 7aad f862 bd77…')).toBeVisible();
    await page.getByRole('button', { name: 'Rename' }).click();
    await page.getByLabel('Device name').fill('Laptop');
    await page.getByRole('button', { name: 'Save' }).click();
    await expect(page.getByText('Laptop')).toBeVisible();
    await page.getByRole('button', { name: 'Revoke' }).click();
    await page.getByLabel('Current password').fill(password);
    await page.getByRole('button', { name: 'Revoke device' }).click();
    await expect(page.getByText(/Revoked/)).toBeVisible();
    expect(writes).toEqual([
        { method: 'PATCH', body: { display_name: 'Laptop' } },
        { method: 'DELETE', body: { current_password: password } },
    ]);
    await context.close();
});
