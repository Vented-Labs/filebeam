import { expect, test, type Browser, type Page, type StorageState } from '@playwright/test';

type Policy = 'anyone' | 'authenticated' | 'friends' | 'nobody';
type Contact = {
    id: number;
    name: string;
    username: string;
    status: 'accepted' | 'incoming' | 'outgoing';
    canSend: boolean | null;
    autoDownload: boolean | null;
    effective: { canSend: boolean; autoDownload: boolean };
};
type Snapshot = {
    settings: { receivingPolicy: Policy; autoDownloadFriends: boolean; revision: number };
    contacts: Contact[];
    blocked: Array<Pick<Contact, 'id' | 'name' | 'username'>>;
};
type Write = { path: string; method: string; body: Record<string, unknown> };
type ApiFailure = { status: number; message?: string; body?: string; abort?: boolean };
type MutationResult = Snapshot | { settings: Snapshot['settings'] } | ApiFailure;

const password = 'Browser8!Contacts';
let authenticatedState: StorageState;
let authenticatedUsername: string;

function account(): { username: string; email: string } {
    const suffix = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
    return { username: `contacts_${suffix}`, email: `contacts-${suffix}@example.test` };
}

function snapshot(
    contacts: Contact[] = [],
    settings: Partial<Snapshot['settings']> = {},
    blocked: Snapshot['blocked'] = [],
): Snapshot {
    return {
        settings: {
            receivingPolicy: 'anyone',
            autoDownloadFriends: false,
            revision: 1,
            ...settings,
        },
        contacts,
        blocked,
    };
}

function contact(
    id: number,
    username: string,
    status: Contact['status'] = 'accepted',
    overrides: Partial<Contact> = {},
): Contact {
    return {
        id,
        name: username.replace(
            /(^|_)([a-z])/g,
            (_, prefix: string, letter: string) => `${prefix}${letter.toUpperCase()}`,
        ),
        username,
        status,
        canSend: null,
        autoDownload: null,
        effective: { canSend: true, autoDownload: false },
        ...overrides,
    };
}

async function registerShell(browser: Browser): Promise<{ state: StorageState; username: string }> {
    const context = await browser.newContext();
    const page = await context.newPage();
    const user = account();
    await page.goto('/register');
    await expect(page.getByRole('heading', { name: 'Create your account' })).toBeVisible();
    await page.getByLabel('Username', { exact: true }).fill(user.username);
    await page.getByLabel('Email', { exact: true }).fill(user.email);
    await page.getByLabel('Password', { exact: true }).fill(password);
    await page.getByLabel('Confirm password', { exact: true }).fill(password);
    let registration = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' &&
            new URL(response.url()).pathname === '/register',
    );
    await page.getByRole('button', { name: 'Create account', exact: true }).click();
    let response = await registration;
    if (response.status() === 429) {
        const delay = Number(response.headers()['retry-after'] ?? 60);
        await page.waitForTimeout(Math.min(60, Math.max(1, delay)) * 1000 + 250);
        await page.getByLabel('Password', { exact: true }).fill(password);
        await page.getByLabel('Confirm password', { exact: true }).fill(password);
        registration = page.waitForResponse(
            (candidate) =>
                candidate.request().method() === 'POST' &&
                new URL(candidate.url()).pathname === '/register',
        );
        await page.getByRole('button', { name: 'Create account', exact: true }).click();
        response = await registration;
    }
    expect(response.status()).toBeLessThan(400);
    await expect(page).toHaveURL(/\/verify-email$/);
    const state = await context.storageState();
    await context.close();
    return { state, username: user.username };
}

async function installApi(
    page: Page,
    initial: Snapshot,
    mutate?: (write: Write, current: Snapshot) => MutationResult | Promise<MutationResult>,
    read?: (current: Snapshot) => Snapshot | Promise<Snapshot>,
): Promise<{ writes: Write[]; setSnapshot: (next: Snapshot) => void }> {
    let current = initial;
    const writes: Write[] = [];
    const endpoint = /\/api\/native\/v1\/(contacts(?:\/[^/?]+)?|account\/receiving)$/;
    await page.route(endpoint, async (route) => {
        const request = route.request();
        const path = new URL(request.url()).pathname.replace('/api/native/v1/', '');
        if (request.method() === 'GET') {
            await route.fulfill({ json: { data: await (read?.(current) ?? current) } });
            return;
        }
        const body = (request.postDataJSON() ?? {}) as Record<string, unknown>;
        const write = { path, method: request.method(), body };
        writes.push(write);
        const result = await (mutate?.(write, current) ?? current);
        if ('status' in result) {
            if (result.abort) {
                await route.abort();
                return;
            }
            if (result.body) {
                await route.fulfill({
                    status: result.status,
                    contentType: 'text/html',
                    body: result.body,
                });
                return;
            }
            await route.fulfill({ status: result.status, json: { message: result.message } });
            return;
        }
        if ('settings' in result && !('contacts' in result)) {
            current = { ...current, settings: result.settings };
            await route.fulfill({ json: { data: result.settings } });
            return;
        }
        current = result;
        await route.fulfill({ json: { data: current } });
    });
    return { writes, setSnapshot: (next) => (current = next) };
}

