import { expect, test, type Page } from '@playwright/test';
import { buildDownloadCommand } from '../../ui/src/lib/cli-commands';

type ClipboardWindow = Window & { cliCopies: string[] };

test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
        (window as ClipboardWindow).cliCopies = [];
        Object.defineProperty(navigator, 'clipboard', {
            configurable: true,
            value: {
                writeText: async (text: string) => {
                    (window as ClipboardWindow).cliCopies.push(text);
                },
            },
        });
    });
});

async function installerFixture(page: Page, installerUrl: string | null): Promise<void> {
    await page.route('**/', async (route) => {
        const response = await route.fetch();
        const html = await response.text();
        const pattern = /(<script\b[^>]*\bdata-page[^>]*>)([\s\S]*?)(<\/script>)/i;
        const match = html.match(pattern);
        if (!match) throw new Error('Missing Inertia bootstrap');
        const data = JSON.parse(match[2]);
        data.props.filebeam.cli = {
            installer_url: installerUrl,
            installer_interpreter: 'sh',
            executable: 'beam',
        };
        await route.fulfill({
            response,
            body: html.replace(pattern, () => `${match[1]}${JSON.stringify(data)}${match[3]}`),
        });
    });
}

test('unconfigured installer stays non-actionable and instructions preserve the mounted draft', async ({
    page,
}) => {
    await installerFixture(page, null);
    await page.goto('/');
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    const editor = page.locator('.cm-content[contenteditable="true"]');
    await editor.fill('Retain this private draft');
    await editor.evaluate((node) => Object.assign(window, { cliEditor: node }));
    await page.waitForLoadState('networkidle');
    const requests: string[] = [];
    page.on('request', (request) => requests.push(request.url()));
    const trigger = page
        .locator('header')
        .getByRole('button', { name: 'Install CLI', exact: true });
    await trigger.click();
    const dialog = page.getByRole('dialog', { name: 'Install CLI', exact: true });
    await expect(dialog).toContainText('Installer instructions unavailable');
    await expect(dialog.locator('textarea')).toHaveCount(0);
    await page.keyboard.press('Tab');
    expect(await dialog.evaluate((node) => node.contains(document.activeElement))).toBe(true);
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await expect(trigger).toBeFocused();
    await expect(editor).toHaveText('Retain this private draft');
    expect(
        await editor.evaluate(
            (node) => node === (window as Window & { cliEditor?: Element }).cliEditor,
        ),
    ).toBe(true);
    expect(requests).toEqual([]);
});

for (const width of [360, 390]) {
    test(`configured installer is copyable and viewport-contained at ${width}px`, async ({
        page,
    }) => {
        await page.setViewportSize({ width, height: 812 });
        await page.emulateMedia({ reducedMotion: 'reduce' });
        await installerFixture(page, 'https://releases.filebeam.test/cli/install.sh');
        await page.goto('/');
        const trigger = page
            .locator('header')
            .getByRole('button', { name: 'Install CLI', exact: true });
        await expect(trigger).toBeVisible();
        await expect(page.getByRole('button', { name: 'Open navigation' })).toBeVisible();
        await page.evaluate(async () => {
            await document.fonts.load(
                `12px ${getComputedStyle(document.documentElement).getPropertyValue('--fb-font-code')}`,
            );
        });
        await page.waitForLoadState('networkidle');
        const requests: string[] = [];
        page.on('request', (request) => requests.push(request.url()));
        await trigger.click();
        const dialog = page.getByRole('dialog', { name: 'Install CLI', exact: true });
        const command =
            "curl -fsSL 'https://releases.filebeam.test/cli/install.sh' -o beam-install.sh && sh beam-install.sh";
        await expect(dialog.getByRole('textbox', { name: 'Installer command' })).toHaveValue(
            command,
        );
        await dialog.getByRole('button', { name: 'Copy installer', exact: true }).click();
        await expect(dialog.getByRole('button', { name: 'Copy installer: copied' })).toBeVisible();
        expect(await page.evaluate(() => (window as ClipboardWindow).cliCopies)).toEqual([command]);
        const bounds = (await dialog.boundingBox())!;
        expect(bounds.x).toBeGreaterThanOrEqual(0);
        expect(bounds.x + bounds.width).toBeLessThanOrEqual(width);
        expect(bounds.height).toBeLessThanOrEqual(812);
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
            true,
        );
        await dialog.getByRole('button', { name: 'Done', exact: true }).click();
        await expect(trigger).toBeFocused();
        expect(requests).toEqual([]);
    });
}

