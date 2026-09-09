import { expect, test, type Page } from '@playwright/test';

const profilePath = '/qa-auth-profile';
const mainSite = 'http://main.filebeam.test:8123/filebeam';

async function profile(
    page: Page,
    mainSiteUrl: string,
    { anonymousUploads = true, registration = true } = {},
): Promise<void> {
    const response = await page.request.get('/');
    expect(response.ok()).toBe(true);
    const html = await response.text();
    const script = /(<script\b[^>]*\bdata-page[^>]*>)([\s\S]*?)(<\/script>)/i;
    const match = html.match(script);
    if (!match) throw new Error('The page did not contain an Inertia data-page script.');
    const data = JSON.parse(match[2]);
    data.component = 'Receive';
    data.url = profilePath;
    data.props.auth = { user: null };
    Object.assign(data.props.filebeam, {
        main_site_url: mainSiteUrl,
        anonymous_uploads_enabled: anonymousUploads,
        registration_enabled: registration,
    });
    data.props.recipient = {
        id: 1,
        username: 'receiver',
        public_key: '',
        account_key_bundle_id: 1,
        version: 1,
        fingerprint: '',
    };
    await page.route(`**${profilePath}`, (route) =>
        route.fulfill({
            contentType: 'text/html',
            body: html.replace(script, () => `${match[1]}${JSON.stringify(data)}${match[3]}`),
        }),
    );
    await page.goto(profilePath);
}

for (const mobile of [false, true]) {
    for (const destination of ['login', 'register']) {
        test(`${mobile ? 'mobile' : 'desktop'} profile ${destination} opens the canonical site with document navigation`, async ({
            page,
        }) => {
            if (mobile) await page.setViewportSize({ width: 390, height: 844 });
            await profile(page, `${mainSite}///`);
            await expect(
                page.getByRole('heading', { name: 'Send files to @receiver' }),
            ).toBeVisible();
            let links = page.locator('.fb-header__actions');
            if (mobile) {
                await expect(links.locator('.fb-sign-in-link')).toBeHidden();
                await page.getByRole('button', { name: 'Open navigation' }).click();
                links = page.getByRole('menu');
            }
            await expect(links.locator('a', { hasText: 'Sign in' })).toHaveAttribute(
                'href',
                `${mainSite}/login`,
            );
            await expect(links.locator('a', { hasText: 'Register' })).toHaveAttribute(
                'href',
                `${mainSite}/register`,
            );
            const url = `${mainSite}/${destination}`;
            await page.route(url, (route) =>
                route.fulfill({
                    contentType: 'text/html',
                    body: '<h1>Main site authentication</h1>',
                }),
            );
            const navigation = page.waitForRequest(url);
            await links.locator(`a[href="${url}"]`).click();
            const request = await navigation;
            expect(request.isNavigationRequest()).toBe(true);
            expect(request.headers()['x-inertia']).toBeUndefined();
            await expect(page).toHaveURL(url);
            await expect(
                page.getByRole('heading', { name: 'Main site authentication' }),
            ).toBeVisible();
        });
    }
}

test('the sign-in gate uses canonical links and respects disabled registration', async ({
    page,
}) => {
    await profile(page, mainSite, { anonymousUploads: false });
    const gate = page
        .locator('section')
        .filter({
            has: page.getByRole('heading', { name: 'Sign in to share files' }),
        })
        .last();
    await expect(gate.getByRole('link', { name: 'Sign in', exact: true })).toHaveAttribute(
        'href',
        `${mainSite}/login`,
    );
    await expect(gate.getByRole('link', { name: 'Register', exact: true })).toHaveAttribute(
        'href',
        `${mainSite}/register`,
    );

    await profile(page, mainSite, { anonymousUploads: false, registration: false });
    await expect(page.getByRole('link', { name: 'Register', exact: true })).toHaveCount(0);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.getByRole('button', { name: 'Open navigation' }).click();
    await expect(page.getByRole('menuitem', { name: 'Register', exact: true })).toHaveCount(0);
    await expect(page.getByRole('menuitem', { name: 'Sign in', exact: true })).toHaveAttribute(
        'href',
        `${mainSite}/login`,
    );
});

test('same-origin auth links and mode switches keep the Vue document', async ({
    page,
    baseURL,
}) => {
    await profile(page, baseURL!);
    await page.evaluate(() => Object.assign(window, { authDocument: document }));
    await page.locator('.fb-header__actions').getByRole('link', { name: 'Sign in' }).click();
    await expect(page.getByRole('heading', { name: 'Welcome back' })).toBeVisible();
    await page
        .getByRole('navigation', { name: 'Authentication links' })
        .getByRole('link', { name: 'Create account' })
        .click();
    await expect(page.getByRole('heading', { name: 'Create your account' })).toBeVisible();
    await page
        .getByRole('navigation', { name: 'Authentication links' })
        .getByRole('link', { name: 'Sign in' })
        .click();
    await expect(page.getByRole('heading', { name: 'Welcome back' })).toBeVisible();
    expect(
        await page.evaluate(
            () => (window as Window & { authDocument?: Document }).authDocument === document,
        ),
    ).toBe(true);
});