async function openContacts(
    browser: Browser,
    initial: Snapshot,
    run: (page: Page, api: Awaited<ReturnType<typeof installApi>>) => Promise<void>,
    mutate?: Parameters<typeof installApi>[2],
    read?: Parameters<typeof installApi>[3],
): Promise<void> {
    const context = await browser.newContext({ storageState: authenticatedState });
    const page = await context.newPage();
    try {
        const api = await installApi(page, initial, mutate, read);
        await page.goto('/account/contacts');
        await expect(page.getByRole('heading', { name: 'Contacts' })).toBeVisible();
        await run(page, api);
    } finally {
        await context.close();
    }
}

async function choose(page: Page, label: string, option: string): Promise<void> {
    const select = page.getByRole('combobox', { name: label, exact: true });
    await expect(select).toBeEnabled();
    await expect(select).toHaveClass(/fb-select-trigger/);
    await select.click();
    await expect(page.locator('.fb-select-content')).toBeVisible();
    await page.getByRole('option', { name: option, exact: true }).click();
}

function identity(page: Page, name: string, username: string) {
    return page.getByRole('button', { name: new RegExp(`${name}.*@${username}`, 'i') });
}

async function expectRequestActionOnOneLine(page: Page, formName: string): Promise<void> {
    const form = page.getByRole('form', { name: formName, exact: true });
    const action = form.getByRole('button', { name: 'Send request', exact: true });
    await expect(action).toBeVisible();
    const metrics = await action.evaluate((button) => {
        const textNodes = document.createTreeWalker(button, NodeFilter.SHOW_TEXT);
        const textLines: Array<{ top: number; width: number }> = [];
        let node: Node | null;
        while ((node = textNodes.nextNode())) {
            if (!node.textContent?.trim()) continue;
            const range = document.createRange();
            range.selectNode(node);
            textLines.push(
                ...[...range.getClientRects()]
                    .filter((rect) => rect.width > 0 && rect.height > 0)
                    .map((rect) => ({ top: Math.round(rect.top), width: rect.width })),
            );
        }
        const distinctLines = [...new Set(textLines.map((rect) => rect.top))];
        const form = button.closest('form');
        const input = form?.querySelector('input');
        const prefix = form?.querySelector<HTMLElement>('.at');
        const buttonRect = button.getBoundingClientRect();
        const inputRect = input?.getBoundingClientRect();
        const prefixRect = prefix?.getBoundingClientRect();
        const panelRect = button.closest('.contacts-panel')?.getBoundingClientRect();
        return {
            buttonWidth: buttonRect.width,
            textWidth: Math.max(...textLines.map((rect) => rect.width), 0),
            textLineCount: distinctLines.length,
            whiteSpace: getComputedStyle(button).whiteSpace,
            controlsWithinPanel:
                inputRect && panelRect
                    ? [inputRect, buttonRect].every(
                          (rect) =>
                              rect.left >= panelRect.left &&
                              rect.right <= panelRect.right &&
                              rect.top >= panelRect.top &&
                              rect.bottom <= panelRect.bottom,
                      )
                    : false,
            controlsOverlap: inputRect
                ? buttonRect.left < inputRect.right &&
                  inputRect.left < buttonRect.right &&
                  buttonRect.top < inputRect.bottom &&
                  inputRect.top < buttonRect.bottom
                : true,
            prefixAlignedWithInput:
                inputRect && prefixRect
                    ? prefixRect.left >= inputRect.left &&
                      prefixRect.right <= inputRect.right &&
                      prefixRect.top < inputRect.bottom &&
                      inputRect.top < prefixRect.bottom
                    : false,
        };
    });
    expect(metrics.whiteSpace).toBe('nowrap');
    expect(metrics.textLineCount).toBe(1);
    expect(metrics.buttonWidth).toBeGreaterThan(metrics.textWidth);
    expect(metrics.controlsWithinPanel).toBe(true);
    expect(metrics.controlsOverlap).toBe(false);
    expect(metrics.prefixAlignedWithInput).toBe(true);
}

