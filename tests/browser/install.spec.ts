import { expect, test, type APIRequestContext, type Page } from '@playwright/test';

type CompleteFailure = Record<string, string[]>;

const defaults = {
    database: {
        driver: 'sqlite',
        transport: 'tcp',
        socket: '',
        host: '127.0.0.1',
        port: '',
        database: '/tmp/filebeam-test.sqlite',
        username: '',
        password: '',
        sslmode: 'prefer',
    },
    cache: {
        driver: 'file',
        transport: 'tcp',
        host: '127.0.0.1',
        port: 6379,
        username: '',
        password: '',
        database: 0,
        prefix: 'filebeam:',
    },
    instance: {
        name: 'Filebeam test',
        url: 'https://files.example.com',
        username_domain: '',
        visibility: 'private',
        auto_updates_enabled: false,
    },
    storage: [
        {
            name: 'Local storage',
            driver: 'local',
            root: 'primary',
            bucket: '',
            key: '',
            secret: '',
            region: 'us-east-1',
            endpoint: '',
            use_path_style_endpoint: false,
        },
    ],
    placement_mode: 'replicate',
};

function installationPage(html: string): string {
    const script =
        /(<script\b[^>]*\bdata-page(?:=(?:"[^"]*"|'[^']*'|[^\s>]+))?[^>]*>)([\s\S]*?)(<\/script>)/i;
    const match = html.match(script);
    if (!match) throw new Error('The production page did not contain an Inertia data-page script.');

    const existing = JSON.parse(match[2]) as { props?: Record<string, unknown>; version?: string };
    const page = {
        ...existing,
        component: 'Install',
        props: {
            ...existing.props,
            bootstrapRequired: false,
            challenge: null,
            unavailableReason: null,
        },
        url: '/install/',
        version: existing.version,
    };

    return html.replace(
        script,
        (_match, opening: string, _data: string, closing: string) =>
            `${opening}${JSON.stringify(page)}${closing}`,
    );
}

async function install(
    page: Page,
    request: APIRequestContext,
    completeFailure?: CompleteFailure,
    managedInstance = false,
): Promise<void> {
    const response = await request.get('/');
    if (!response.ok())
        throw new Error(`Unable to load the production shell (${response.status()}).`);
    const html = installationPage(await response.text());

    await page.route('**/install/**', async (route) => {
        const { pathname } = new URL(route.request().url());
        const method = route.request().method();
        if (method === 'GET' && pathname === '/install/') {
            return route.fulfill({ contentType: 'text/html', body: html });
        }
        if (method === 'POST' && pathname === '/install/configuration') {
            return route.fulfill({
                contentType: 'application/json',
                body: JSON.stringify({
                    databaseDrivers: ['sqlite', 'mysql'],
                    redisAvailable: true,
                    defaults,
                    prerequisites: [{ label: 'Test prerequisites', passed: true }],
                    chunks: {
                        minimum: 17,
                        maximum: 25_000_000,
                        recommended: 1_048_576,
                        post_max_size: '25M',
                        post_max_bytes: 25_000_000,
                        upload_max_filesize: '25M',
                        overhead: 16,
                    },
                    managed: {
                        container: false,
                        variant: null,
                        database: false,
                        cache: false,
                        instance: managedInstance,
                        auto_updates: false,
                    },
                }),
            });
        }
        if (method === 'PUT' && pathname === '/install/probe') {
            return route.fulfill({
                contentType: 'application/json',
                body: JSON.stringify({ bytes: route.request().postDataBuffer()?.byteLength ?? 0 }),
            });
        }
        if (method === 'POST' && pathname === '/install/complete') {
            return route.fulfill({
                status: completeFailure ? 422 : 200,
                contentType: 'application/json',
                body: JSON.stringify(
                    completeFailure
                        ? {
                              message: 'The submitted settings are invalid.',
                              errors: completeFailure,
                          }
                        : {
                              message: 'Installed',
                              redirect: 'https://files.example.com/admin/login',
                          },
                ),
            });
        }
        if (
            method === 'POST' &&
            ['/install/database', '/install/cache', '/install/storage'].includes(pathname)
        ) {
            return route.fulfill({
                contentType: 'application/json',
                body: JSON.stringify({ message: 'Connection verified.' }),
            });
        }
        throw new Error(`Unexpected installer request: ${method} ${pathname}`);
    });

    await page.goto('/install/');
}

async function ready(page: Page): Promise<void> {
    await page.locator('#install-token').fill('test-installation-token');
    await page.getByRole('button', { name: 'Check server readiness' }).click();
    await expect(page.getByRole('heading', { name: 'Authorize installation' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Continue to database' })).toBeEnabled();
}

async function review(page: Page): Promise<void> {
    await ready(page);
    await page.getByRole('button', { name: 'Continue to database' }).click();
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.locator('#install-admin-name').fill('Test Administrator');
    await page.locator('#install-admin-username').fill('test_admin');
    await page.locator('#install-admin-email').fill('admin@filebeam.test');
    await page.locator('#install-admin-password').fill('Static-test-password-1');
    await page.locator('#install-admin-password_confirmation').fill('Static-test-password-1');
    await page.locator('#install-admin-email_ownership_confirmed').check();
    await page.getByRole('button', { name: 'Review installation' }).click();
    await expect(page.getByRole('heading', { name: 'Complete installation' })).toBeVisible();
}

test('renders the Filebeam installer shell and accessible desktop progress', async ({
    page,
    request,
}) => {
    await install(page, request);

    await expect(page.getByRole('heading', { name: 'Set up Filebeam' })).toBeVisible();
    await expect(page.getByText('INSTALLER / v1')).toHaveCount(0);
    await expect(page.getByText(/This installer keeps/i)).toHaveCount(0);
    await expect(page.getByRole('img', { name: 'Filebeam' })).toHaveAttribute(
        'src',
        '/brand/filebeam-logo-header.svg',
    );
    const progress = page.getByRole('navigation', { name: 'Installation progress' });
    await expect(progress.getByRole('button', { name: 'Access' })).toHaveAttribute(
        'aria-current',
        'step',
    );
    await expect(progress.getByRole('button', { name: 'Database & cache' })).not.toHaveAttribute(
        'aria-current',
        'step',
    );
    await ready(page);
    await expect(progress.getByRole('button', { name: 'Database & cache' })).toBeEnabled();
    await expect(progress.getByRole('button', { name: 'Review' })).toBeDisabled();
    await page.evaluate(() => document.fonts.ready);
    expect(
        await progress
            .locator('.installer-step__marker')
            .first()
            .evaluate((element) => getComputedStyle(element).fontFamily),
    ).toContain('Inter');
    expect(await page.evaluate(() => document.fonts.check('500 14px "Inter Variable"'))).toBe(true);
});

test('uses native validation for the optional username domain before advancing', async ({
    page,
    request,
}) => {
    await install(page, request);
    await ready(page);
    await page.getByRole('button', { name: 'Continue to database' }).click();
    await page.getByRole('button', { name: 'Continue' }).click();

    const domain = page.locator('#install-instance-username_domain');
    await expect(domain).toHaveAttribute('pattern', /[A-Za-z0-9]/);
    await domain.fill('https://not-a-host.example/path');
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect(domain).toBeFocused();
    await expect(domain).toHaveAttribute('aria-invalid', 'true');
    await expect(page.locator('#install-instance-username_domain-error')).toContainText(
        'Enter a hostname such as example.com',
    );

    await domain.fill('');
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect(page.getByRole('heading', { name: 'Storage and chunks' })).toBeVisible();
});

test('maps completion field failures to their step, summary, focus, and field styling', async ({
    page,
    request,
}) => {
    await install(page, request, {
        'instance.username_domain': ['Choose a valid username domain.'],
        'admin.password': ['This password is too weak.'],
        'admin.password_confirmation': ['The passwords do not match.'],
    });
    await review(page);
    await page.getByRole('button', { name: 'Complete installation' }).click();

    const domain = page.locator('#install-instance-username_domain');
    await expect(page.getByRole('heading', { name: 'Instance' })).toBeVisible();
    await expect(domain).toBeFocused();
    await expect(domain).toHaveAttribute('aria-invalid', 'true');
    await expect(domain).toHaveAttribute(
        'aria-describedby',
        /install-instance-username_domain-error/,
    );
    await expect(page.locator('#install-error-summary')).toContainText('Username domain');
    await expect(page.locator('#install-error-summary a').first()).toHaveAttribute(
        'href',
        '#install-instance-username_domain',
    );
    await expect(page.getByRole('button', { name: /Instance.*Needs attention/i })).toBeVisible();
    const colors = await domain.evaluate((element) => {
        const sample = document.createElement('span');
        sample.style.color = 'var(--fb-danger)';
        document.body.append(sample);
        const danger = getComputedStyle(sample).color;
        sample.remove();
        return {
            border: getComputedStyle(element).borderTopColor,
            outline: getComputedStyle(element).outlineColor,
            danger,
        };
    });
    expect(colors.border).toBe(colors.danger);
    expect(colors.outline).toBe(colors.danger);

    await domain.fill('users.filebeam.test');
    await expect(domain).not.toHaveAttribute('aria-invalid', 'true');
    await page.getByRole('button', { name: 'Admin' }).click();
    await expect(page.locator('#install-admin-password')).toHaveAttribute('aria-invalid', 'true');
    await page.locator('#install-admin-password_confirmation').fill('corrected-confirmation');
    await expect(page.locator('#install-admin-password_confirmation')).not.toHaveAttribute(
        'aria-invalid',
        'true',
    );
    await expect(page.locator('#install-admin-password')).toHaveAttribute('aria-invalid', 'true');
});

test('keeps generic connection failures at section level and reindexes store failures', async ({
    page,
    request,
}) => {
    await install(page, request);
    await ready(page);
    await page.getByRole('button', { name: 'Continue to database' }).click();
    await page.route('**/install/database', (route) =>
        route.fulfill({
            status: 422,
            contentType: 'application/json',
            body: JSON.stringify({ errors: { database: ['Cannot connect to the database.'] } }),
        }),
    );
    await page.getByRole('button', { name: 'Test database' }).click();
    await expect(page.locator('#install-database-error')).toContainText('Cannot connect');
    await expect(page.locator('#install-database-database')).not.toHaveAttribute(
        'aria-invalid',
        'true',
    );

    await page.unroute('**/install/database');
    await page.getByRole('button', { name: 'Test database' }).click();
    await expect(page.locator('#install-database-error')).toHaveCount(0);
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByRole('button', { name: '+ Add another store' }).click();
    await page.route('**/install/storage', (route) =>
        route.fulfill({
            status: 422,
            contentType: 'application/json',
            body: JSON.stringify({ errors: { 'storage.root': ['This path is unavailable.'] } }),
        }),
    );
    await page.getByRole('button', { name: 'Test store' }).nth(1).click();
    await expect(page.locator('#install-storage-1-root')).toHaveAttribute('aria-invalid', 'true');
    await page.getByRole('button', { name: 'Remove' }).first().click();
    await expect(page.locator('#install-storage-0-root')).toHaveAttribute('aria-invalid', 'true');
});

test('distinguishes administrator password and confirmation errors', async ({ page, request }) => {
    await install(page, request);
    await review(page);
    await page.getByRole('button', { name: 'Back' }).click();
    await page.locator('#install-admin-password_confirmation').fill('different-password');
    await page.getByRole('button', { name: 'Review installation' }).click();
    await expect(page.locator('#install-admin-password_confirmation')).toBeFocused();
    await expect(page.locator('#install-admin-password_confirmation')).toHaveAttribute(
        'aria-invalid',
        'true',
    );
    await expect(page.locator('#install-admin-password')).not.toHaveAttribute(
        'aria-invalid',
        'true',
    );
    await page.locator('#install-admin-password').fill('different-password');
    await page.getByRole('button', { name: 'Review installation' }).click();
    await expect(page.getByRole('heading', { name: 'Complete installation' })).toBeVisible();
});

test('clears only obsolete errors when switching connection and storage types', async ({
    page,
    request,
}) => {
    await install(page, request);
    await ready(page);
    await page.getByRole('button', { name: 'Continue to database' }).click();
    await page.locator('#install-database-driver').selectOption('mysql');
    await page.locator('#install-database-transport').selectOption('socket');
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect(page.locator('#install-database-socket')).toHaveAttribute('aria-invalid', 'true');
    await page.locator('#install-database-transport').selectOption('tcp');
    await expect(page.locator('#install-error-summary')).not.toContainText('Socket path');
    await expect(page.locator('#install-database-username')).toHaveAttribute(
        'aria-invalid',
        'true',
    );
    await page.locator('#install-database-driver').selectOption('sqlite');
    await page.locator('#install-cache-driver').selectOption('redis');
    await page.locator('#install-cache-host').fill('');
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect(page.locator('#install-cache-host')).toHaveAttribute('aria-invalid', 'true');
    await page.locator('#install-cache-driver').selectOption('file');
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.locator('#install-storage-0-driver').selectOption('s3');
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect(page.locator('#install-storage-0-bucket')).toHaveAttribute('aria-invalid', 'true');
    await page.locator('#install-storage-0-driver').selectOption('local');
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect(
        page.getByRole('heading', { name: 'Create the first administrator' }),
    ).toBeVisible();
});

test('highlights an invalid installation token and recovers after correction', async ({
    page,
    request,
}) => {
    await install(page, request);
    await page.route('**/install/configuration', (route) =>
        route.fulfill({ status: 401, json: { message: 'The installation token is invalid.' } }),
    );
    await page.locator('#install-token').fill('invalid-test-token');
    await page.getByRole('button', { name: 'Check server readiness' }).click();
    await expect(page.locator('#install-token')).toHaveAttribute('aria-invalid', 'true');
    await expect(page.locator('#install-token')).toBeFocused();
    await page.unroute('**/install/configuration');
    await ready(page);
    await expect(page.locator('#install-token')).not.toHaveAttribute('aria-invalid', 'true');
});

test('explains errors in container-managed fields without focusing disabled controls', async ({
    page,
    request,
}) => {
    await install(
        page,
        request,
        { 'instance.username_domain': ['Use a hostname without a scheme or path.'] },
        true,
    );
    await review(page);
    await page.getByRole('button', { name: 'Complete installation' }).click();
    await expect(page.locator('#install-instance-username_domain')).toBeDisabled();
    await expect(page.locator('#install-instance-username_domain-error')).toBeFocused();
    await expect(page.getByText(/Instance identity is managed/)).toContainText(
        'Correct the container environment configuration',
    );
});

test('submits corrected configuration and follows the installation redirect', async ({
    page,
    request,
}) => {
    await install(page, request);
    await page.route('https://files.example.com/admin/login', (route) =>
        route.fulfill({ contentType: 'text/html', body: '<h1>Administrator sign in</h1>' }),
    );
    await review(page);
    const submitted = page.waitForRequest(
        (request) => new URL(request.url()).pathname === '/install/complete',
    );
    await page.getByRole('button', { name: 'Complete installation' }).click();
    expect((await submitted).postDataJSON()).toMatchObject({
        admin: { email_ownership_confirmed: true },
        chunk_max_size: 1_048_576,
    });
    await expect(page).toHaveURL('https://files.example.com/admin/login');
});

test('fits the mobile step picker without overflow and respects reduced motion', async ({
    page,
    request,
}) => {
    await page.setViewportSize({ width: 320, height: 720 });
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await install(page, request);

    const picker = page.locator('#install-step-picker');
    await expect(picker).toHaveAccessibleName('Step 1 of 6');
    await expect(picker.locator('option')).toHaveText([
        'Access',
        'Database & cache',
        'Instance',
        'Storage',
        'Admin',
        'Review',
    ]);
    await expect
        .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth))
        .toBe(true);
    expect(
        await page
            .locator('.installer-heading')
            .evaluate((element) => getComputedStyle(element).fontFamily),
    ).toMatch(/Inter|sans-serif/i);
    expect(
        await page
            .locator('.installer-heading')
            .evaluate((element) => getComputedStyle(element).fontFamily),
    ).not.toMatch(/mono/i);
    await ready(page);
    await picker.selectOption('2');
    await picker.selectOption('3');
    await page.locator('#install-instance-username_domain').fill('https://example.com');
    await picker.selectOption('4');
    await expect(picker).toHaveValue('3');
    await expect(page.locator('#install-instance-username_domain')).toBeFocused();
    await page.locator('#install-instance-username_domain').fill('users.example.com');
    await picker.selectOption('4');
    await expect(page.getByRole('heading', { name: 'Storage and chunks' })).toBeFocused();
    await expect
        .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth))
        .toBe(true);
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByRole('button', { name: 'Review installation' }).click();
    await expect(page.locator('#install-admin-name')).toBeFocused();
    await expect(page.locator('#install-admin-email_ownership_confirmed')).toHaveAttribute(
        'aria-invalid',
        'true',
    );
    await expect
        .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth))
        .toBe(true);
});
