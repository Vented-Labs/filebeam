/**
 * UI-only Prism integration contract. Creation POSTs are counted and aborted,
 * never treated as real transfers.
 */
import { expect, test, type Page } from '@playwright/test';
import type { FilebeamConfig } from '../../ui/src/types';

const enabled = process.env.FILEBEAM_PRISM_TESTS === '1';
const baseURL = process.env.BASE_URL ?? 'http://127.0.0.1:8017';
if (enabled && !['127.0.0.1', 'localhost', '[::1]'].includes(new URL(baseURL).hostname)) {
    throw new Error('Prism contract tests require an explicitly isolated loopback test instance.');
}

test.use({ baseURL });
let creationAttempts: string[] = [];
let pageErrors: string[] = [];
let expectedCreationAttempts = 0;
let holdCreation = false;
let releaseCreation: (() => void) | undefined;
const http = (page: Page) => page.getByRole('radio', { name: 'HTTP (stored)', exact: true });
const rtc = (page: Page) => page.getByRole('radio', { name: 'WebRTC (live)', exact: true });
const filesTab = (page: Page) => page.getByRole('tab', { name: 'Files', exact: true });
const notesTab = (page: Page) => page.getByRole('tab', { name: 'Notes', exact: true });
const editor = (page: Page) =>
    page.locator('[data-testid="note-editor"] .cm-content[contenteditable="true"]');

async function addTinyFile(page: Page): Promise<void> {
    await page.locator('#filebeam-picker').setInputFiles({
        name: 'prism-fixture.txt',
        mimeType: 'text/plain',
        buffer: Buffer.from('Throwaway Prism fixture.\n'),
    });
}

async function setSenderPassword(page: Page, value: string): Promise<void> {
    await page.getByTestId('prism-password-trigger').click();
    const popup = page.getByTestId('prism-password-popover');
    await expect(popup).toBeVisible();
    await popup.locator('#transfer-password').fill(value);
    await popup.locator('#transfer-password').press('Escape');
    await expect(popup).toBeHidden();
}

async function expectNoHorizontalOverflow(page: Page): Promise<void> {
    expect(
        await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1),
    ).toBe(true);
}

async function reloadWithConfig(
    page: Page,
    update: (config: FilebeamConfig) => void,
): Promise<void> {
    await page.route(
        (url) => url.pathname === '/' && url.origin === new URL(baseURL).origin,
        async (route) => {
            const response = await route.fetch();
            const body = await response.text();
            const opening = '<script data-page="app" type="application/json">';
            const start = body.indexOf(opening);
            const contentStart = start + opening.length;
            const end = body.indexOf('</script>', contentStart);
            if (start < 0 || end < 0) throw new Error('Inertia page payload was not found.');
            const payload = JSON.parse(body.slice(contentStart, end)) as {
                props: { filebeam: FilebeamConfig };
            };
            update(payload.props.filebeam);
            await route.fulfill({
                response,
                body: `${body.slice(0, contentStart)}${JSON.stringify(payload)}${body.slice(end)}`,
            });
        },
    );
    await page.reload();
}

async function selectFixedFile(
    page: Page,
    files: Array<{ name: string; content: string; lastModified: number }>,
): Promise<void> {
    await page.locator('#filebeam-picker').evaluate((element, fixtures) => {
        const transfer = new DataTransfer();
        for (const fixture of fixtures) {
            transfer.items.add(
                new File([fixture.content], fixture.name, {
                    type: 'text/plain',
                    lastModified: fixture.lastModified,
                }),
            );
        }
        const input = element as HTMLInputElement;
        input.files = transfer.files;
        input.dispatchEvent(new Event('change', { bubbles: true }));
    }, files);
}

