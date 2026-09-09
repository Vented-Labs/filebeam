import { expect, test, type Page } from '@playwright/test';

test.use({
    launchOptions: {
        ignoreDefaultArgs: ['--hide-scrollbars'],
        args: ['--disable-features=OverlayScrollbar'],
    },
});

type Transfer = { id: string; deleteToken: string };

let transfers: Transfer[];

test.beforeEach(() => {
    transfers = [];
});

test.afterEach(async ({ request }) => {
    await Promise.all(
        transfers.map(({ id, deleteToken }) =>
            request.delete(`/api/v1/transfers/${id}`, {
                headers: { 'X-Filebeam-Delete-Token': deleteToken },
            }),
        ),
    );
});

async function upload(
    page: Page,
    name = 'refinement-file.txt',
): Promise<{ link: string; id: string }> {
    await page.goto('/');
    await page.locator('#filebeam-picker').setInputFiles({
        name,
        mimeType: 'text/plain',
        buffer: Buffer.from('known browser refinement content'),
    });
    const created = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' &&
            response.url().endsWith('/api/v1/transfers') &&
            response.ok(),
    );
    await page.getByRole('button', { name: 'Encrypt and share' }).click();
    const payload = (await created).json() as Promise<{
        data: { id: string; delete_token: string };
    }>;
    const { data } = await payload;
    transfers.push({ id: data.id, deleteToken: data.delete_token });
    await expect(page.getByRole('heading', { name: 'Your encrypted link is ready' })).toBeVisible();
    return { link: await page.locator('#share-link').inputValue(), id: data.id };
}

async function note(page: Page, value: string): Promise<void> {
    await page.getByRole('tab', { name: 'Notes' }).click();
    await page.locator('.cm-content[contenteditable="true"]').fill(value);
}

test('keeps independent file and note composers across completed results and resets only the active note', async ({
    page,
}) => {
    const file = await upload(page);
    const outgoingFileResult = page.locator('.prism-stage__pane--result');
    await page.getByRole('tab', { name: 'Notes' }).click();
    await expect(outgoingFileResult).toHaveAttribute('inert', '');
    await expect(outgoingFileResult).toHaveAttribute('aria-hidden', 'true');
    await page.locator('.cm-content[contenteditable="true"]').fill('private note draft');
    await page.getByRole('tab', { name: 'Files' }).click();
    await expect(page.locator('#share-link')).toHaveValue(file.link);
    await note(page, 'private note draft');
    const created = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' &&
            response.url().endsWith('/api/v1/transfers') &&
            response.ok(),
    );
    await page.getByRole('button', { name: 'Encrypt and share' }).click();
    const data = (await (await created).json()).data as { id: string; delete_token: string };
    transfers.push({ id: data.id, deleteToken: data.delete_token });
    await expect(page.getByRole('heading', { name: 'Your encrypted link is ready' })).toBeVisible();
    const outgoingNoteResult = page.locator('.prism-stage__pane--result');
    await page.getByRole('button', { name: 'New transfer' }).click();
    await expect(outgoingNoteResult).toHaveAttribute('inert', '');
    await expect(outgoingNoteResult).toHaveAttribute('aria-hidden', 'true');
    await expect(page.locator('.cm-content[contenteditable="true"]')).toHaveText('');
    await page.getByRole('tab', { name: 'Files' }).click();
    await expect(page.locator('#share-link')).toHaveValue(file.link);
});

