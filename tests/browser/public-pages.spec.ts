import { expect, test, type Page } from '@playwright/test';

const noHorizontalOverflow = (page: Page) =>
    page.evaluate(() => document.documentElement.scrollWidth <= innerWidth);

test('primary navigation links the public pages and marks the current page', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto('/');

    const navigation = page.getByRole('navigation', { name: 'Primary navigation' });
    await expect(navigation.getByRole('link', { name: 'GitHub' })).toHaveAttribute(
        'target',
        '_blank',
    );
    await expect(page.locator('header').getByRole('link', { name: /Send files/ })).toHaveCount(0);
    await expect(page.locator('header').getByRole('link', { name: 'Sign in' })).toBeVisible();

    const headerHeight = await page.locator('header').evaluate((header) => header.offsetHeight);
    await navigation.getByRole('link', { name: 'Features' }).click();
    await expect(page).toHaveURL(/\/features$/);
    await expect(navigation.getByRole('link', { name: 'Features' })).toHaveAttribute(
        'aria-current',
        'page',
    );
    await expect(page.getByRole('heading', { name: 'End-to-end encrypted.' })).toBeVisible();
    // The shell keeps the same header layout on every page.
    expect(await page.locator('header').evaluate((header) => header.offsetHeight)).toBe(
        headerHeight,
    );

    const indicator = page.locator('.fb-nav-indicator');
    const centred = async (name: string) => {
        const link = (await navigation.getByRole('link', { name }).boundingBox())!;
        const dot = (await indicator.boundingBox())!;
        return Math.abs(link.x + link.width / 2 - (dot.x + dot.width / 2)) < 1.5;
    };
    await expect.poll(() => centred('Features')).toBe(true);
    await navigation.getByRole('link', { name: 'Changelog' }).click();
    await expect(page).toHaveURL(/\/changelog$/);
    await expect(navigation.getByRole('link', { name: 'Changelog' })).toHaveAttribute(
        'aria-current',
        'page',
    );
    await expect.poll(() => centred('Changelog')).toBe(true);
    await expect(page.getByRole('heading', { name: 'Changelog.' })).toBeVisible();
    expect(errors).toEqual([]);
});

test('footer keeps appearance, version, information, and CLI controls', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto('/features');
    const footer = page.locator('footer');
    await expect(footer.getByRole('button', { name: 'Appearance', exact: true })).toBeVisible();
    await expect(footer.locator('.fb-footer__version')).toHaveAttribute('href', /\/changelog/);
    for (const name of ['About', 'Privacy', 'Help']) {
        await footer.getByRole('button', { name, exact: true }).click();
        await expect(page.getByRole('dialog', { name })).toBeVisible();
        await page.getByRole('button', { name: 'Close dialog' }).click();
        await expect(footer.getByRole('button', { name, exact: true })).toBeFocused();
    }
    await expect(page.locator('header').getByRole('button', { name: 'Appearance' })).toHaveCount(0);
});

test('compact navigation keeps every destination in the menu', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/changelog');
    await page.getByRole('button', { name: 'Open navigation' }).click();
    for (const name of ['Features', 'Changelog', 'GitHub', 'Sign in', 'About', 'Privacy', 'Help'])
        await expect(page.getByRole('menuitem', { name, exact: true })).toBeVisible();
    await expect(page.getByRole('menuitem', { name: 'Changelog', exact: true })).toHaveAttribute(
        'aria-current',
        'page',
    );
    await page.getByRole('menuitem', { name: 'Features', exact: true }).click();
    await expect(page).toHaveURL(/\/features$/);
});

test('public pages have no horizontal overflow across widths', async ({ page }) => {
    for (const width of [320, 390, 768, 1024, 1440]) {
        await page.setViewportSize({ width, height: 900 });
        for (const path of ['/', '/features', '/changelog']) {
            await page.goto(path);
            await page.evaluate(() => document.fonts.ready);
            expect(await noHorizontalOverflow(page), `${path} at ${width}px`).toBe(true);
        }
    }
});

test('the dropzone outline never crosses its artwork or hint', async ({ page }) => {
    for (const [width, height] of [
        [1440, 900],
        [1440, 740],
        [1536, 1100],
        [390, 844],
    ]) {
        await page.setViewportSize({ width: width!, height: height! });
        await page.goto('/');
        const pond = page.getByTestId('file-pond');
        await pond.hover();
        const geometry = await pond.evaluate((element) => {
            const box = element.getBoundingClientRect();
            const inset = getComputedStyle(element, '::after');
            const top = box.top + parseFloat(inset.top);
            const bottom = box.bottom - parseFloat(inset.bottom);
            const art = element.querySelector('.file-pond__art-surface')!.getBoundingClientRect();
            const hint = element
                .querySelector('.file-pond__empty > small')!
                .getBoundingClientRect();
            return { top, bottom, art: art.top, hint: hint.bottom };
        });
        expect(geometry.art, `${width}x${height} artwork`).toBeGreaterThan(geometry.top + 4);
        expect(geometry.hint, `${width}x${height} hint`).toBeLessThan(geometry.bottom - 4);
    }
});

