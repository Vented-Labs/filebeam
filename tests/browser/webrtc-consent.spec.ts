import { expect, test, type Page } from '@playwright/test';

const enabled = process.env.FILEBEAM_WEBRTC_TESTS === '1';

test.use({ baseURL: process.env.FILEBEAM_WEBRTC_BASE_URL ?? 'http://localhost:8017' });

test.describe('WebRTC consent', () => {
    test.skip(
        !enabled,
        'Set FILEBEAM_WEBRTC_TESTS=1 against the isolated WebRTC harness on :8017.',
    );

    async function selectWebRtc(page: Page): Promise<void> {
        await page.getByRole('radio', { name: 'WebRTC (live)' }).click();
    }

    test('selection changes limits without opening consent, and submit requests it before peers', async ({
        page,
        context,
    }) => {
        await page.addInitScript(() => {
            const NativePeerConnection = window.RTCPeerConnection;
            Object.assign(window, { __filebeamPeerCount: 0 });
            Object.defineProperty(window, 'RTCPeerConnection', {
                configurable: true,
                value: function (...args: ConstructorParameters<typeof RTCPeerConnection>) {
                    (window as Window & { __filebeamPeerCount: number }).__filebeamPeerCount++;
                    return new NativePeerConnection(...args);
                },
            });
        });
        await page.goto('/');
        await selectWebRtc(page);
        await expect(page.getByRole('heading', { name: 'WebRTC privacy' })).not.toBeVisible();
        await expect(page.getByText('Unlimited transfer size')).toBeVisible();
        expect(
            await page.evaluate(
                () => (window as Window & { __filebeamPeerCount: number }).__filebeamPeerCount,
            ),
        ).toBe(0);
        await page.getByRole('radio', { name: 'HTTP (stored)' }).click();
        await expect(page.getByText('2.0 MiB per transfer')).toBeVisible();
        await selectWebRtc(page);
        await page.locator('#filebeam-picker').setInputFiles({
            name: 'blocked.txt',
            mimeType: 'text/plain',
            buffer: Buffer.from('blocked'),
        });
        const transferPosts: string[] = [];
        page.on('request', (request) => {
            if (request.method() === 'POST' && request.url().endsWith('/api/v1/transfers'))
                transferPosts.push(request.url());
        });
        await page.getByRole('button', { name: 'Encrypt and share' }).click();
        await expect(page.getByRole('heading', { name: 'WebRTC privacy' })).toBeVisible();
        expect(transferPosts).toEqual([]);

        await page.getByRole('button', { name: 'Not now' }).click();
        await expect(page.getByRole('heading', { name: 'WebRTC privacy' })).not.toBeVisible();
        expect(
            (await context.cookies()).some((cookie) => cookie.name === 'webRTCRiskAccepted'),
        ).toBe(false);
        expect(transferPosts).toEqual([]);
        expect(
            await page.evaluate(
                () => (window as Window & { __filebeamPeerCount: number }).__filebeamPeerCount,
            ),
        ).toBe(0);

        await page.getByRole('button', { name: 'Encrypt and share' }).click();
        await page.getByRole('button', { name: 'Accept and continue' }).click();
        await expect(page.getByRole('heading', { name: 'WebRTC privacy' })).not.toBeVisible();
        expect(
            (await context.cookies()).find((cookie) => cookie.name === 'webRTCRiskAccepted')?.value,
        ).toBe('1');
    });

    test('does not repeat after accepting, but prompts in a fresh context', async ({
        browser,
        page,
    }) => {
        await page.goto('/');
        await selectWebRtc(page);
        await page.locator('#filebeam-picker').setInputFiles({
            name: 'accepted.txt',
            mimeType: 'text/plain',
            buffer: Buffer.from('accepted'),
        });
        await page.getByRole('button', { name: 'Encrypt and share' }).click();
        await page.getByRole('button', { name: 'Accept and continue' }).click();
        await page.reload();
        await selectWebRtc(page);
        await page.locator('#filebeam-picker').setInputFiles({
            name: 'again.txt',
            mimeType: 'text/plain',
            buffer: Buffer.from('again'),
        });
        await page.getByRole('button', { name: 'Encrypt and share' }).click();
        await expect(page.getByRole('heading', { name: 'WebRTC privacy' })).not.toBeVisible();

        const fresh = await browser.newContext();
        try {
            const freshPage = await fresh.newPage();
            await freshPage.goto('/');
            await selectWebRtc(freshPage);
            await freshPage.locator('#filebeam-picker').setInputFiles({
                name: 'fresh.txt',
                mimeType: 'text/plain',
                buffer: Buffer.from('fresh'),
            });
            await freshPage.getByRole('button', { name: 'Encrypt and share' }).click();
            await expect(freshPage.getByRole('heading', { name: 'WebRTC privacy' })).toBeVisible();
            await freshPage.keyboard.press('Escape');
            await expect(
                freshPage.getByRole('heading', { name: 'WebRTC privacy' }),
            ).not.toBeVisible();
        } finally {
            await fresh.close();
        }
    });

    test('keeps the semantic method rail above the pond and side by side on narrow screens', async ({
        page,
    }) => {
        await page.setViewportSize({ width: 390, height: 844 });
        await page.goto('/');
        const http = page.getByRole('radio', { name: 'HTTP (stored)' });
        const rtc = page.getByRole('radio', { name: 'WebRTC (live)' });
        const [httpBox, rtcBox] = await Promise.all([http.boundingBox(), rtc.boundingBox()]);
        expect(httpBox?.y).toBe(rtcBox?.y);
        expect(
            await http.evaluate((control) => control.closest('[data-testid="file-pond"]') === null),
        ).toBe(true);
        await expect(page.getByTestId('prism-transport-rail')).toHaveCount(1);
        await expect(page.getByText('Or drag and drop anywhere', { exact: true })).toBeVisible();
        await expect(http).toHaveAttribute('data-state', 'checked');
        await expect(rtc).toHaveAttribute('data-state', 'unchecked');
        expect(await http.evaluate((control) => control.tagName)).not.toBe('INPUT');
    });
});