test('copies link and key independently after clipboard promises settle, with local retry and stable controls', async ({
    browser,
    baseURL,
}) => {
    const context = await browser.newContext({ baseURL });
    await context.addInitScript(() => {
        const clipboard = { values: [] as string[], reject: false };
        Object.assign(window, { __refinementClipboard: clipboard });
        Object.defineProperty(navigator, 'clipboard', {
            configurable: true,
            value: {
                writeText: (value: string) =>
                    clipboard.reject
                        ? Promise.reject(new Error('denied'))
                        : Promise.resolve(clipboard.values.push(value)),
            },
        });
    });
    const page = await context.newPage();
    try {
        await upload(page, 'clipboard-refinement.txt');
        const link = await page.locator('#share-link').inputValue();
        const linkButton = page.getByRole('button', { name: 'Copy link' });
        const keyButton = page.getByRole('button', { name: 'Copy key' });
        const widths = await Promise.all(
            [linkButton, keyButton].map((button) =>
                button.evaluate((node) => (node as HTMLElement).offsetWidth),
            ),
        );
        await linkButton.click();
        await expect(page.getByRole('button', { name: 'Copy link: copied' })).toBeVisible();
        expect(await page.evaluate(() => (window as any).__refinementClipboard.values)).toEqual([
            link,
        ]);
        await page.evaluate(() => ((window as any).__refinementClipboard.reject = true));
        await keyButton.click();
        await expect(page.getByRole('button', { name: 'Copy key: error' })).toBeVisible();
        await page.evaluate(() => ((window as any).__refinementClipboard.reject = false));
        await page.getByRole('button', { name: 'Copy key: error' }).click();
        await expect(page.getByRole('button', { name: 'Copy key: copied' })).toBeVisible();
        expect(
            await Promise.all(
                [linkButton, keyButton].map((button) =>
                    button.evaluate((node) => (node as HTMLElement).offsetWidth),
                ),
            ),
        ).toEqual(widths);
    } finally {
        await context.close();
    }
});

test('retains a failed deletion result and resets to a files composer only after a 202 delete', async ({
    page,
}) => {
    const transfer = await upload(page, 'delete-refinement.txt');
    await page.route(`**/api/v1/transfers/${transfer.id}`, async (route) =>
        route.fulfill({ status: 500 }),
    );
    await page.getByRole('button', { name: 'Delete now' }).click();
    await expect(page.locator('#share-link')).toBeVisible();
    await expect(page.getByText('Transfer deleted', { exact: true })).toHaveCount(0);
    await page.unroute(`**/api/v1/transfers/${transfer.id}`);
    await page.route(`**/api/v1/transfers/${transfer.id}`, async (route) =>
        route.fulfill({ status: 202 }),
    );
    await page.getByRole('button', { name: 'Delete now' }).click();
    await expect(page.getByText('Transfer deleted', { exact: true })).toBeVisible();
    await expect(page.getByTestId('file-pond')).toBeVisible();
});

test('uses file-only drag depth for the pond and removes visual motion when reduced', async ({
    page,
}) => {
    await page.goto('/');
    const pond = page.getByTestId('file-pond');
    await page.evaluate(() => {
        const files = new DataTransfer();
        files.items.add(new File(['x'], 'drag-refinement.txt', { type: 'text/plain' }));
        const event = (type: string, target: EventTarget) =>
            target.dispatchEvent(new DragEvent(type, { bubbles: true, dataTransfer: files }));
        const nested = document.querySelector('[data-testid="file-pond"] svg')!;
        event('dragenter', window);
        event('dragenter', nested);
        event('dragleave', nested);
    });
    await expect(pond).toHaveClass(/file-pond--dragging/);
    await page.evaluate(() => {
        const files = new DataTransfer();
        files.items.add(new File(['x'], 'drop-refinement.txt', { type: 'text/plain' }));
        window.dispatchEvent(new DragEvent('drop', { bubbles: true, dataTransfer: files }));
    });
    await expect(pond).not.toHaveClass(/file-pond--dragging/);
    await page.evaluate(() =>
        window.dispatchEvent(
            new DragEvent('dragenter', { bubbles: true, dataTransfer: new DataTransfer() }),
        ),
    );
    await expect(pond).not.toHaveClass(/file-pond--dragging/);
    await page.evaluate(() => window.dispatchEvent(new Event('blur')));
    await page.emulateMedia({ reducedMotion: 'reduce' });
    expect(
        parseFloat(
            await pond
                .locator('svg')
                .first()
                .evaluate((svg) => getComputedStyle(svg).transitionDuration),
        ),
    ).toBeLessThan(0.001);
});