test('feature illustrations stay local and describe real behaviour', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    const requests: string[] = [];
    page.on('request', (request) => {
        // Laravel Boost posts development browser logs; everything else must stay read-only.
        if (!['GET', 'HEAD'].includes(request.method()) && !request.url().includes('/_boost/'))
            requests.push(request.url());
    });
    await page.goto('/features');

    await expect(page.getByText(/Planned|Coming in a future release/)).toHaveCount(0);
    await expect(page.locator('.key-link')).not.toContainText('/t/');
    await expect(page.locator('.key-link .example-domain')).toHaveText(new URL(page.url()).host);

    const destination = page.getByLabel('Destination device in the illustration');
    for (const [value, label] of [
        ['laptop', 'Laptop'],
        ['server', 'Server'],
    ]) {
        await destination.selectOption(value!);
        await expect(page.locator('.device-target-card')).toHaveAttribute('data-target', value!);
        await expect(page.locator('.device-target-card strong')).toHaveText(label!);
    }

    await page.getByRole('button', { name: /Sam/ }).click();
    await expect(page.locator('.friend-send code')).toHaveText('beam to @sam notes.zip');

    await page.getByRole('tab', { name: 'TUI' }).click();
    await expect(page.locator('.tui-mock')).toBeVisible();
    await expect(page.locator('.tui-shortcuts')).toBeVisible();

    await page.getByRole('switch', { name: 'Include the key in the example link' }).click();
    await expect(page.locator('.separate-key')).toBeVisible();
    expect(requests).toEqual([]);
});

test('changelog search, jump, reader, and back navigation use the local mirror', async ({
    page,
}) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto('/changelog');
    test.skip(
        (await page.locator('.cl-row').count()) === 0,
        'Requires a synchronized release mirror (php artisan filebeam:sync-changelog --full).',
    );
    const githubApi: string[] = [];
    page.on('request', (request) => {
        if (request.url().includes('api.github.com')) githubApi.push(request.url());
    });

    const tag = (await page.locator('.cl-version').first().textContent())!.trim();
    await page.keyboard.press('/');
    const search = page.getByRole('searchbox', { name: 'Search releases' });
    await expect(search).toBeFocused();
    await search.fill(tag);
    await expect(page).toHaveURL(new RegExp(`q=${encodeURIComponent(tag)}`));
    await expect(search).toBeFocused();
    await expect(page.locator('.cl-version').first()).toHaveText(tag);

    await search.fill('%_no-such-release_%');
    await expect(page.getByRole('heading', { name: 'No matching releases.' })).toBeVisible();
    await page
        .locator('.cl-empty')
        .getByRole('button', { name: /Clear search/ })
        .click();
    await expect(page).toHaveURL(/\/changelog$/);

    await page.locator('.cl-version').first().click();
    await expect(page).toHaveURL(/\/changelog\/(id\/)?[^/]+$/);
    await expect(page.locator('.cl-article h1')).toBeVisible();
    await expect(
        page.getByRole('navigation', { name: 'Primary navigation' }).getByRole('link', {
            name: 'Changelog',
        }),
    ).toHaveAttribute('aria-current', 'page');
    await page.getByRole('link', { name: 'All releases' }).click();
    await expect(page).toHaveURL(/\/changelog$/);

    const last = page.locator('#changelog-jump option').last();
    await page.locator('#changelog-jump').selectOption((await last.getAttribute('value'))!);
    await expect(
        page.locator('.cl-version').filter({ hasText: (await last.getAttribute('value'))! }),
    ).toBeFocused();
    expect(githubApi).toEqual([]);
});

test('the palette orbit and its mark share one centre at rest and on hover', async ({ page }) => {
    for (const width of [1440, 1024, 768, 390]) {
        await page.setViewportSize({ width, height: 900 });
        await page.goto('/features');
        await page.locator('.palette-orbit').scrollIntoViewIfNeeded();
        const centres = () =>
            page.locator('.palette-orbit').evaluate((orbit) => {
                const centre = (element: Element) => {
                    const box = element.getBoundingClientRect();
                    return [box.left + box.width / 2, box.top + box.height / 2];
                };
                return [
                    orbit,
                    orbit.querySelector('.palette-orbit-ring')!,
                    orbit.querySelector('.palette-fold img')!,
                ].map(centre);
            });
        for (const state of ['rest', 'hover']) {
            if (state === 'hover') await page.locator('.appearance-card h2').hover();
            await expect
                .poll(
                    async () => {
                        const [orbit, ...layers] = await centres();
                        return layers.every(
                            ([x, y]) =>
                                Math.abs(x! - orbit![0]!) < 0.5 && Math.abs(y! - orbit![1]!) < 0.5,
                        );
                    },
                    { message: `${width}px ${state}` },
                )
                .toBe(true);
        }
    }
});

test('the release title arrow stays inline and centred on the cap height', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto('/changelog');
    const link = page.locator('.cl-row:not(.cl-sr-only) .cl-title:not(.cl-sr-only) a').first();
    test.skip((await link.count()) === 0, 'Requires a synchronized release with a title.');
    await link.hover();
    const offset = await link.evaluate((anchor) => {
        const tail = anchor.querySelector('.cl-title__tail')!;
        const probe = document.createElement('span');
        probe.style.cssText = 'display:inline-block;width:0;height:0';
        tail.prepend(probe);
        const baseline = probe.getBoundingClientRect().bottom;
        probe.remove();
        const style = getComputedStyle(anchor);
        const context = document.createElement('canvas').getContext('2d')!;
        context.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
        const capCentre = baseline - context.measureText('H').actualBoundingBoxAscent / 2;
        const arrow = anchor.querySelector('.cl-title__arrow')!.getBoundingClientRect();
        return Math.abs(arrow.top + arrow.height / 2 - capCentre);
    });
    expect(offset).toBeLessThan(1.5);
});
