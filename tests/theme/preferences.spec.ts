import { expect, test, type Page } from '@playwright/test';

async function popup(page: Page) {
    await page.getByRole('button', { name: 'Appearance', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Appearance', exact: true });
    await expect(dialog).toBeVisible();
    return dialog;
}

async function choose(page: Page, mode: string, preset: string) {
    const dialog = await popup(page);
    await dialog
        .getByRole('radiogroup', { name: 'Theme', exact: true })
        .getByRole('radio', { name: mode, exact: true })
        .click();
    await dialog
        .getByRole('radiogroup', { name: 'Color', exact: true })
        .getByRole('radio', { name: preset, exact: true })
        .click();
    return dialog;
}

test('guest presets persist, recolor browser artwork, and keep instance social images', async ({
    page,
}) => {
    await page.goto('/');
    const social = await page.locator('meta[property="og:image"]').getAttribute('content');
    const originalBackground = await page
        .locator('.fb-shell')
        .evaluate((el) => getComputedStyle(el).backgroundColor);
    let dialog = await choose(page, 'Dark', 'Blue');
    await expect(page.locator('html')).toHaveAttribute('data-fb-preset', 'blue');
    await expect
        .poll(() =>
            page.locator('.fb-shell').evaluate((el) => getComputedStyle(el).backgroundColor),
        )
        .not.toBe(originalBackground);
    const logo = page.locator('header .fb-brand img');
    await expect(logo).toHaveAttribute('src', /\/3d78d8\/dark\/logo\.svg/);
    await expect(page.locator('link[data-fb-favicon="favicon.svg"]')).toHaveAttribute(
        'href',
        /\/3d78d8\/dark\/favicon\.svg/,
    );
    await expect(dialog).toContainText('Saved in this browser.');
    await dialog.getByRole('button', { name: 'Done', exact: true }).click();
    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('data-fb-preset', 'blue');
    await expect(page.locator('html')).toHaveAttribute('data-fb-theme', 'dark');
    await expect(page.locator('meta[property="og:image"]')).toHaveAttribute('content', social!);
    dialog = await popup(page);
    for (const color of ['Purple', 'Teal', 'Green', 'Amber', 'Orange', 'Rose']) {
        await dialog.getByRole('radio', { name: color, exact: true }).click();
        await expect(page.locator('html')).toHaveAttribute('data-fb-preset', color.toLowerCase());
    }
    await dialog.getByRole('button', { name: 'Reset', exact: true }).click();
    await expect(page.locator('html')).toHaveAttribute('data-fb-preset', 'instance');
    await expect(dialog.getByRole('radio', { name: 'System', exact: true })).toBeChecked();
});

test('account preferences adopt guests once, persist across browsers, and restore guest choices on logout', async ({
    page,
    browser,
    baseURL,
}) => {
    const suffix = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;
    const account = {
        username: `theme_${suffix}`,
        email: `theme-${suffix}@example.test`,
        password: 'Theme8!Account',
    };
    await page.goto('/');
    let dialog = await choose(page, 'Light', 'Teal');
    await dialog.getByRole('button', { name: 'Done', exact: true }).click();
    await page.goto('/register');
    await page.getByLabel('Username', { exact: true }).fill(account.username);
    await page.getByLabel('Email', { exact: true }).fill(account.email);
    await page.getByLabel('Password', { exact: true }).fill(account.password);
    await page.getByLabel('Confirm password', { exact: true }).fill(account.password);
    const adoption = page.waitForResponse(
        (response) =>
            new URL(response.url()).pathname === '/account/appearance' &&
            response.request().method() === 'PATCH',
    );
    await page.getByRole('button', { name: 'Create account', exact: true }).click();
    expect((await adoption).ok()).toBe(true);
    await page.goto('/');
    await expect(page.locator('html')).toHaveAttribute('data-fb-preset', 'teal');
    await expect(page.locator('html')).toHaveAttribute('data-fb-theme', 'light');
    dialog = await choose(page, 'Dark', 'Rose');
    await expect(dialog).toContainText('Saved to your account.');
    await dialog.getByRole('button', { name: 'Done', exact: true }).click();
    await page.goto('/account');
    await page.getByRole('button', { name: 'Sign out', exact: true }).click();
    await expect(page.locator('html')).toHaveAttribute('data-fb-preset', 'teal');
    await expect(page.locator('html')).toHaveAttribute('data-fb-theme', 'light');

    const other = await browser.newContext({ baseURL });
    try {
        const second = await other.newPage();
        await second.goto('/');
        const guest = await choose(second, 'Light', 'Blue');
        await guest.getByRole('button', { name: 'Done', exact: true }).click();
        await second.goto('/login');
        await second.getByLabel('Email or username', { exact: true }).fill(account.email);
        await second.getByLabel('Password', { exact: true }).fill(account.password);
        await second.getByRole('button', { name: 'Sign in', exact: true }).click();
        await expect(second).toHaveURL(/\/account$/);
        await expect(second.locator('html')).toHaveAttribute('data-fb-preset', 'rose');
        await expect(second.locator('html')).toHaveAttribute('data-fb-theme', 'dark');
        await second.reload();
        await expect(second.locator('html')).toHaveAttribute('data-fb-preset', 'rose');
    } finally {
        await other.close();
    }
});

test('failed account saves can be retried and rapid selections finish with the latest choice', async ({
    page,
}) => {
    await page.goto('/login');
    await page.getByLabel('Email or username', { exact: true }).fill('admin@filebeam.test');
    await page.getByLabel('Password', { exact: true }).fill('password');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect(page).toHaveURL(/\/account$/);
    await page.goto('/');
    const dialog = await popup(page);
    await expect(dialog).toContainText('Saved to your account.');
    await page.route('**/account/appearance', (route) =>
        route.fulfill({ status: 503, contentType: 'application/json', body: '{}' }),
    );
    await dialog.getByRole('radio', { name: 'Blue', exact: true }).click();
    await expect(dialog).toContainText('Appearance could not be saved.');
    await page.unroute('**/account/appearance');
    await dialog.getByRole('button', { name: 'Retry', exact: true }).click();
    await expect(dialog).toContainText('Saved to your account.');
    for (const name of ['Purple', 'Orange', 'Green'])
        await dialog.getByRole('radio', { name, exact: true }).click();
    await expect(dialog).toContainText('Saved to your account.');
    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('data-fb-preset', 'green');
    const reset = await popup(page);
    await reset.getByRole('button', { name: 'Reset', exact: true }).click();
    await expect(reset).toContainText('Saved to your account.');
});