async function activateAndCaptureStageAnimation(
    control: ReturnType<Page['getByRole']>,
): Promise<{ names: string[]; computed: string[] }> {
    return control.evaluate(async (element) => {
        const target = element as HTMLElement;
        target.focus();
        target.dispatchEvent(
            new PointerEvent('pointerdown', {
                bubbles: true,
                button: 0,
                buttons: 1,
                isPrimary: true,
                pointerType: 'mouse',
            }),
        );
        target.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, buttons: 1 }));
        target.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0 }));
        target.dispatchEvent(new MouseEvent('click', { bubbles: true, button: 0 }));
        await Promise.resolve();
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        const root = document.querySelector('.contacts-stage-stack')!;
        const animations = root
            .getAnimations({ subtree: true })
            .filter((animation) => animation instanceof CSSAnimation)
            .filter((animation) => animation.animationName.startsWith('contacts-stage'));
        const captured = {
            names: animations.map((animation) => animation.animationName),
            computed: [...root.querySelectorAll<HTMLElement>('.contacts-stage')].map(
                (stage) => getComputedStyle(stage).animationName,
            ),
        };
        await Promise.all(animations.map((animation) => animation.finished.catch(() => undefined)));
        return captured;
    });
}

async function toggleAndCaptureDisclosureAnimation(
    control: ReturnType<Page['getByRole']>,
): Promise<{
    names: string[];
    heights: string[];
    state: string | null;
    ariaHidden: string | null;
    inert: boolean;
}> {
    return control.evaluate(async (element) => {
        const target = element as HTMLElement;
        target.dispatchEvent(
            new PointerEvent('pointerdown', {
                bubbles: true,
                button: 0,
                buttons: 1,
                isPrimary: true,
                pointerType: 'mouse',
            }),
        );
        target.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, buttons: 1 }));
        target.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0 }));
        target.dispatchEvent(new MouseEvent('click', { bubbles: true, button: 0 }));
        await Promise.resolve();
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        const body = document.querySelector<HTMLElement>('.receiving-body')!;
        const animations = body
            .getAnimations({ subtree: true })
            .filter((animation) => animation instanceof CSSAnimation)
            .filter((animation) => animation.animationName.startsWith('contacts-disclosure'));
        const captured = {
            names: animations.map((animation) => animation.animationName),
            heights: animations.flatMap(
                (animation) =>
                    (animation.effect as KeyframeEffect | null)
                        ?.getKeyframes()
                        .map((keyframe) => String(keyframe.height ?? '')) ?? [],
            ),
            state: body.dataset.state ?? null,
            ariaHidden: body.getAttribute('aria-hidden'),
            inert: body.inert,
        };
        await Promise.all(animations.map((animation) => animation.finished.catch(() => undefined)));
        return captured;
    });
}

