import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';

const enabled = process.env.FILEBEAM_WEBRTC_TESTS === '1';
type Transfer = { id: string; deleteToken: string };

let transfers: Transfer[];
let contexts: BrowserContext[];

test.describe('WebRTC live transfers', () => {
    test.skip(!enabled, 'Set FILEBEAM_WEBRTC_TESTS=1 against the isolated WebRTC harness.');

    test.beforeEach(() => {
        transfers = [];
        contexts = [];
    });

    test.afterEach(async ({ request }) => {
        await Promise.all(
            transfers.map(({ id, deleteToken }) =>
                request.delete(`/api/v1/transfers/${id}`, {
                    headers: { 'X-Filebeam-Delete-Token': deleteToken },
                }),
            ),
        );
        await Promise.all(contexts.map((context) => context.close()));
    });

    function captureTransfers(page: Page): void {
        page.on('response', async (response) => {
            if (
                !response.ok() ||
                response.request().method() !== 'POST' ||
                !response.url().endsWith('/api/v1/transfers')
            )
                return;
            const body = (await response.json()) as {
                data?: { id?: string; delete_token?: string };
            };
            if (body.data?.id && body.data.delete_token)
                transfers.push({ id: body.data.id, deleteToken: body.data.delete_token });
        });
    }

    async function newPage(browser: Browser, initScript?: () => void): Promise<Page> {
        const context = await browser.newContext();
        await context.addInitScript(() => {
            Object.defineProperty(window, 'showSaveFilePicker', {
                value: undefined,
                configurable: true,
            });
        });
        if (initScript) await context.addInitScript(initScript);
        contexts.push(context);
        return context.newPage();
    }

    async function chooseDriver(page: Page, driver: 'http' | 'webrtc'): Promise<void> {
        await page
            .getByRole('radio', { name: driver === 'http' ? 'HTTP (stored)' : 'WebRTC (live)' })
            .check({ force: true });
    }

    async function clickWithRisk(page: Page, button: ReturnType<Page['getByRole']>): Promise<void> {
        await button.click();
        const dialog = page.getByRole('dialog', { name: 'WebRTC privacy' });
        if (await dialog.isVisible().catch(() => false)) {
            await dialog.getByRole('button', { name: 'Accept and continue' }).click();
        }
    }

    async function createLiveTransfer(
        page: Page,
        files: Array<{ name: string; buffer: Buffer }>,
        options: { password?: string; note?: string; burn?: boolean } = {},
    ): Promise<{ id: string; link: string; key: string }> {
        captureTransfers(page);
        await page.goto('/');
        if (options.note !== undefined) {
            await page.getByRole('tab', { name: 'Notes' }).click();
            await page
                .locator('[data-testid="note-editor"] .cm-content[contenteditable="true"]')
                .fill(options.note);
            if (options.burn) await page.getByRole('switch', { name: 'Burn on read' }).click();
        } else {
            await page.locator('#filebeam-picker').setInputFiles(
                files.map((file) => ({
                    name: file.name,
                    mimeType: 'application/octet-stream',
                    buffer: file.buffer,
                })),
            );
        }
        if (options.password) await page.locator('#transfer-password').fill(options.password);
        await chooseDriver(page, 'webrtc');
        await clickWithRisk(page, page.getByRole('button', { name: 'Encrypt and share' }));
        await expect(
            page.getByRole('heading', { name: 'Your live transfer is ready' }),
        ).toBeVisible({
            timeout: 90_000,
        });
        await expect.poll(() => transfers.length).toBe(1);
        return {
            id: transfers[0]!.id,
            link: await page.locator('#share-link').inputValue(),
            key: await page.locator('#generated-key').inputValue(),
        };
    }

    async function unlockLive(
        page: Page,
        link: string,
        key: string,
        password?: string,
    ): Promise<void> {
        await page.goto(link);
        if (password) {
            await page.locator('#transfer-password').fill(password);
            await page.getByRole('button', { name: 'Unlock', exact: true }).click();
        }
        const keyField = page.locator('#transfer-key');
        if (await keyField.count()) {
            await keyField.fill(key);
            await page.getByRole('button', { name: 'Unlock', exact: true }).click();
        }
        await expect(
            page.getByText('WebRTC live transfer. The sender must keep their browser open.'),
        ).toBeVisible();
    }

    async function downloadFile(page: Page, name: string): Promise<Buffer> {
        const download = page.waitForEvent('download');
        const individual = page.getByRole('button', { name: `Download ${name}` });
        await clickWithRisk(
            page,
            (await individual.count())
                ? individual
                : page.getByRole('button', { name: 'Download files' }),
        );
        return readFile((await (await download).path())!);
    }

    test('keeps HTTP selected by default and only constructs peers after both sides consent', async ({
        browser,
        page,
    }) => {
        await page.addInitScript(() => {
            const Original = window.RTCPeerConnection;
            Object.assign(window, { __filebeamPeerCount: 0 });
            // Preserve the browser implementation while recording every attempted peer connection.
            Object.defineProperty(window, 'RTCPeerConnection', {
                configurable: true,
                value: function (...args: ConstructorParameters<typeof RTCPeerConnection>) {
                    (window as any).__filebeamPeerCount++;
                    return new Original(...args);
                },
            });
        });
        await page.goto('/');
        await expect(page.getByRole('radio', { name: 'HTTP (stored)' })).toBeChecked();
        expect(await page.evaluate(() => (window as any).__filebeamPeerCount)).toBe(0);

        const live = await createLiveTransfer(page, [
            { name: 'consent.txt', buffer: Buffer.from('consent') },
        ]);
        expect(await page.evaluate(() => (window as any).__filebeamPeerCount)).toBe(0);

        const recipient = await newPage(browser, () => {
            const Original = window.RTCPeerConnection;
            Object.assign(window, { __filebeamPeerCount: 0 });
            Object.defineProperty(window, 'RTCPeerConnection', {
                configurable: true,
                value: function (...args: ConstructorParameters<typeof RTCPeerConnection>) {
                    (window as any).__filebeamPeerCount++;
                    return new Original(...args);
                },
            });
        });
        await unlockLive(recipient, live.link, live.key);
        expect(await recipient.evaluate(() => (window as any).__filebeamPeerCount)).toBe(0);
        await clickWithRisk(recipient, recipient.getByRole('button', { name: 'Download files' }));
        await expect
            .poll(() => recipient.evaluate(() => (window as any).__filebeamPeerCount), {
                timeout: 45_000,
            })
            .toBeGreaterThan(0);
    });

    test('moves encrypted multichunk files peer-to-peer without HTTP payload requests or signaling leaks', async ({
        browser,
        page,
    }) => {
        const marker = 'WEBRTC_PRIVATE_MARKER';
        const password = 'live-private-password';
        const requestBodies: string[] = [];
        const httpPayloadRequests: string[] = [];
        page.on('request', (request) => {
            if (request.url().includes('/api/v1/transfers'))
                requestBodies.push(request.postData() ?? '');
            if (/\/items\/[^/]+\/chunks|\/uploads\//.test(request.url()))
                httpPayloadRequests.push(request.url());
        });
        const files = [
            {
                name: 'private-name-a.bin',
                buffer: Buffer.concat([Buffer.alloc(300_000, 3), Buffer.from(marker)]),
            },
            { name: 'private-name-b.bin', buffer: Buffer.alloc(280_000, 9) },
        ];
        const live = await createLiveTransfer(page, files, { password });
        const recipient = await newPage(browser, () => {
            Object.defineProperty(window, 'showSaveFilePicker', {
                value: undefined,
                configurable: true,
            });
        });
        recipient.on('request', (request) => {
            if (/\/items\/[^/]+\/chunks|\/uploads\//.test(request.url()))
                httpPayloadRequests.push(request.url());
        });
        await unlockLive(recipient, live.link, live.key, password);
        await expect(recipient.getByText('private-name-a.bin', { exact: true })).toBeVisible();
        expect(await downloadFile(recipient, 'private-name-a.bin')).toEqual(files[0]!.buffer);
        expect(await downloadFile(recipient, 'private-name-b.bin')).toEqual(files[1]!.buffer);
        for (const body of requestBodies) {
            expect(body).not.toContain(marker);
            expect(body).not.toContain('private-name-a.bin');
            expect(body).not.toContain(password);
        }
        expect(httpPayloadRequests).toEqual([]);
    });

    test('burn-on-read note permits one registered receiver and deletes metadata after successful decrypt', async ({
        browser,
        page,
        request,
    }) => {
        await page.addInitScript(() => {
            const send = RTCDataChannel.prototype.send;
            const queued: Array<{
                channel: RTCDataChannel;
                value: string | ArrayBufferView | ArrayBuffer | Blob;
            }> = [];
            Object.assign(window, {
                __queuedWebRtcFrames: () => queued.length,
                __releaseWebRtcFrames: () => {
                    for (const { channel, value } of queued) send.call(channel, value);
                    queued.length = 0;
                },
            });
            RTCDataChannel.prototype.send = function (value) {
                if (value instanceof Uint8Array && value.byteLength > 16) {
                    queued.push({ channel: this, value: new Uint8Array(value) });
                    return;
                }
                return send.call(this, value);
            };
        });
        const live = await createLiveTransfer(page, [], {
            note: 'WEBRTC_BURN_NOTE',
            password: 'burn-password',
            burn: true,
        });
        const first = await newPage(browser);
        await unlockLive(first, live.link, live.key, 'burn-password');
        const second = await newPage(browser);
        await unlockLive(second, live.link, live.key, 'burn-password');
        await clickWithRisk(first, first.getByRole('button', { name: 'Decrypt note' }));
        await expect
            .poll(() => page.evaluate(() => (window as any).__queuedWebRtcFrames()))
            .toBeGreaterThan(0);
        await clickWithRisk(second, second.getByRole('button', { name: 'Decrypt note' }));
        await expect(second.getByRole('alert')).toContainText(/claim|receiver|unavailable|failed/i);
        await page.evaluate(() => (window as any).__releaseWebRtcFrames());
        await expect(first.getByText('WEBRTC_BURN_NOTE')).toBeVisible({ timeout: 90_000 });
        await expect
            .poll(async () => (await request.get(`/api/v1/transfers/${live.id}`)).status())
            .toBe(404);
    });

    for (const kind of ['files', 'note'] as const) {
        test(`confirms ${kind} restart in a branded modal before creating a new stored HTTP transfer`, async ({
            page,
            request,
        }) => {
            const nativeDialogs: string[] = [];
            page.on('dialog', async (dialog) => {
                nativeDialogs.push(dialog.message());
                await dialog.dismiss();
            });
            const live = await createLiveTransfer(
                page,
                [{ name: 'restart.bin', buffer: Buffer.alloc(32_000, 7) }],
                kind === 'note' ? { note: 'A note to restart over HTTP.' } : {},
            );
            await page.getByRole('button', { name: 'Restart as stored HTTP' }).click();
            const modal = page.getByRole('dialog', { name: 'Restart as stored HTTP?' });
            await expect(modal).toBeVisible();
            await expect(modal).toHaveClass(/fb-dialog__content/);
            await expect(modal).toContainText('This creates a new link and applies HTTP limits.');
            await expect(modal).toContainText('the current live share will end');
            expect(nativeDialogs).toEqual([]);
            expect(transfers).toHaveLength(1);
            expect((await request.get(`/api/v1/transfers/${live.id}`)).status()).toBe(200);

            await modal.getByRole('button', { name: 'Restart with HTTP' }).click();
            await expect(modal).toBeHidden();
            await expect(
                page.getByRole('heading', { name: 'Your encrypted link is ready' }),
            ).toBeVisible({ timeout: 90_000 });
            await expect.poll(() => transfers.length).toBe(2);
            expect(transfers[1]!.id).not.toBe(live.id);
            expect((await request.get(`/api/v1/transfers/${live.id}`)).status()).toBe(404);
        });
    }

    test('dismissing restart confirmation preserves the live share and restores keyboard focus', async ({
        page,
        request,
    }) => {
        const live = await createLiveTransfer(page, [
            { name: 'keep-live.bin', buffer: Buffer.alloc(1024, 3) },
        ]);
        const trigger = page.getByRole('button', { name: 'Restart as stored HTTP' });
        const modal = page.getByRole('dialog', { name: 'Restart as stored HTTP?' });
        for (const dismissal of ['cancel', 'escape', 'close', 'outside']) {
            await trigger.click();
            await expect(modal).toBeVisible();
            await expect(modal.getByRole('button', { name: 'Cancel', exact: true })).toBeFocused();
            await page.keyboard.press('Shift+Tab');
            await expect(
                modal.getByRole('button', { name: 'Close restart confirmation' }),
            ).toBeFocused();
            if (dismissal === 'cancel')
                await modal.getByRole('button', { name: 'Cancel', exact: true }).click();
            else if (dismissal === 'escape') await page.keyboard.press('Escape');
            else if (dismissal === 'close')
                await modal.getByRole('button', { name: 'Close restart confirmation' }).click();
            else
                await page
                    .locator('.fb-dialog__overlay[data-state="open"]')
                    .click({ position: { x: 5, y: 5 } });
            await expect(modal).toBeHidden();
            await expect(trigger).toBeFocused();
            await expect(page.locator('#share-link')).toHaveValue(live.link);
            expect(transfers).toHaveLength(1);
            expect((await request.get(`/api/v1/transfers/${live.id}`)).status()).toBe(200);
        }
    });

    test('restart confirmation fits mobile and respects reduced motion', async ({ page }) => {
        await page.setViewportSize({ width: 375, height: 812 });
        await createLiveTransfer(page, [
            { name: 'mobile-restart.txt', buffer: Buffer.from('mobile') },
        ]);
        const trigger = page.getByRole('button', { name: 'Restart as stored HTTP' });
        const modal = page.getByRole('dialog', { name: 'Restart as stored HTTP?' });
        await trigger.click();
        await expect(modal).toBeVisible();
        await expect(modal).toHaveCSS('animation-name', 'fb-dialog-in');
        const bounds = await modal.boundingBox();
        expect(bounds!.x).toBeGreaterThanOrEqual(0);
        expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(375);
        await page.keyboard.press('Escape');
        await expect(modal).toBeHidden();
        await page.emulateMedia({ reducedMotion: 'reduce' });
        await trigger.click();
        await expect(modal).toBeVisible();
        expect(
            await modal.evaluate((element) =>
                parseFloat(getComputedStyle(element).animationDuration),
            ),
        ).toBeLessThanOrEqual(0.001);
        await modal.getByRole('button', { name: 'Cancel', exact: true }).click();
        await expect(modal).toBeHidden();
    });

    test('disables live selection without RTCPeerConnection and rejects HTTP restart over its smaller limit', async ({
        browser,
        request,
    }) => {
        const unsupported = await newPage(browser, () => {
            Object.defineProperty(window, 'RTCPeerConnection', {
                value: undefined,
                configurable: true,
            });
        });
        await unsupported.goto('/');
        await expect(unsupported.getByRole('radio', { name: 'WebRTC (live)' })).toBeDisabled();

        const sender = await newPage(browser);
        const live = await createLiveTransfer(sender, [
            { name: 'over-http-limit.bin', buffer: Buffer.alloc(2_200_000, 1) },
        ]);
        await sender.getByRole('button', { name: 'Restart as stored HTTP' }).click();
        await expect(sender.getByText(/HTTP limits/i)).toBeVisible();
        await expect(sender.getByRole('dialog', { name: 'Restart as stored HTTP?' })).toHaveCount(
            0,
        );
        expect(transfers).toHaveLength(1);
        expect(live.id).toBe(transfers[0]!.id);
        expect((await request.get(`/api/v1/transfers/${live.id}`)).status()).toBe(200);
    });

    test('serves simultaneous recipients independently and keeps the live link readable', async ({
        browser,
        page,
        request,
    }) => {
        const files = [
            { name: 'recipient-a.txt', buffer: Buffer.from('recipient-a') },
            { name: 'recipient-b.txt', buffer: Buffer.from('recipient-b') },
        ];
        const live = await createLiveTransfer(page, files);
        const first = await newPage(browser, () => {
            Object.defineProperty(window, 'showSaveFilePicker', {
                value: undefined,
                configurable: true,
            });
        });
        const second = await newPage(browser, () => {
            Object.defineProperty(window, 'showSaveFilePicker', {
                value: undefined,
                configurable: true,
            });
        });
        await Promise.all([
            unlockLive(first, live.link, live.key),
            unlockLive(second, live.link, live.key),
        ]);
        await Promise.all([
            expect(first.getByText('recipient-a.txt', { exact: true })).toBeVisible(),
            expect(second.getByText('recipient-b.txt', { exact: true })).toBeVisible(),
        ]);
        const [firstDownload, secondDownload] = await Promise.all([
            downloadFile(first, 'recipient-a.txt'),
            downloadFile(second, 'recipient-b.txt'),
        ]);
        expect(firstDownload).toEqual(files[0]!.buffer);
        expect(secondDownload).toEqual(files[1]!.buffer);
        expect(await downloadFile(first, 'recipient-a.txt')).toEqual(files[0]!.buffer);
        expect((await request.get(`/api/v1/transfers/${live.id}`)).status()).toBe(200);
    });

    test('stopping a sender closes the live session and makes a recipient link unavailable', async ({
        browser,
        page,
    }) => {
        const live = await createLiveTransfer(page, [
            { name: 'stop.txt', buffer: Buffer.from('stop') },
        ]);
        const recipient = await newPage(browser);
        await unlockLive(recipient, live.link, live.key);
        await page.getByRole('button', { name: 'Stop live transfer' }).click();
        await clickWithRisk(recipient, recipient.getByRole('button', { name: 'Download files' }));
        await expect(recipient.getByRole('alert')).toContainText(
            /unavailable|cancelled|stopped|register/i,
            {
                timeout: 45_000,
            },
        );
    });

    test('round-trips an empty live file through the Blob download fallback', async ({
        browser,
        page,
    }) => {
        const live = await createLiveTransfer(page, [
            { name: 'empty-live.bin', buffer: Buffer.alloc(0) },
        ]);
        const recipient = await newPage(browser, () => {
            Object.defineProperty(window, 'showSaveFilePicker', {
                value: undefined,
                configurable: true,
            });
        });
        await unlockLive(recipient, live.link, live.key);
        expect(await downloadFile(recipient, 'empty-live.bin')).toEqual(Buffer.alloc(0));
    });

    test('cancels a receiver midstream without saving a partial file', async ({
        browser,
        page,
    }) => {
        await page.addInitScript(() => {
            const send = RTCDataChannel.prototype.send;
            RTCDataChannel.prototype.send = function (value) {
                if (value instanceof Uint8Array && value.byteLength > 16) return;
                return send.call(this, value);
            };
        });
        const live = await createLiveTransfer(page, [
            { name: 'cancelled-live.bin', buffer: Buffer.alloc(400_000, 5) },
        ]);
        const recipient = await newPage(browser, () => {
            Object.defineProperty(window, 'showSaveFilePicker', {
                value: undefined,
                configurable: true,
            });
        });
        let saved = false;
        recipient.on('download', () => {
            saved = true;
        });
        await unlockLive(recipient, live.link, live.key);
        await clickWithRisk(recipient, recipient.getByRole('button', { name: 'Download files' }));
        await expect(recipient.getByRole('button', { name: 'Cancel' })).toBeVisible();
        await recipient.getByRole('button', { name: 'Cancel' }).click();
        await expect(recipient.getByRole('button', { name: 'Download files' })).toBeVisible();
        expect(saved).toBe(false);
    });

    test('rejects a tampered live ciphertext frame before saving any download', async ({
        browser,
        page,
    }) => {
        await page.addInitScript(() => {
            const send = RTCDataChannel.prototype.send;
            let changed = false;
            RTCDataChannel.prototype.send = function (
                value: string | ArrayBufferView | ArrayBuffer | Blob,
            ) {
                if (!changed && value instanceof Uint8Array && value.byteLength > 16) {
                    const copy = new Uint8Array(value);
                    copy[16] ^= 1;
                    changed = true;
                    return send.call(this, copy);
                }
                return send.call(this, value);
            };
        });
        const live = await createLiveTransfer(page, [
            { name: 'tampered.bin', buffer: Buffer.alloc(80_000, 4) },
        ]);
        const recipient = await newPage(browser, () => {
            Object.defineProperty(window, 'showSaveFilePicker', {
                value: undefined,
                configurable: true,
            });
        });
        await unlockLive(recipient, live.link, live.key);
        await clickWithRisk(recipient, recipient.getByRole('button', { name: 'Download files' }));
        await expect(recipient.getByRole('alert')).toContainText(/altered|failed|integrity/i, {
            timeout: 90_000,
        });
    });

    test('preserves a narrow viewport and never places the generated key in a request URL', async ({
        page,
    }, testInfo) => {
        const urls: string[] = [];
        page.on('request', (request) => urls.push(request.url()));
        await page.setViewportSize({ width: 375, height: 812 });
        const live = await createLiveTransfer(page, [
            { name: 'mobile-private.txt', buffer: Buffer.from('MOBILE_PRIVATE_MARKER') },
        ]);
        expect(
            await page.locator('body').evaluate((body) => body.scrollWidth <= window.innerWidth),
        ).toBe(true);
        expect(live.link).toContain('#k=v1.');
        expect(urls.join('\n')).not.toContain(live.key);
        await page.screenshot({ path: testInfo.outputPath('webrtc-mobile.png'), fullPage: true });
    });
});