test.describe('Prism production integration contract', () => {
    test.skip(
        !enabled,
        'NOT RUN: set FILEBEAM_PRISM_TESTS=1 against the isolated both-driver profile.',
    );

    test.beforeEach(async ({ page }) => {
        creationAttempts = [];
        pageErrors = [];
        expectedCreationAttempts = 0;
        holdCreation = false;
        releaseCreation = undefined;
        page.on('pageerror', (error) => pageErrors.push(error.message));
        await page.route(
            (url) => url.pathname === '/api/v1/transfers',
            async (route) => {
                if (route.request().method() === 'POST') {
                    creationAttempts.push(route.request().url());
                    if (holdCreation)
                        await new Promise<void>((resolve) => {
                            releaseCreation = resolve;
                        });
                    await route.abort('blockedbyclient').catch(() => undefined);
                } else await route.continue();
            },
        );
        await page.addInitScript(() => {
            const state = window as typeof window & { __prismPeerAttempts: number };
            state.__prismPeerAttempts = 0;
            const Original = window.RTCPeerConnection;
            if (!Original) return;
            Object.defineProperty(window, 'RTCPeerConnection', {
                configurable: true,
                value: class extends Original {
                    constructor(...args: ConstructorParameters<typeof RTCPeerConnection>) {
                        state.__prismPeerAttempts++;
                        super(...args);
                    }
                },
            });
        });
        await page.goto('/');
        await expect(page.getByTestId('prism-composer')).toBeVisible();
        await expect(http(page)).toBeChecked();
    });

    test.afterEach(async ({ page }) => {
        releaseCreation?.();
        expect(creationAttempts, 'Unexpected transfer creation attempts').toHaveLength(
            expectedCreationAttempts,
        );
        expect(pageErrors, 'Unexpected runtime errors').toEqual([]);
        expect(
            await page.evaluate(
                () =>
                    (window as typeof window & { __prismPeerAttempts: number }).__prismPeerAttempts,
            ),
        ).toBe(0);
    });

    test('empty composer keeps copy, settings, real radios and no file burn', async ({ page }) => {
        await expect(page.getByRole('heading', { name: 'Drop your files here' })).toBeVisible();
        await expect(page.getByText('Or drag and drop anywhere', { exact: true })).toBeVisible();
        await expect(page.getByTestId('prism-settings')).toBeVisible();
        await expect(
            page.getByRole('button', { name: 'Encrypt and share', exact: true }),
        ).toBeDisabled();
        await expect(page.getByRole('switch', { name: 'Burn on read', exact: true })).toHaveCount(
            0,
        );
        await expect(page.getByTestId('prism-transport-rail')).toHaveCount(1);
        expect(
            await page
                .getByTestId('prism-transport-rail')
                .evaluate((element) => element.closest('[data-testid="file-pond"]') === null),
        ).toBe(true);
        await expect(page.locator('#filebeam-picker')).toHaveCount(1);
        await expect(page.getByTestId('prism-composer')).toHaveCSS('overflow', 'clip');
        await expect(page.locator('.transfer-options__footer')).toHaveCSS(
            'border-bottom-left-radius',
            '24px',
        );
        await expect(page.locator('.transfer-options__footer')).toHaveCSS(
            'border-bottom-right-radius',
            '24px',
        );
        const fileCountReveal = page.getByTestId('prism-file-count-reveal');
        await expect(fileCountReveal).toContainText('Up to 20 files');
        expect(
            await fileCountReveal.evaluate((element) =>
                getComputedStyle(element)
                    .transitionDuration.split(',')
                    .some((duration) => parseFloat(duration) > 0),
            ),
        ).toBe(true);
        await notesTab(page).click();
        await expect(fileCountReveal).toBeHidden();
    });

    test('rail stays mounted and above work region through note and queue changes', async ({
        page,
    }) => {
        const rail = page.getByTestId('prism-transport-rail');
        const identity = await rail.elementHandle();
        expect(identity).not.toBeNull();
        const initial = await rail.boundingBox();
        await notesTab(page).click();
        await expect(editor(page)).toBeVisible();
        await filesTab(page).click();
        await addTinyFile(page);
        await expect(page.getByTestId('prism-file-row')).toHaveCount(1);
        expect(await identity!.evaluate((element) => element.isConnected)).toBe(true);
        const current = await rail.boundingBox();
        const stage = await page.getByTestId('prism-stage').boundingBox();
        expect(current).not.toBeNull();
        expect(stage).not.toBeNull();
        expect(Math.abs(current!.y - initial!.y)).toBeLessThanOrEqual(1);
        expect(current!.y + current!.height).toBeLessThanOrEqual(stage!.y + 1);
    });

    test('keyboard removal moves focus through surviving rows and back to file selection', async ({
        page,
    }) => {
        await page.locator('#filebeam-picker').setInputFiles(
            ['first.txt', 'second.txt', 'third.txt'].map((name) => ({
                name,
                mimeType: 'text/plain',
                buffer: Buffer.from(name),
            })),
        );
        const second = page.getByRole('button', { name: 'Remove second.txt' });
        await second.focus();
        await second.press('Enter');
        await expect(page.getByRole('button', { name: 'Remove third.txt' })).toBeFocused();

        await page.getByRole('button', { name: 'Remove third.txt' }).press('Enter');
        await expect(page.getByRole('button', { name: 'Remove first.txt' })).toBeFocused();
        await page.getByRole('button', { name: 'Remove first.txt' }).press('Enter');
        await expect(page.getByRole('button', { name: 'Choose files' })).toBeFocused();
    });

    test('radio keyboard selection changes only policy and never asks for consent', async ({
        page,
        context,
    }) => {
        await http(page).focus();
        await http(page).press('ArrowRight');
        await expect(rtc(page)).toBeChecked();
        await expect(rtc(page)).toBeFocused();
        await expect(page.getByTestId('prism-transport-rail')).toContainText(
            'Unlimited transfer size',
        );
        await expect(page.getByRole('dialog', { name: 'WebRTC privacy' })).toHaveCount(0);
        expect(
            (await context.cookies()).some((cookie) => cookie.name === 'webRTCRiskAccepted'),
        ).toBe(false);
        await rtc(page).press('ArrowLeft');
        await expect(http(page)).toBeChecked();
    });

    test('per-mode password, include-key and note text survive method/tab switches', async ({
        page,
    }) => {
        await addTinyFile(page);
        await setSenderPassword(page, 'file-fixture-password');
        const includeKey = page.getByRole('switch', { name: 'Include key in link', exact: true });
        await includeKey.focus();
        await includeKey.press('Space');
        await expect(includeKey).not.toBeChecked();
        const retention = page.getByRole('combobox', { name: 'Retention period', exact: true });
        await retention.click();
        await page.getByRole('option', { name: '6 hours', exact: true }).click();
        await expect(retention).toContainText('6 hours');
        await notesTab(page).click();
        await editor(page).fill('const fixture = "do not discard";');
        await setSenderPassword(page, 'note-fixture-password');
        await retention.click();
        await page.getByRole('option', { name: '12 hours', exact: true }).click();
        await expect(retention).toContainText('12 hours');
        const language = page.getByRole('combobox', { name: 'Note language', exact: true });
        await language.click();
        await page.getByRole('option', { name: 'PHP', exact: true }).click();
        await expect(
            page.getByRole('switch', { name: 'Include key in link', exact: true }),
        ).toBeChecked();
        await rtc(page).click();
        await filesTab(page).click();
        await expect(page.getByTestId('prism-file-row')).toHaveCount(1);
        await expect(
            page.getByRole('switch', { name: 'Include key in link', exact: true }),
        ).not.toBeChecked();
        await expect(retention).toContainText('6 hours');
        await page.getByTestId('prism-password-trigger').click();
        await expect(
            page.getByTestId('prism-password-popover').locator('#transfer-password'),
        ).toHaveValue('file-fixture-password');
        await page.keyboard.press('Escape');
        await notesTab(page).click();
        await expect(editor(page)).toContainText('do not discard');
        await expect(retention).toContainText('12 hours');
        await expect(language).toContainText('PHP');
        await page.getByTestId('prism-password-trigger').click();
        await expect(
            page.getByTestId('prism-password-popover').locator('#transfer-password'),
        ).toHaveValue('note-fixture-password');
        await page.keyboard.press('Escape');
    });

    test('note burn remains available in both drivers and lifetime copy is contextual', async ({
        page,
    }) => {
        await notesTab(page).click();
        const burn = page.getByRole('switch', { name: 'Burn on read', exact: true });
        await expect(burn).toBeVisible();
        await burn.click();
        await expect(page.getByText('Retained for', { exact: true })).toBeVisible();
        await rtc(page).click();
        await expect(burn).toBeVisible();
        await expect(burn).toBeChecked();
        await expect(page.getByText('Link lifetime', { exact: true })).toBeVisible();
        await expect(
            page.getByText('The server may shorten this lifetime.', { exact: true }),
        ).toBeVisible();
        await http(page).click();
        await expect(burn).toBeChecked();
    });

    test('over-cap HTTP preserves a live-valid queue and offers only selection recovery', async ({
        page,
    }) => {
        await rtc(page).click();
        await page.locator('#filebeam-picker').setInputFiles({
            name: 'actual-3mib-fixture.bin',
            mimeType: 'application/octet-stream',
            buffer: Buffer.alloc(3 * 1024 * 1024, 7),
        });
        await expect(page.getByTestId('prism-file-row')).toHaveCount(1);
        await http(page).click();
        await expect(page.getByTestId('prism-file-row')).toHaveCount(1);
        await expect(page.getByTestId('prism-policy-error')).toBeVisible();
        await expect(
            page.getByRole('button', { name: 'Encrypt and share', exact: true }),
        ).toBeDisabled();
        await page.getByRole('button', { name: 'Use WebRTC', exact: true }).click();
        await expect(rtc(page)).toBeChecked();
        await expect(page.getByTestId('prism-file-row')).toHaveCount(1);
        await expect(page.getByRole('dialog', { name: 'WebRTC privacy' })).toHaveCount(0);
    });

    test('finite HTTP and WebRTC limits include ciphertext overhead at the boundary', async ({
        page,
    }) => {
        await reloadWithConfig(page, (config) => {
            config.transport_policy!.limits.http.maximum_transfer_bytes = 64;
            config.transport_policy!.limits.webrtc.maximum_transfer_bytes = 80;
            config.transport_policy!.limits.webrtc.maximum_file_count = 2;
        });
        const picker = page.locator('#filebeam-picker');
        const submit = page.getByRole('button', { name: 'Encrypt and share', exact: true });
        await picker.setInputFiles({
            name: 'http-boundary.bin',
            mimeType: 'application/octet-stream',
            buffer: Buffer.alloc(48),
        });
        await expect(submit).toBeEnabled();
        await page.getByRole('button', { name: 'Remove http-boundary.bin' }).click();
        await picker.setInputFiles({
            name: 'http-over.bin',
            mimeType: 'application/octet-stream',
            buffer: Buffer.alloc(49),
        });
        await expect(page.getByTestId('prism-policy-error')).toBeVisible();
        await expect(submit).toBeDisabled();
        await page.getByRole('button', { name: 'Remove http-over.bin' }).click();

        await rtc(page).click();
        await expect(page.getByTestId('prism-transport-rail')).toContainText('80 B per transfer');
        await expect(page.getByTestId('prism-transport-rail')).toContainText('Up to 2 files');
        await picker.setInputFiles({
            name: 'live-boundary.bin',
            mimeType: 'application/octet-stream',
            buffer: Buffer.alloc(64),
        });
        await expect(submit).toBeEnabled();
        await page.getByRole('button', { name: 'Remove live-boundary.bin' }).click();
        await picker.setInputFiles({
            name: 'live-over.bin',
            mimeType: 'application/octet-stream',
            buffer: Buffer.alloc(65),
        });
        await expect(page.getByTestId('prism-policy-error')).toBeVisible();
        await expect(submit).toBeDisabled();
    });

    test('multibyte note policy uses UTF-8 ciphertext bytes instead of character count', async ({
        page,
    }) => {
        await reloadWithConfig(page, (config) => {
            config.transport_policy!.limits.http.maximum_note_bytes = 32;
        });
        await notesTab(page).click();
        const submit = page.getByRole('button', { name: 'Encrypt and share', exact: true });
        await editor(page).fill('\u{1f512}'.repeat(4));
        await expect(submit).toBeEnabled();
        await editor(page).fill('\u{1f512}'.repeat(5));
        await expect(page.getByTestId('prism-policy-error')).toBeVisible();
        await expect(submit).toBeDisabled();
    });

    test('differing count policies cap additions without losing the existing queue', async ({
        page,
    }) => {
        await reloadWithConfig(page, (config) => {
            config.transport_policy!.limits.http.maximum_file_count = 2;
            config.transport_policy!.limits.webrtc.maximum_file_count = null;
        });
        await page.locator('#filebeam-picker').setInputFiles(
            ['one.txt', 'two.txt', 'three.txt'].map((name) => ({
                name,
                mimeType: 'text/plain',
                buffer: Buffer.from(name),
            })),
        );
        await expect(page.getByTestId('prism-file-row')).toHaveCount(2);
        await expect(page.getByText(/Only 2 more files can be added/)).toBeVisible();
        await rtc(page).click();
        await expect(page.getByTestId('prism-transport-rail')).toContainText('Unlimited files');
        await page.locator('#filebeam-picker').setInputFiles({
            name: 'three.txt',
            mimeType: 'text/plain',
            buffer: Buffer.from('three.txt'),
        });
        await expect(page.getByTestId('prism-file-row')).toHaveCount(3);
    });

    test('duplicate files are ignored, then may be removed and reselected', async ({ page }) => {
        const duplicate = { name: 'duplicate.txt', content: 'same', lastModified: 1234 };
        await selectFixedFile(page, [duplicate, duplicate]);
        await expect(page.getByTestId('prism-file-row')).toHaveCount(1);
        await expect(page.getByText('1 duplicate file was already in the queue.')).toBeVisible();
        await page.getByRole('button', { name: 'Remove duplicate.txt' }).click();
        await expect(page.getByTestId('prism-file-row')).toHaveCount(0);
        await selectFixedFile(page, [duplicate]);
        await expect(page.getByTestId('prism-file-row')).toHaveCount(1);
    });

    test('invalid selections for both drivers never offer misleading recovery', async ({
        page,
    }) => {
        await reloadWithConfig(page, (config) => {
            config.transport_policy!.limits.http.maximum_transfer_bytes = 64;
            config.transport_policy!.limits.webrtc.maximum_transfer_bytes = 64;
        });
        await page.locator('#filebeam-picker').setInputFiles({
            name: 'invalid-everywhere.bin',
            mimeType: 'application/octet-stream',
            buffer: Buffer.alloc(49),
        });
        await expect(page.getByTestId('prism-policy-error')).toBeVisible();
        await expect(page.getByRole('button', { name: 'Use WebRTC' })).toHaveCount(0);
        await rtc(page).click();
        await expect(page.getByTestId('prism-policy-error')).toBeVisible();
        await expect(page.getByRole('button', { name: 'Use WebRTC' })).toHaveCount(0);
    });

    test('HTTP-only policy renders one full-width real radio and indicator', async ({ page }) => {
        await reloadWithConfig(page, (config) => {
            config.transport_policy!.enabled_drivers = ['http'];
            config.transport_policy!.default_driver = 'http';
        });
        await expect(rtc(page)).toHaveCount(0);
        const [card, indicator] = await Promise.all([
            http(page).boundingBox(),
            page.getByTestId('prism-driver-indicator').boundingBox(),
        ]);
        expect(card).not.toBeNull();
        expect(indicator).not.toBeNull();
        expect(Math.abs(card!.x - indicator!.x)).toBeLessThanOrEqual(1);
        expect(Math.abs(card!.width - indicator!.width)).toBeLessThanOrEqual(1);
    });

    test('WebRTC-only policy renders one full-width real radio and indicator', async ({ page }) => {
        await reloadWithConfig(page, (config) => {
            config.transport_policy!.enabled_drivers = ['webrtc'];
            config.transport_policy!.default_driver = 'webrtc';
        });
        await expect(http(page)).toHaveCount(0);
        await expect(rtc(page)).toBeChecked();
        const [card, indicator] = await Promise.all([
            rtc(page).boundingBox(),
            page.getByTestId('prism-driver-indicator').boundingBox(),
        ]);
        expect(card).not.toBeNull();
        expect(indicator).not.toBeNull();
        expect(Math.abs(card!.x - indicator!.x)).toBeLessThanOrEqual(1);
        expect(Math.abs(card!.width - indicator!.width)).toBeLessThanOrEqual(1);
    });

    test('hostile file and note text stays literal and never creates markup', async ({ page }) => {
        const name = '<img src=x onerror=alert(1)>.txt';
        await page.locator('#filebeam-picker').setInputFiles({
            name,
            mimeType: 'text/plain',
            buffer: Buffer.from('literal fixture'),
        });
        await expect(page.getByText(name, { exact: true })).toBeVisible();
        await expect(page.getByTestId('prism-file-row').locator('img[src="x"]')).toHaveCount(0);
        await notesTab(page).click();
        const content = '<script>window.__prismInjected = true</script><b>literal note</b>';
        await editor(page).fill(content);
        await expect(editor(page)).toContainText(content);
        await expect(page.locator('script').filter({ hasText: '__prismInjected' })).toHaveCount(0);
        expect(await page.evaluate(() => '__prismInjected' in window)).toBe(false);
    });

    test('disabled anonymous and registration flags show the real sign-in gate', async ({
        page,
    }) => {
        await reloadWithConfig(page, (config) => {
            config.anonymous_uploads_enabled = false;
            config.registration_enabled = false;
        });
        await expect(page.getByTestId('prism-composer')).toHaveCount(0);
        await expect(page.getByRole('heading', { name: 'Sign in to share files' })).toBeVisible();
        await expect(
            page.getByRole('main').getByRole('link', { name: 'Sign in', exact: true }),
        ).toBeVisible();
        await expect(page.getByRole('link', { name: 'Register', exact: true })).toHaveCount(0);
    });

    test('declining real consent restores focus and preserves the ready selection', async ({
        page,
    }) => {
        await addTinyFile(page);
        await rtc(page).click();
        const submit = page.getByRole('button', { name: 'Encrypt and share', exact: true });
        await submit.click();
        const dialog = page.getByRole('dialog', { name: 'WebRTC privacy' });
        await expect(dialog).toBeVisible();
        await dialog.getByRole('button', { name: 'Not now', exact: true }).click();
        await expect(dialog).toBeHidden();
        await expect(submit).toBeFocused();
        await expect(page.getByTestId('prism-file-row')).toHaveCount(1);
        await expect(rtc(page)).toBeChecked();
    });

    test('retention menu uses real selection semantics and returns focus', async ({ page }) => {
        const trigger = page.getByRole('combobox', { name: 'Retention period', exact: true });
        await trigger.click();
        await expect(page.getByRole('listbox')).toBeVisible();
        await page.keyboard.press('ArrowUp');
        await expect(page.getByRole('option', { name: '12 hours', exact: true })).toBeFocused();
        await page.keyboard.press('Enter');
        await expect(page.getByRole('listbox')).toBeHidden();
        await expect(trigger).toBeFocused();
        await expect(trigger).toContainText('12 hours');
        await trigger.click();
        await page.keyboard.press('Escape');
        await expect(trigger).toBeFocused();
    });

    test('busy settings stay locked while sender cancellation remains operable', async ({
        page,
    }) => {
        holdCreation = true;
        expectedCreationAttempts = 1;
        await addTinyFile(page);
        await page.getByRole('button', { name: 'Encrypt and share', exact: true }).click();
        await expect.poll(() => creationAttempts.length).toBe(1);

        await expect(page.getByTestId('prism-settings-controls')).toHaveAttribute('inert', '');
        const cancel = page.getByRole('button', { name: 'Cancel', exact: true });
        await expect(cancel).toBeEnabled();
        await cancel.focus();
        await expect(cancel).toBeFocused();
        await cancel.click();
        releaseCreation?.();
        releaseCreation = undefined;

        await expect(cancel).toBeHidden();
        await expect(page.getByText('Upload cancelled.', { exact: false })).toBeVisible();
        await expect(page.getByTestId('prism-file-row')).toHaveCount(1);
    });

    test('rapid changes keep current pane, editor content and one set of IDs', async ({ page }) => {
        await notesTab(page).click();
        await editor(page).fill('Preserve this editor through rapid changes.');
        for (let index = 0; index < 10; index++) {
            await filesTab(page).evaluate((element: HTMLElement) => element.click());
            await notesTab(page).evaluate((element: HTMLElement) => element.click());
        }
        await expect(editor(page)).toBeVisible();
        await expect(editor(page)).toContainText('Preserve this editor');
        await expect(page.getByTestId('prism-transport-rail')).toHaveCount(1);
        await expect(page.locator('#filebeam-picker')).toHaveCount(1);
        await expect
            .poll(() =>
                page
                    .getByTestId('prism-stage')
                    .evaluate((element) => element.getBoundingClientRect().height),
            )
            .toBeGreaterThan(100);
        const duplicates = await page.evaluate(() => {
            const seen = new Set<string>();
            return Array.from(document.querySelectorAll('[id]'))
                .filter((element) => !element.closest('svg'))
                .map((element) => element.id)
                .filter((id) => seen.has(id) || !seen.add(id));
        });
        expect(duplicates).toEqual([]);
    });

    test('reduced motion preserves selection without long running UI animations', async ({
        page,
    }) => {
        await page.emulateMedia({ reducedMotion: 'reduce' });
        await rtc(page).click();
        await notesTab(page).click();
        await expect(editor(page)).toBeVisible();
        await expect(rtc(page)).toBeChecked();
        const longAnimations = await page.getByTestId('prism-composer').evaluate((element) =>
            element
                .getAnimations({ subtree: true })
                .filter((animation) => animation.playState === 'running')
                .map((animation) => Number(animation.effect?.getTiming().duration ?? 0))
                .filter((duration) => Number.isFinite(duration) && duration > 25),
        );
        expect(longAnimations).toEqual([]);
    });

    test('large text reflows long file and help copy without clipping controls', async ({
        page,
    }) => {
        // A 720 CSS-pixel viewport is the effective layout width of 1440px at 200% browser zoom.
        await page.setViewportSize({ width: 720, height: 500 });
        const name = `very-${'long-'.repeat(30)}filename.txt`;
        await page.locator('#filebeam-picker').setInputFiles({
            name,
            mimeType: 'text/plain',
            buffer: Buffer.from('large text fixture'),
        });
        await expect(page.getByRole('button', { name: `Remove ${name}` })).toBeVisible();
        await rtc(page).click();
        await expect(
            page.getByText('The server may shorten this lifetime.', { exact: true }),
        ).toBeVisible();
        await expect(
            page.getByRole('button', { name: 'Encrypt and share', exact: true }),
        ).toBeVisible();
        await expectNoHorizontalOverflow(page);
    });

    for (const width of [320, 360, 390, 768, 1024, 1440, 1920]) {
        test(`layout and controls remain usable at ${width}px`, async ({ page }, testInfo) => {
            await page.setViewportSize({
                width,
                height: width === 320 ? 800 : width < 500 ? 844 : 1000,
            });
            await expectNoHorizontalOverflow(page);
            const first = await http(page).boundingBox();
            const second = await rtc(page).boundingBox();
            expect(first).not.toBeNull();
            expect(second).not.toBeNull();
            expect(Math.abs(first!.y - second!.y)).toBeLessThanOrEqual(1);
            await addTinyFile(page);
            await expect(page.getByTestId('prism-file-row')).toHaveCount(1);
            await expect(
                page.getByRole('button', { name: 'Turbo Transfer', exact: true }),
            ).toBeVisible();
            await expectNoHorizontalOverflow(page);
            await page.evaluate(() => document.fonts.ready);
            await page.screenshot({
                path: testInfo.outputPath(`prism-queued-${width}.png`),
                fullPage: true,
                animations: 'disabled',
            });
        });
    }
});