test('preserves scrollbar geometry through portalled menus and an authentication drawer', async ({
    page,
}) => {
    await page.setViewportSize({ width: 1280, height: 720 });
    await page.goto('/');
    await page.locator('#filebeam-picker').setInputFiles({
        name: 'geometry.txt',
        mimeType: 'text/plain',
        buffer: Buffer.from('geometry'),
    });
    await page.evaluate(() => {
        document.body.style.minHeight = '300vh';
    });
    const geometry = () =>
        page.evaluate(() => ({
            gutter: innerWidth - document.documentElement.clientWidth,
            header: document.querySelector('header')!.getBoundingClientRect().right,
            bodyOverflow: getComputedStyle(document.body).overflow,
        }));
    const before = await geometry();
    expect(before.gutter).toBeGreaterThan(0);
    await page.getByRole('combobox', { name: 'Retention period' }).click();
    expect(await geometry()).toEqual(before);
    await page.keyboard.press('Escape');
    await page.locator('.fb-header__actions a[href="/login"]').click();
    await expect(page.getByRole('dialog')).toBeVisible();
    const locked = await geometry();
    expect(locked.header).toBe(before.header);
    expect(locked.bodyOverflow).toBe('hidden');
    await page.getByRole('button', { name: 'Close authentication' }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    expect(await geometry()).toEqual(before);
});

test('renders expiry metadata and submits only the public report contract without navigating', async ({
    page,
    request,
}) => {
    const transfer = await upload(page, 'download-refinement.txt');
    const metadata = (await (await request.get(`/api/v1/transfers/${transfer.id}`)).json()) as {
        data: { expires_at: string };
    };
    const expiry = new Date(metadata.data.expires_at);
    const expectedUtc = `${expiry.getUTCFullYear()}-${String(expiry.getUTCMonth() + 1).padStart(2, '0')}-${String(expiry.getUTCDate()).padStart(2, '0')} ${String(expiry.getUTCHours()).padStart(2, '0')}:${String(expiry.getUTCMinutes()).padStart(2, '0')}:${String(expiry.getUTCSeconds()).padStart(2, '0')} UTC`;
    await page.goto(transfer.link);
    const csrf = decodeURIComponent(
        (await page.context().cookies()).find((cookie) => cookie.name === 'XSRF-TOKEN')!.value,
    );
    await expect(page.getByRole('button', { name: 'Expiry details' }).locator('span')).toHaveText(
        /^Expires in/,
    );
    await page.getByRole('button', { name: 'Expiry details' }).click();
    await expect(page.getByText(expectedUtc, { exact: true })).toBeVisible();
    await expect(page.locator('[data-testid="transfer-meta"]')).not.toContainText(
        /download|ready/i,
    );
    const download = page.getByRole('button', { name: 'Download files' });
    expect((await download.boundingBox())!.height).toBeGreaterThanOrEqual(52);
    expect(
        await page.locator('[data-testid="transfer-meta"]').evaluate((meta) => {
            const [expiryButton, reportButton] = Array.from(meta.querySelectorAll('button'));
            return (
                reportButton.getBoundingClientRect().left >=
                expiryButton.getBoundingClientRect().right
            );
        }),
    ).toBe(true);
    const url = page.url();
    const submissions: { body: Record<string, string>; csrf?: string }[] = [];
    let status = 422;
    await page.route('**/reports', async (route) => {
        submissions.push({
            body: route.request().postDataJSON() as Record<string, string>,
            csrf: route.request().headers()['x-xsrf-token'],
        });
        await route.fulfill({
            status,
            contentType: 'application/json',
            body:
                status === 422
                    ? JSON.stringify({ errors: { category: ['Choose a category.'] } })
                    : '{}',
        });
    });
    await page.getByRole('button', { name: 'Report this transfer' }).click();
    const dialog = page.getByRole('dialog');
    expect(await dialog.evaluate((node) => node.scrollWidth <= node.clientWidth)).toBe(true);
    await dialog.getByLabel('Description').fill('Public browser refinement report.');
    await dialog.getByLabel('Email address').fill('reporter@example.test');
    await dialog.getByRole('button', { name: 'Submit report' }).click();
    await expect(dialog.getByLabel('Category')).toHaveAttribute('aria-invalid', 'true');
    await expect(dialog.getByLabel('Email address')).toHaveValue('reporter@example.test');
    await dialog.getByLabel('Category').click();
    expect(await page.evaluate(() => getComputedStyle(document.body).overflow)).toBe('hidden');
    await page.getByRole('option', { name: 'Other', exact: true }).click();
    expect(await page.evaluate(() => getComputedStyle(document.body).overflow)).toBe('hidden');
    status = 202;
    await dialog.getByRole('button', { name: 'Submit report' }).click();
    await expect(dialog).toContainText('Your report has been received and will be reviewed.');
    expect(page.url()).toBe(url);
    expect(submissions).toHaveLength(2);
    expect(submissions[1]).toEqual({
        body: {
            transfer_id: transfer.id,
            category: 'other',
            description: 'Public browser refinement report.',
            reporter_email: 'reporter@example.test',
            website: '',
        },
        csrf,
    });
});

test('start a new transfer uses Vue navigation and a view transition', async ({ page }) => {
    await page.goto('/01AAAAAAAAAAAAAAAAAAAAAAAA');
    await expect(page.getByRole('heading', { name: 'Transfer unavailable' })).toBeVisible();
    await page.evaluate(() => {
        Object.assign(window, { homeTransitionCount: 0, originalHomeDocument: document });
        new MutationObserver((records) => {
            if (
                records.some(
                    (record) =>
                        record.target instanceof Element &&
                        record.target.classList.contains('fb-page-enter-active'),
                )
            ) {
                (window as any).homeTransitionCount++;
            }
        }).observe(document.body, { subtree: true, attributes: true, attributeFilter: ['class'] });
    });
    await page.getByRole('link', { name: 'Start a new transfer' }).click();
    await expect(page.getByTestId('file-pond')).toBeVisible();
    expect(await page.evaluate(() => (window as any).originalHomeDocument === document)).toBe(true);
    expect(await page.evaluate(() => (window as any).homeTransitionCount)).toBeGreaterThan(0);
});

test('profile and home navigation stays within the Vue document without route ghosting', async ({
    page,
}) => {
    const username = `qa_nav_${Date.now().toString(36)}`;
    await page.goto('/register');
    await page.getByLabel('Username', { exact: true }).fill(username);
    await page.getByLabel('Email', { exact: true }).fill(`${username}@example.test`);
    await page.getByLabel('Password', { exact: true }).fill('Ui-navigation-password9!');
    await page.getByLabel('Confirm password', { exact: true }).fill('Ui-navigation-password9!');
    await page.getByRole('button', { name: 'Create account', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Verify your email' })).toBeVisible();
    await page.getByRole('button', { name: 'Close authentication' }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByTestId('file-pond')).toBeVisible();
    await page.evaluate(() => Object.assign(window, { profileDocument: document }));

    const openProfile = async () => {
        const frameSampling = page.locator('.fb-page-transition').evaluate(async (surface) => {
            const deadline = performance.now() + 5_000;
            while (!surface.hasAttribute('data-changing') && performance.now() < deadline)
                await new Promise(requestAnimationFrame);

            let samples = 0;
            let accountStateSamples = 0;
            let filteredStateFrames = 0;
            let nestedStateTransitionFrames = 0;
            let scaledPageFrames = 0;
            let exposedOutgoingFrames = 0;
            while (surface.hasAttribute('data-changing') && performance.now() < deadline) {
                const leaving = surface.querySelector<HTMLElement>('.fb-page-leave-active');
                if (leaving && getComputedStyle(leaving).visibility !== 'hidden')
                    exposedOutgoingFrames++;
                const entering = surface.querySelector<HTMLElement>('.fb-page-enter-active');
                if (entering) {
                    const transform = getComputedStyle(entering).transform;
                    const matrix =
                        transform === 'none'
                            ? new DOMMatrixReadOnly()
                            : new DOMMatrixReadOnly(transform);
                    if (Math.abs(matrix.a - 1) > 0.0001 || Math.abs(matrix.d - 1) > 0.0001)
                        scaledPageFrames++;
                }
                const accountStates = [
                    ...document.querySelectorAll<HTMLElement>('.inbox-settings__state'),
                ];
                if (accountStates.length > 0) accountStateSamples++;
                if (accountStates.some((state) => getComputedStyle(state).filter !== 'none'))
                    filteredStateFrames++;
                if (
                    accountStates.some(
                        (state) =>
                            state.classList.contains('inbox-settings__state-enter-active') ||
                            state.classList.contains('inbox-settings__state-leave-active'),
                    )
                )
                    nestedStateTransitionFrames++;
                samples++;
                await new Promise(requestAnimationFrame);
            }
            return {
                samples,
                accountStateSamples,
                filteredStateFrames,
                nestedStateTransitionFrames,
                scaledPageFrames,
                exposedOutgoingFrames,
            };
        });
        const transitionStarted = page.waitForFunction(
            () =>
                document.querySelectorAll('.fb-page-transition[data-changing] .fb-page-pane')
                    .length === 2,
        );
        await page.locator('header a[href="/account"]').click();
        await transitionStarted;
        const surfaceState = await page
            .locator('.fb-page-transition[data-changing]')
            .evaluate((surface) => {
                const leaving = surface.querySelector<HTMLElement>('.fb-page-leave-active');
                return {
                    outgoingHidden: leaving && getComputedStyle(leaving).visibility === 'hidden',
                    mask: getComputedStyle(surface, '::before').content,
                    ambient: getComputedStyle(surface.closest('.fb-shell')!).backgroundImage,
                };
            });
        expect(surfaceState.outgoingHidden).toBe(true);
        expect(surfaceState.mask).toBe('none');
        expect(surfaceState.ambient).toContain('radial-gradient');
        const frames = await frameSampling;
        expect(frames.samples).toBeGreaterThan(0);
        expect(frames.accountStateSamples).toBeGreaterThan(0);
        expect(frames.filteredStateFrames).toBe(0);
        expect(frames.nestedStateTransitionFrames).toBe(0);
        expect(frames.scaledPageFrames).toBe(0);
        expect(frames.exposedOutgoingFrames).toBe(0);
        await expect(page.getByRole('heading', { name: 'Account', exact: true })).toBeVisible();
        await expect(page.locator('.fb-page-transition[data-changing]')).toHaveCount(0);
    };

    for (const mode of ['Files', 'Notes']) {
        if (mode === 'Notes') {
            await page.getByRole('tab', { name: 'Notes', exact: true }).click();
            await expect(page.getByRole('tab', { name: 'Notes', exact: true })).toHaveAttribute(
                'aria-selected',
                'true',
            );
        }
        await openProfile();
        await page.getByRole('link', { name: 'Filebeam home' }).click();
        await expect(page.getByTestId('file-pond')).toBeVisible();
        await expect(page.locator('.fb-page-transition[data-changing]')).toHaveCount(0);
    }
    expect(await page.evaluate(() => (window as any).profileDocument === document)).toBe(true);
});

test('keeps note controls aligned, exposes mobile information, and changes auth modes in one drawer', async ({
    page,
}) => {
    await page.goto('/');
    await note(page, '<p>short</p>');
    const controls = page.locator('.note-composer__tools');
    const controlGeometry = () =>
        controls.evaluate((node) => {
            const wrap = node.querySelector<HTMLElement>('.note-composer__wrap')!;
            const picker = node.querySelector<HTMLElement>('[aria-label="Note language"]')!;
            return {
                wrap: wrap.offsetLeft,
                picker: picker.offsetLeft + picker.offsetWidth,
            };
        });
    const initial = await controlGeometry();
    await page.getByRole('combobox', { name: 'Note language' }).click();
    await page.getByRole('option', { name: 'HTML', exact: true }).click();
    await page.locator('.cm-content[contenteditable="true"]').fill('plain text '.repeat(200));
    expect(await controlGeometry()).toEqual(initial);
    await expect(page.getByText('Private draft', { exact: true })).toHaveCount(0);
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(page.locator('.fb-desktop-nav')).toBeHidden();
    await page.getByRole('button', { name: 'Open navigation' }).click();
    await expect(page.getByRole('menuitem', { name: 'GitHub' })).toHaveAttribute(
        'target',
        '_blank',
    );
    await expect(page.getByRole('menuitem', { name: 'GitHub' })).toHaveAttribute(
        'href',
        'https://github.com/Vented-Labs/filebeam',
    );
    await page.getByText('About', { exact: true }).last().click();
    await expect(page.getByRole('dialog')).toBeVisible();
    await page.getByRole('button', { name: 'Close dialog' }).click();
    await page.getByRole('button', { name: 'Open navigation' }).click();
    await page.getByRole('menuitem', { name: 'Register', exact: true }).click();
    const drawer = page.getByRole('dialog');
    await drawer.evaluate((node) => ((window as any).__refinementDrawer = node));
    await expect
        .poll(() => drawer.evaluate((node) => getComputedStyle(node).transform))
        .toBe('none');
    const paneGeometry = (selector: string) =>
        drawer.locator(selector).evaluate((pane) => ({
            x: pane.offsetLeft,
            y: pane.offsetTop,
            width: pane.offsetWidth,
            height: pane.offsetHeight,
        }));
    await page.getByLabel('Email', { exact: true }).fill('drawer@example.test');
    await page.getByLabel('Password', { exact: true }).fill('not-a-secret');
    const registerHeight = (await drawer.boundingBox())!.height;
    const registerPaneBox = await paneGeometry('.auth-drawer__pane');
    await page.getByRole('link', { name: 'Sign in', exact: true }).click();
    await expect(drawer.locator('.auth-drawer__pane')).toHaveCount(2);
    const outgoingRegisterBox = await paneGeometry('.auth-drawer__pane[inert]');
    for (const edge of ['x', 'y', 'width', 'height'] as const)
        expect(Math.abs(outgoingRegisterBox[edge] - registerPaneBox[edge])).toBeLessThan(1);
    await expect(drawer).toContainText('Welcome back');
    const checkbox = page.getByRole('checkbox', { name: 'Keep me signed in' });
    await checkbox.focus();
    await checkbox.press('Space');
    await expect(checkbox).toBeChecked();
    await expect(checkbox.locator('svg')).toBeVisible();
    expect(await drawer.evaluate((node) => node === (window as any).__refinementDrawer)).toBe(true);
    await expect(page.getByLabel('Email or username', { exact: true })).toHaveValue(
        'drawer@example.test',
    );
    await expect(page.getByLabel('Password', { exact: true })).toHaveValue('');
    await expect(drawer.locator('.auth-drawer__pane[inert]')).toHaveCount(0);
    const loginPaneBox = await paneGeometry('.auth-drawer__pane');
    await page.getByRole('link', { name: 'Forgot password?' }).click();
    await expect(drawer.locator('.auth-drawer__pane')).toHaveCount(2);
    const outgoingLoginBox = await paneGeometry('.auth-drawer__pane[inert]');
    for (const edge of ['x', 'y', 'width', 'height'] as const)
        expect(Math.abs(outgoingLoginBox[edge] - loginPaneBox[edge])).toBeLessThan(1);
    await expect(drawer).toContainText('Reset your password');
    expect((await drawer.boundingBox())!.height).not.toBe(registerHeight);
    expect(await drawer.evaluate((node) => node.scrollHeight <= node.clientHeight)).toBe(true);
    await page.getByRole('button', { name: 'Close authentication' }).click();
    await expect(drawer).toHaveCount(0);
    await page.evaluate(() => Object.assign(window, { refinementDocument: document }));
    await page.getByRole('link', { name: 'Filebeam home' }).click();
    await expect(page.getByTestId('file-pond')).toBeVisible();
    expect(await page.evaluate(() => (window as any).refinementDocument === document)).toBe(true);
});

test('copies the full decrypted note from its icon-only header action', async ({
    browser,
    baseURL,
}) => {
    const context = await browser.newContext({ baseURL });
    await context.addInitScript(() => {
        Object.assign(window, { copiedNote: '' });
        Object.defineProperty(navigator, 'clipboard', {
            configurable: true,
            value: {
                writeText: async (value: string) => {
                    (window as any).copiedNote = value;
                },
            },
        });
    });
    const page = await context.newPage();
    const content = Array.from({ length: 200 }, (_, index) => `Full note line ${index + 1}`).join(
        '\n',
    );
    try {
        await page.goto('/');
        await note(page, content);
        const creation = page.waitForResponse(
            (response) =>
                response.request().method() === 'POST' &&
                response.url().endsWith('/api/v1/transfers'),
        );
        await page.getByRole('button', { name: 'Encrypt and share' }).click();
        const { data } = await (await creation).json();
        transfers.push({ id: data.id, deleteToken: data.delete_token });
        await expect(page.locator('#share-link')).toBeVisible();
        await page.goto(await page.locator('#share-link').inputValue());
        await page.getByRole('button', { name: 'Decrypt note' }).click();
        const copy = page.getByRole('button', { name: 'Copy note', exact: true });
        await expect(copy).toBeVisible();
        await expect(copy.locator('svg')).toBeVisible();
        expect((await copy.innerText()).trim()).toBe('');
        await copy.click();
        await expect(page.getByRole('button', { name: 'Copy note: copied' })).toBeVisible();
        expect(await page.evaluate(() => (window as any).copiedNote)).toBe(content);
    } finally {
        await context.close();
    }
});

test('inline reporting reaches the CSRF-protected endpoint without leaving the transfer', async ({
    page,
    request,
}) => {
    const transfer = await upload(page, 'report-endpoint.txt');
    await page.goto(transfer.link);
    await expect(page.getByRole('button', { name: 'Download files' })).toBeVisible();
    // The endpoint gives the same confirmation for an expired link, without leaving a test report in moderation.
    await request.delete(`/api/v1/transfers/${transfer.id}`, {
        headers: { 'X-Filebeam-Delete-Token': transfers[0].deleteToken },
    });
    await page.getByRole('button', { name: 'Report this transfer' }).click();
    await page.getByRole('combobox', { name: 'Category' }).click();
    await page.getByRole('option', { name: 'Other', exact: true }).click();
    await page
        .getByRole('textbox', { name: 'Description' })
        .fill('Browser QA report for an expired test transfer.');
    const response = page.waitForResponse(
        (response) => response.request().method() === 'POST' && response.url().endsWith('/reports'),
    );
    await page.getByRole('button', { name: 'Submit report' }).click();
    expect((await response).status()).toBe(202);
    await expect(page.getByRole('dialog')).toContainText(
        'Your report has been received and will be reviewed.',
    );
    expect(page.url()).toBe(transfer.link);
    await page.getByRole('button', { name: 'Close', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Report this transfer' })).toBeFocused();
});