for (const variant of ['included', 'separate', 'password'] as const) {
    test(`${variant} shared file copies its exact CLI target without consuming a download`, async ({
        page,
        request,
    }) => {
        await page.goto('/');
        await page.locator('#filebeam-picker').setInputFiles({
            name: 'cli-integration.txt',
            mimeType: 'text/plain',
            buffer: Buffer.from('Browser and CLI share the same encrypted file.'),
        });
        if (variant === 'separate')
            await page.getByRole('switch', { name: 'Include key in link' }).uncheck();
        if (variant === 'password') {
            await page.getByTestId('prism-password-trigger').click();
            await page.locator('#transfer-password').fill('CLI-file-password9!');
            await page
                .getByTestId('prism-password-popover')
                .getByRole('button', { name: 'Done' })
                .click();
        }
        const created = page.waitForResponse(
            (response) =>
                response.request().method() === 'POST' &&
                new URL(response.url()).pathname === '/api/v1/transfers',
        );
        await page.getByRole('button', { name: 'Encrypt and share' }).click();
        const { data } = await (await created).json();
        try {
            await expect(page.locator('#share-link')).toBeVisible();
            const link = await page.locator('#share-link').inputValue();
            const command = buildDownloadCommand(link);
            expect(command.includes('#')).toBe(variant !== 'separate');
            expect(command).not.toContain('CLI-file-password9!');
            const card = page.getByRole('region', { name: 'Download with CLI' });
            await expect(card.getByRole('textbox', { name: 'Download command' })).toHaveValue(
                command,
            );
            await card.getByRole('button', { name: 'Copy command', exact: true }).click();
            expect(await page.evaluate(() => (window as ClipboardWindow).cliCopies)).toEqual([
                command,
            ]);
            await page.goto(link);
            const recipientCard = page.getByRole('region', { name: 'Download with CLI' });
            await expect(
                recipientCard.getByRole('textbox', { name: 'Download command' }),
            ).toHaveValue(command);
            if (variant === 'separate')
                await expect(recipientCard).toContainText('key is separate');
            if (variant === 'password')
                await expect(recipientCard).toContainText('password only at the CLI prompt');
            await page.waitForLoadState('networkidle');
            const requests: string[] = [];
            page.on('request', (outgoing) => requests.push(outgoing.url()));
            await recipientCard.getByRole('button', { name: 'Copy command', exact: true }).click();
            await recipientCard.getByRole('button', { name: 'Install CLI', exact: true }).click();
            await expect(
                page.getByRole('dialog', { name: 'Install CLI', exact: true }),
            ).toBeVisible();
            await page.keyboard.press('Escape');
            expect(requests).toEqual([]);
            expect(await page.evaluate(() => (window as ClipboardWindow).cliCopies)).toEqual([
                command,
            ]);
        } finally {
            await request.delete(`/api/v1/transfers/${data.id}`, {
                headers: { 'X-Filebeam-Delete-Token': data.delete_token },
            });
        }
        await page.reload();
        await expect(page.getByRole('heading', { name: 'Transfer unavailable' })).toBeVisible();
        await expect(page.getByRole('textbox', { name: 'Download command' })).toHaveCount(0);
    });
}

test('CLI instructions leave a burn-on-read note unread and non-actionable', async ({
    page,
    request,
}) => {
    await page.goto('/');
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    await page
        .locator('.cm-content[contenteditable="true"]')
        .fill('A private burn-on-read fixture');
    await page.getByRole('switch', { name: 'Burn on read' }).click();
    const created = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' &&
            new URL(response.url()).pathname === '/api/v1/transfers',
    );
    await page.getByRole('button', { name: 'Encrypt and share' }).click();
    const { data } = await (await created).json();
    try {
        await expect(page.locator('#share-link')).toBeVisible();
        const link = await page.locator('#share-link').inputValue();
        await page.goto(link);
        const card = page.getByRole('region', { name: 'Download with CLI' });
        await expect(card).toContainText('Use the browser for notes and burn-on-read');
        await expect(card.getByRole('textbox')).toHaveCount(0);
        await page.evaluate(async () => {
            await document.fonts.load(
                `12px ${getComputedStyle(document.documentElement).getPropertyValue('--fb-font-code')}`,
            );
        });
        await page.waitForLoadState('networkidle');
        const requests: string[] = [];
        page.on('request', (outgoing) => requests.push(outgoing.url()));
        await card.getByRole('button', { name: 'Install CLI', exact: true }).click();
        await expect(page.getByRole('dialog', { name: 'Install CLI', exact: true })).toBeVisible();
        await page.keyboard.press('Escape');
        expect(requests).toEqual([]);
        expect((await request.get(`/api/v1/transfers/${data.id}`)).ok()).toBe(true);
    } finally {
        await request.delete(`/api/v1/transfers/${data.id}`, {
            headers: { 'X-Filebeam-Delete-Token': data.delete_token },
        });
    }
});