test.describe('Contacts redesign', () => {
    test.beforeAll(async ({ browser }) => {
        const registered = await registerShell(browser);
        authenticatedState = registered.state;
        authenticatedUsername = registered.username;
    });

    test('handles loading, empty, load failure, retry, and an expired session', async ({
        browser,
    }) => {
        const context = await browser.newContext({ storageState: authenticatedState });
        const page = await context.newPage();
        let attempts = 0;
        let releaseFirstResponse: (() => void) | undefined;
        const firstResponse = new Promise<void>((resolve) => (releaseFirstResponse = resolve));
        await page.route(/\/api\/native\/v1\/contacts$/, async (route) => {
            attempts += 1;
            if (attempts === 1) {
                await firstResponse;
                await route.fulfill({ status: 500, json: { message: 'Unavailable' } });
                return;
            }
            await route.fulfill({ json: { data: snapshot() } });
        });
        await page.goto('/account/contacts');
        await expect(
            page.getByRole('status').filter({ hasText: /loading contacts/i }),
        ).toBeVisible();
        releaseFirstResponse?.();
        await expect(page.getByRole('alert')).toContainText(/request could not be completed/i);
        await page.getByRole('button', { name: 'Retry', exact: true }).click();
        await expect(page.getByRole('heading', { name: 'Add your first friend' })).toBeVisible();
        await expect(page.getByLabel('Username', { exact: true })).toBeVisible();
        await context.close();

        const expired = await browser.newContext({ storageState: authenticatedState });
        const expiredPage = await expired.newPage();
        await expiredPage.route(/\/api\/native\/v1\/contacts$/, (route) =>
            route.fulfill({ status: 401, json: { message: 'Unauthenticated.' } }),
        );
        await expiredPage.goto('/account/contacts');
        await expect(expiredPage.getByRole('alert')).toContainText(/session has expired/i);
        await expired.close();
    });

    test('normalizes additions, preserves drafts on errors, detects local duplicates, and prevents double writes', async ({
        browser,
    }) => {
        const alex = contact(1, 'alex');
        const incoming = contact(2, 'jamie', 'incoming');
        const outgoing = contact(3, 'morgan', 'outgoing');
        await openContacts(
            browser,
            snapshot([alex, incoming, outgoing]),
            async (page, api) => {
                await page.getByRole('button', { name: 'Add friend', exact: true }).click();
                const username = page.getByLabel('Username', { exact: true });
                await username.fill(' @NEW_FRIEND ');
                await page.getByRole('button', { name: 'Send request', exact: true }).click();
                await expect(
                    page.getByRole('status').filter({ hasText: 'Request sent to @new_friend' }),
                ).toBeVisible();
                expect(api.writes).toContainEqual({
                    path: 'contacts/new_friend',
                    method: 'POST',
                    body: expect.objectContaining({ action: 'request' }),
                });
                await expect(username).toHaveValue('');

                await username.fill('no!');
                await page.getByRole('button', { name: 'Send request', exact: true }).click();
                await expect(page.getByRole('alert')).toContainText(/exact username/i);
                await expect(username).toHaveValue('no!');
                expect(api.writes).toHaveLength(1);

                for (const [value, message] of [
                    ['alex', /already.*friend/i],
                    ['jamie', /already requested.*friend/i],
                    ['morgan', /already pending/i],
                ] as const) {
                    await username.fill(value);
                    await page.getByRole('button', { name: 'Send request', exact: true }).click();
                    await expect(page.getByRole('alert')).toContainText(message);
                }
                expect(api.writes).toHaveLength(1);
                await username.fill(authenticatedUsername);
                await page.getByRole('button', { name: 'Send request', exact: true }).click();
                await expect(page.getByRole('alert')).toContainText(/cannot add yourself/i);
                expect(api.writes).toHaveLength(2);
            },
            (write, current) =>
                write.path === `contacts/${authenticatedUsername}`
                    ? { status: 422, message: 'You cannot add yourself.' }
                    : current,
        );

        const unavailable = snapshot([contact(4, 'taylor')]);
        await openContacts(
            browser,
            unavailable,
            async (page, api) => {
                await page.getByRole('button', { name: 'Add friend', exact: true }).click();
                const username = page.getByLabel('Username', { exact: true });
                await username.fill('missing');
                await page.getByRole('button', { name: 'Send request', exact: true }).dblclick();
                await expect(page.getByRole('alert')).toContainText(/unavailable/i);
                await expect(username).toHaveValue('missing');
                expect(api.writes).toHaveLength(1);

                for (const value of ['malformed', 'offline']) {
                    await username.fill(value);
                    await page.getByRole('button', { name: 'Send request', exact: true }).click();
                    await expect(page.getByRole('alert')).toContainText(
                        /could(?:n't| not) confirm/i,
                    );
                    await expect(username).toHaveValue(value);
                }
                expect(api.writes).toHaveLength(3);
            },
            (write) => {
                if (write.path === 'contacts/missing')
                    return {
                        status: 404,
                        message:
                            'This username or receiving inbox is unavailable on this instance.',
                    };
                if (write.path === 'contacts/malformed')
                    return { status: 404, body: '<!doctype html><title>Not found</title>' };
                return { status: 0, abort: true };
            },
        );
    });

    test('keeps incoming and outgoing requests distinct while applying request actions', async ({
        browser,
    }) => {
        const incoming = contact(1, 'alex', 'incoming');
        const outgoing = contact(2, 'jamie', 'outgoing');
        await openContacts(
            browser,
            snapshot([incoming, outgoing]),
            async (page, api) => {
                await expect(
                    page.getByRole('tab', { name: /Requests, 1 incoming/i }),
                ).toBeVisible();
                await page.getByRole('tab', { name: /Requests, 1 incoming/i }).click();
                await expect(page.getByRole('heading', { name: /^Incoming/ })).toBeVisible();
                await expect(page.getByRole('heading', { name: /^Sent/ })).toBeVisible();
                await page.getByRole('button', { name: 'Accept', exact: true }).click();
                await expect(
                    page.getByRole('tab', { name: 'Requests', exact: true }),
                ).toBeVisible();
                await expect(
                    page.getByRole('button', { name: 'Cancel request', exact: true }),
                ).toBeVisible();
                await page.getByRole('button', { name: 'Cancel request', exact: true }).click();
                await expect(
                    page.getByRole('heading', { name: 'No requests', exact: true }),
                ).toBeVisible();
                expect(api.writes.map((write) => write.body.action)).toEqual(['accept', 'cancel']);
            },
            (write, current) => {
                const username = write.path.split('/')[1];
                const contacts = current.contacts
                    .map((item) =>
                        write.body.action === 'accept' && item.username === username
                            ? { ...item, status: 'accepted' as const }
                            : item,
                    )
                    .filter(
                        (item) => !(write.body.action === 'cancel' && item.username === username),
                    );
                return snapshot(contacts, {
                    ...current.settings,
                    revision: current.settings.revision + 1,
                });
            },
        );
    });

    test('preserves tri-state overrides, server effective direction, reset semantics, and failed saves', async ({
        browser,
    }) => {
        const denied = contact(1, 'alex', 'accepted', {
            canSend: false,
            autoDownload: true,
            effective: { canSend: false, autoDownload: false },
        });
        await openContacts(
            browser,
            snapshot([denied]),
            async (page, api) => {
                await identity(page, 'Alex', 'alex').click();
                await expect(
                    page.getByRole('heading', { name: 'Receiving from Alex' }),
                ).toBeVisible();
                await expect(
                    page.getByRole('link', { name: 'Send files', exact: true }),
                ).toHaveAttribute('href', '/u/alex');
                await choose(page, 'Can send me files', 'Allow');
                await choose(page, 'Automatic download', 'Off');
                await expect(
                    page.getByText('Currently not allowed.', { exact: true }),
                ).toBeVisible();
                await page.getByRole('button', { name: 'Use defaults', exact: true }).click();
                expect(api.writes.slice(-3).map((write) => write.body)).toEqual([
                    expect.objectContaining({
                        action: 'preferences',
                        canSend: true,
                        autoDownload: true,
                    }),
                    expect.objectContaining({
                        action: 'preferences',
                        canSend: true,
                        autoDownload: false,
                    }),
                    expect.objectContaining({
                        action: 'preferences',
                        canSend: null,
                        autoDownload: null,
                    }),
                ]);
            },
            (write, current) => {
                const value = write.body;
                return snapshot(
                    current.contacts.map((item) =>
                        item.username === 'alex'
                            ? {
                                  ...item,
                                  canSend: value.canSend as boolean | null,
                                  autoDownload: value.autoDownload as boolean | null,
                              }
                            : item,
                    ),
                    { ...current.settings, revision: current.settings.revision + 1 },
                );
            },
        );

        await openContacts(
            browser,
            snapshot([contact(2, 'jamie')]),
            async (page) => {
                await identity(page, 'Jamie', 'jamie').click();
                await choose(page, 'Can send me files', "Don't allow");
                await expect(page.getByRole('alert')).toContainText(/could(?:n't| not) confirm/i);
                await expect(
                    page.getByRole('combobox', { name: 'Can send me files' }),
                ).toContainText(/use account default/i);
            },
            () => ({ status: 500, message: 'Unavailable' }),
        );
    });

    test('updates all receiving defaults and refreshes effective values without native selects', async ({
        browser,
    }) => {
        const alex = contact(1, 'alex', 'accepted', {
            effective: { canSend: true, autoDownload: false },
        });
        await openContacts(
            browser,
            snapshot([alex]),
            async (page, api) => {
                await page.getByRole('button', { name: /^Receiving preferences/ }).click();
                await expect(page.locator('select')).toHaveCount(0);
                const policy = page.getByRole('combobox', { name: 'Who can send me files' });
                await policy.click();
                for (const option of [
                    'Anyone, including anonymous senders',
                    'Signed-in users',
                    'Friends only',
                    'Nobody, unless explicitly allowed',
                ])
                    await expect(
                        page.getByRole('option', { name: option, exact: true }),
                    ).toBeVisible();
                await page.keyboard.press('Escape');
                for (const option of [
                    'Signed-in users',
                    'Friends only',
                    'Nobody, unless explicitly allowed',
                ]) {
                    await choose(page, 'Who can send me files', option);
                }
                await page
                    .getByRole('switch', { name: /Automatically download from friends/i })
                    .click();
                await expect(page.getByText(/auto-download on/i)).toBeVisible();
                expect(
                    api.writes.filter((write) => write.path === 'account/receiving'),
                ).toHaveLength(4);
                expect(api.writes.every((write) => write.method === 'PATCH')).toBe(true);
                await identity(page, 'Alex', 'alex').click();
                await expect(page.getByText('Currently allowed.', { exact: true })).toBeVisible();
            },
            (write, current) => {
                const settings = {
                    ...current.settings,
                    ...(write.body as Partial<Snapshot['settings']>),
                    revision: current.settings.revision + 1,
                };
                return { settings };
            },
        );
    });

    test('does not apply a stale refreshed contacts read after a newer person preference write', async ({
        browser,
    }) => {
        const initial = snapshot([contact(1, 'alex')]);
        const stale = snapshot([contact(1, 'alex')], { revision: 1 });
        let reads = 0;
        let releaseStaleRead: ((value: Snapshot) => void) | undefined;
        const staleRead = new Promise<Snapshot>((resolve) => (releaseStaleRead = resolve));
        await openContacts(
            browser,
            initial,
            async (page, api) => {
                await page.getByRole('button', { name: /^Receiving preferences/ }).click();
                await choose(page, 'Who can send me files', 'Friends only');
                await expect.poll(() => reads).toBe(2);
                await identity(page, 'Alex', 'alex').click();
                await choose(page, 'Can send me files', 'Allow');
                expect(api.writes).toHaveLength(1);
                releaseStaleRead?.(stale);
                await expect.poll(() => api.writes).toHaveLength(2);
                await expect(
                    page.getByText('Friends can send · Auto-download off', { exact: true }),
                ).toBeVisible();
                await expect(
                    page.getByRole('combobox', { name: 'Can send me files', exact: true }),
                ).toContainText(/allow/i);
            },
            (write, current) => {
                if (write.path === 'account/receiving') {
                    return {
                        settings: {
                            ...current.settings,
                            receivingPolicy: 'friends',
                            revision: 2,
                        },
                    };
                }
                return snapshot(
                    current.contacts.map((item) =>
                        item.username === 'alex' ? { ...item, canSend: true } : item,
                    ),
                    { receivingPolicy: 'friends', revision: 3 },
                );
            },
            (current) => {
                reads += 1;
                return reads === 2 ? staleRead : current;
            },
        );
    });

    test('uses contextual menus and explicit confirmations for remove, block, and unblock', async ({
        browser,
    }) => {
        const alex = contact(1, 'alex');
        await openContacts(
            browser,
            snapshot([alex]),
            async (page) => {
                await page.getByRole('button', { name: 'Alex actions', exact: true }).click();
                await page.getByRole('menuitem', { name: 'Remove friend', exact: true }).click();
                await expect(
                    page.getByText(/becoming friends again requires a new request/i),
                ).toBeVisible();
                await expect(
                    page.getByRole('button', { name: 'Cancel', exact: true }),
                ).toBeFocused();
                await page.keyboard.press('Escape');
                await expect(
                    page.getByRole('button', { name: 'Alex actions', exact: true }),
                ).toBeFocused();
                await page.getByRole('button', { name: 'Alex actions', exact: true }).click();
                await page.getByRole('menuitem', { name: 'Block account', exact: true }).click();
                await page.getByRole('button', { name: 'Block account', exact: true }).click();
                await page.getByRole('button', { name: /Blocked accounts.*1/i }).click();
                await page.getByRole('button', { name: 'Unblock', exact: true }).click();
                await expect(page.getByText(/does not recreate a friendship/i)).toBeVisible();
                await page.getByRole('button', { name: 'Unblock account', exact: true }).click();
                await expect(page.getByText(/no blocked accounts/i)).toBeVisible();
            },
            (write, current) => {
                if (write.body.action === 'block')
                    return snapshot([], current.settings, [
                        { id: 1, name: 'Alex', username: 'alex' },
                    ]);
                if (write.body.action === 'unblock') return snapshot([], current.settings);
                return current;
            },
        );
    });

    test('searches the complete friends list by name and username', async ({ browser }) => {
        const friends = Array.from({ length: 9 }, (_, index) =>
            contact(index + 1, `friend_${index}`),
        );
        friends[7] = contact(8, 'needle_user', 'accepted', { name: 'Long Needle Name' });
        await openContacts(browser, snapshot(friends), async (page) => {
            const search = page.getByRole('searchbox', { name: /Find a friend/ });
            await search.fill('Long Needle');
            await expect(page.getByText('Long Needle Name', { exact: true })).toBeVisible();
            await search.fill('needle_user');
            await expect(page.getByText('Long Needle Name', { exact: true })).toBeVisible();
            await search.fill('absent');
            const noMatches = page.getByRole('heading', {
                name: 'No matching friends',
                exact: true,
            });
            await expect(noMatches).toBeVisible();
            await noMatches.locator('..').getByRole('button', { name: 'Clear search' }).click();
            await expect(search).toHaveValue('');
        });
    });

    test('queues defaults before a per-person write', async ({ browser }) => {
        const friends = [contact(1, 'friend_0')];
        let releaseDefaults: ((result: MutationResult) => void) | undefined;
        const defaultsResponse = new Promise<MutationResult>(
            (resolve) => (releaseDefaults = resolve),
        );
        await openContacts(
            browser,
            snapshot(friends),
            async (page, api) => {
                await page.getByRole('button', { name: /^Receiving preferences/ }).click();
                await choose(page, 'Who can send me files', 'Friends only');
                await expect.poll(() => api.writes).toHaveLength(1);
                await identity(page, 'Friend_0', 'friend_0').click();
                await choose(page, 'Can send me files', 'Allow');
                expect(api.writes).toHaveLength(1);
                releaseDefaults?.({
                    settings: {
                        receivingPolicy: 'friends',
                        autoDownloadFriends: false,
                        revision: 2,
                    },
                });
                await expect.poll(() => api.writes).toHaveLength(2);
                await expect(
                    page.getByRole('combobox', { name: 'Can send me files', exact: true }),
                ).toContainText(/allow/i);
                await page.goto('/account');
                await expect(page.getByRole('heading', { name: 'Account' })).toBeVisible();
                expect(api.writes).toHaveLength(2);
            },
            (write) =>
                write.path === 'account/receiving'
                    ? defaultsResponse
                    : snapshot(
                          friends.map((item) =>
                              item.username === 'friend_0' ? { ...item, canSend: true } : item,
                          ),
                          { receivingPolicy: 'friends', revision: 3 },
                      ),
        );
    });

    test('supports keyboard controls, reduced motion, long identities, and narrow viewports without overflow', async ({
        browser,
    }) => {
        const long = contact(1, 'a_very_long_username_that_wraps', 'accepted', {
            name: 'A very long display name that must wrap without clipping contact actions',
        });
        await openContacts(browser, snapshot([long]), async (page) => {
            await page.emulateMedia({ reducedMotion: 'reduce' });
            for (const width of [320, 390, 560, 768, 1440]) {
                await page.setViewportSize({ width, height: 800 });
                expect(
                    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
                ).toBe(true);
            }
            await page.getByRole('tab', { name: /^Friends/ }).focus();
            await page.keyboard.press('ArrowRight');
            await expect(page.getByRole('tab', { name: 'Requests', exact: true })).toBeFocused();
            await page.keyboard.press('Home');
            await expect(page.getByRole('tab', { name: /^Friends/ })).toBeFocused();
            await page.keyboard.press('Enter');
            await page.getByRole('button', { name: /actions actions$/i }).focus();
            await page.keyboard.press('Enter');
            await expect(page.getByRole('menu')).toBeVisible();
            await page.keyboard.press('Escape');
            await expect(page.getByRole('button', { name: /actions actions$/i })).toBeFocused();
        });
    });

    test('keeps empty and toolbar request actions on one line at narrow and desktop widths', async ({
        browser,
    }) => {
        await openContacts(browser, snapshot(), async (page) => {
            for (const width of [320, 390, 1440]) {
                await page.setViewportSize({ width, height: 800 });
                await expectRequestActionOnOneLine(page, 'Send request form');
            }
        });

        await openContacts(browser, snapshot([contact(1, 'alex')]), async (page) => {
            await page.getByRole('button', { name: 'Add friend', exact: true }).click();
            for (const width of [320, 390, 1440]) {
                await page.setViewportSize({ width, height: 800 });
                await expectRequestActionOnOneLine(page, 'Add friend toolbar');
            }
        });
    });

    test('animates Friends, Requests, Blocked, and Back in normal motion', async ({ browser }) => {
        await openContacts(
            browser,
            snapshot([contact(1, 'alex'), contact(2, 'jamie', 'incoming')]),
            async (page) => {
                const requests = page.getByRole('tab', { name: /Requests, 1 incoming/ });
                const requestAnimation = await activateAndCaptureStageAnimation(requests);
                expect([...requestAnimation.names, ...requestAnimation.computed]).toContainEqual(
                    expect.stringMatching(/^contacts-stage-/),
                );
                await expect(requests).toHaveAttribute('aria-selected', 'true');

                const blocked = page.getByRole('button', { name: /^Blocked accounts/ });
                const blockedAnimation = await activateAndCaptureStageAnimation(blocked);
                expect([...blockedAnimation.names, ...blockedAnimation.computed]).toContainEqual(
                    expect.stringMatching(/^contacts-stage-/),
                );
                await expect(page.getByRole('button', { name: 'Back', exact: true })).toBeVisible();

                const backAnimation = await activateAndCaptureStageAnimation(
                    page.getByRole('button', { name: 'Back', exact: true }),
                );
                expect([...backAnimation.names, ...backAnimation.computed]).toContainEqual(
                    expect.stringMatching(/^contacts-stage-/),
                );
                await expect(page.getByRole('tab', { name: /^Friends/ })).toHaveAttribute(
                    'aria-selected',
                    'true',
                );
            },
        );
    });

    test('animates receiving disclosure height and makes its closing content inert', async ({
        browser,
    }) => {
        await openContacts(browser, snapshot([contact(1, 'alex')]), async (page) => {
            const trigger = page.getByRole('button', { name: /^Receiving preferences/ });
            const opened = await toggleAndCaptureDisclosureAnimation(trigger);
            expect(opened.names).toContainEqual(expect.stringMatching(/^contacts-disclosure-in/));
            expect(opened.heights).toContain('0px');
            expect(opened.heights.some((height) => height !== '' && height !== '0px')).toBe(true);
            expect(opened.state).toBe('open');
            expect(opened.ariaHidden).toBe('false');
            expect(opened.inert).toBe(false);

            const closed = await toggleAndCaptureDisclosureAnimation(trigger);
            expect(closed.names).toContainEqual(expect.stringMatching(/^contacts-disclosure-out/));
            expect(closed.heights).toContain('0px');
            expect(closed.heights.some((height) => height !== '' && height !== '0px')).toBe(true);
            expect(closed.state).toBe('closed');
            expect(closed.ariaHidden).toBe('true');
            expect(closed.inert).toBe(true);
        });
    });

    test('keeps overlapping inactive tab panels hidden, inert, and unfocusable while tabs rove', async ({
        browser,
    }) => {
        await openContacts(
            browser,
            snapshot([contact(1, 'alex'), contact(2, 'jamie', 'incoming')]),
            async (page) => {
                const friends = page.getByRole('tab', { name: /^Friends/ });
                const requests = page.getByRole('tab', { name: /Requests, 1 incoming/ });
                const friendsPanelId = await friends.getAttribute('aria-controls');
                const requestsPanelId = await requests.getAttribute('aria-controls');
                const friendsTabId = await friends.getAttribute('id');
                const requestsTabId = await requests.getAttribute('id');
                if (!friendsPanelId || !requestsPanelId || !friendsTabId || !requestsTabId)
                    throw new Error('Contacts tabs must link each tab to its panel.');
                const friendsPanel = page.locator(`#${friendsPanelId}`);
                const requestsPanel = page.locator(`#${requestsPanelId}`);
                await expect(friendsPanel).toHaveAttribute('role', 'tabpanel');
                await expect(requestsPanel).toHaveAttribute('role', 'tabpanel');
                await expect(friendsPanel).toHaveAttribute('aria-labelledby', friendsTabId);
                await expect(requestsPanel).toHaveAttribute('aria-labelledby', requestsTabId);

                await friends.focus();
                await page.keyboard.press('ArrowRight');
                await expect(requests).toBeFocused();
                await expect(requests).toHaveAttribute('aria-selected', 'true');
                await page.evaluate(
                    () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())),
                );
                await expect(friendsPanel).toHaveAttribute('aria-hidden', 'true');
                await expect(friendsPanel).toHaveAttribute('inert', '');
                const focused = await friendsPanel.evaluate((panel) => {
                    const controls = panel.querySelectorAll<HTMLElement>(
                        'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
                    );
                    return [...controls].map((control) => {
                        control.focus();
                        return document.activeElement === control;
                    });
                });
                expect(focused).not.toContain(true);
                await expect(requestsPanel).not.toHaveAttribute('aria-hidden', 'true');
                await page.keyboard.press('Home');
                await expect(friends).toBeFocused();
            },
        );
    });

    test('removes spatial stage animation under reduced motion without changing navigation', async ({
        browser,
    }) => {
        await openContacts(
            browser,
            snapshot([contact(1, 'alex'), contact(2, 'jamie', 'incoming')]),
            async (page) => {
                await page.emulateMedia({ reducedMotion: 'reduce' });
                const requests = page.getByRole('tab', { name: /Requests, 1 incoming/ });
                const requestAnimation = await activateAndCaptureStageAnimation(requests);
                expect(requestAnimation.names).toHaveLength(0);
                expect(requestAnimation.computed).not.toContain('contacts-stage-in');
                expect(requestAnimation.computed).not.toContain('contacts-stage-out');
                await expect(page.getByRole('heading', { name: /^Incoming/ })).toBeVisible();

                const blockedAnimation = await activateAndCaptureStageAnimation(
                    page.getByRole('button', { name: /^Blocked accounts/ }),
                );
                expect(blockedAnimation.names).toHaveLength(0);
                await expect(page.getByRole('button', { name: 'Back', exact: true })).toBeVisible();

                const disclosureAnimation = await toggleAndCaptureDisclosureAnimation(
                    page.getByRole('button', { name: /^Receiving preferences/ }),
                );
                expect(disclosureAnimation.names).toHaveLength(0);
                expect(disclosureAnimation.state).toBe('open');
                expect(disclosureAnimation.inert).toBe(false);
            },
        );
    });
});
