import { expect, test, type Page } from '@playwright/test';
import type { DesktopReleaseResult } from '../../ui/src/types';

const endpoint = '**/api/v1/desktop/releases';
const result: DesktopReleaseResult = {
    state: 'available',
    release: {
        version: '1.2.3',
        notes_url: 'https://github.com/Vented-Labs/filebeam/releases/tag/v1.2.3',
        assets: [
            ['windows', 'x86_64', 'exe'],
            ['macos', 'x86_64', 'dmg'],
            ['macos', 'aarch64', 'dmg'],
            ['linux', 'x86_64', 'AppImage'],
            ['linux', 'aarch64', 'AppImage'],
        ].map(([os, architecture, format]) => {
            const name = `filebeam-desktop-v1.2.3-${os}-${architecture}${os === 'windows' ? '-setup' : ''}.${format}`;
            return {
                os: os as 'windows' | 'macos' | 'linux',
                architecture: architecture as 'x86_64' | 'aarch64',
                format: format as 'exe' | 'dmg' | 'AppImage',
                name,
                url: `https://github.com/Vented-Labs/filebeam/releases/download/v1.2.3/${name}`,
                size: 25_000_000,
            };
        }),
    },
};

async function desktopBrowser(page: Page): Promise<void> {
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.addInitScript(() => {
        Object.defineProperty(navigator, 'userAgentData', {
            configurable: true,
            value: { platform: 'Linux', mobile: false },
        });
    });
}

test('desktop installer loads on demand, selects native packages and retains manual choices', async ({
    page,
}) => {
    await desktopBrowser(page);
    let requests = 0;
    await page.route(endpoint, async (route) => {
        requests++;
        await route.fulfill({ json: result });
    });
    await page.goto('/');
    const trigger = page.getByRole('button', { name: 'Install Desktop App', exact: true });
    await expect(trigger).toBeEnabled();
    expect(requests).toBe(0);
    await trigger.click();
    const dialog = page.getByRole('dialog', { name: 'Install Desktop App' });
    await expect(dialog.getByRole('radio', { name: 'Linux', exact: true })).toBeChecked();
    await expect(dialog.getByRole('combobox', { name: 'Processor' })).toHaveText(
        'Choose your processor',
    );
    await expect(dialog).toContainText('Choose your processor');
    for (const asset of result.release!.assets) {
        const label = { windows: 'Windows', macos: 'macOS', linux: 'Linux' }[asset.os];
        await dialog.getByRole('radio', { name: label, exact: true }).click();
        await dialog.getByRole('combobox', { name: 'Processor' }).click();
        const processor =
            asset.os === 'macos'
                ? asset.architecture === 'aarch64'
                    ? 'Apple Silicon'
                    : 'Intel'
                : asset.architecture === 'aarch64'
                  ? 'ARM64'
                  : 'x86-64 (Intel / AMD)';
        await page.getByRole('option', { name: processor, exact: true }).click();
        await expect(dialog.getByRole('link', { name: `Download for ${label}` })).toHaveAttribute(
            'href',
            asset.url,
        );
        await expect(dialog).toContainText(`Version 1.2.3 · ${asset.format} · 23.8 MiB`);
        await expect(dialog.getByRole('heading', { name: 'Install and launch' })).toBeVisible();
    }
    await expect(dialog.getByRole('link', { name: 'Release notes' })).toHaveAttribute(
        'href',
        result.release!.notes_url,
    );
    await dialog.getByRole('button', { name: 'Done', exact: true }).click();
    await expect(trigger).toBeFocused();
    await trigger.click();
    await expect(dialog.getByRole('combobox', { name: 'Processor' })).toHaveText('ARM64');
    expect(requests).toBe(1);
    await page.keyboard.press('Escape');
    await expect(trigger).toBeFocused();
});

test('desktop dialog shows loading, retry and unpublished states without download links', async ({
    page,
}) => {
    await desktopBrowser(page);
    let respond!: () => void;
    const pending = new Promise<void>((resolve) => {
        respond = resolve;
    });
    let requests = 0;
    await page.route(endpoint, async (route) => {
        requests++;
        if (requests === 1) {
            await pending;
            await route.fulfill({ status: 503, json: { state: 'error', release: null } });
        } else {
            await route.fulfill({ json: { state: 'unavailable', release: null } });
        }
    });
    await page.goto('/');
    await page.getByRole('button', { name: 'Install Desktop App', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Install Desktop App' });
    await expect(dialog).toContainText('Checking available downloads');
    respond();
    await expect(dialog).toContainText('could not be loaded');
    await dialog.getByRole('button', { name: 'Try again' }).click();
    await expect(dialog).toContainText('have not been published yet');
    await expect(dialog.getByRole('link', { name: /Download/ })).toHaveCount(0);
});

test('missing architectures do not offer a different installer', async ({ page }) => {
    await desktopBrowser(page);
    await page.route(endpoint, (route) =>
        route.fulfill({
            json: {
                ...result,
                release: {
                    ...result.release,
                    assets: result.release!.assets.filter(
                        (asset) => asset.architecture === 'x86_64',
                    ),
                },
            },
        }),
    );
    await page.goto('/');
    await page.getByRole('button', { name: 'Install Desktop App', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Install Desktop App' });
    await dialog.getByRole('combobox', { name: 'Processor' }).click();
    await page.getByRole('option', { name: 'ARM64', exact: true }).click();
    await expect(dialog).toContainText('not available in this release');
    await expect(dialog.getByRole('link', { name: /Download/ })).toHaveCount(0);
});

test('desktop dialog preserves drafts and queued files, supports keyboard selection and resize focus', async ({
    page,
}) => {
    await desktopBrowser(page);
    await page.route(endpoint, (route) => route.fulfill({ json: result }));
    await page.goto('/');
    await page.locator('#filebeam-picker').setInputFiles({
        name: 'keep.txt',
        mimeType: 'text/plain',
        buffer: Buffer.from('keep queued'),
    });
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    const editor = page.locator('.cm-content[contenteditable="true"]');
    await editor.fill('Keep this private draft');
    const trigger = page.getByRole('button', { name: 'Install Desktop App', exact: true });
    await trigger.focus();
    await page.keyboard.press('Enter');
    const dialog = page.getByRole('dialog', { name: 'Install Desktop App' });
    await expect(dialog.getByRole('heading', { name: 'Install Desktop App' })).toBeFocused();
    const linux = dialog.getByRole('radio', { name: 'Linux', exact: true });
    await linux.focus();
    await linux.press('ArrowRight');
    await expect(dialog.getByRole('radio', { name: 'macOS', exact: true })).toBeChecked();
    await page.keyboard.press('Tab');
    expect(await dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);
    await page.setViewportSize({ width: 375, height: 812 });
    await expect(trigger).toBeHidden();
    const box = (await dialog.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(375);
    await page.keyboard.press('Escape');
    await expect(page.getByRole('button', { name: 'Open navigation' })).toBeFocused();
    await expect(editor).toHaveText('Keep this private draft');
    await page.getByRole('tab', { name: 'Files', exact: true }).click();
    await expect(page.getByText('keep.txt', { exact: true })).toBeVisible();
});

for (const [os, cpu, processor] of [
    ['Linux', 'x86', 'x86-64 (Intel / AMD)'],
    ['macOS', 'arm', 'Apple Silicon'],
    ['macOS', 'x86', 'Intel'],
    ['Windows', 'x86', 'x86-64 (Intel / AMD)'],
    ['Windows', 'arm', 'ARM64'],
]) {
    test(`opening detects ${os} ${cpu} without overriding later choices`, async ({ page }) => {
        await desktopBrowser(page);
        await page.addInitScript(
            ({ os, cpu }) => {
                Object.defineProperty(navigator, 'userAgentData', {
                    configurable: true,
                    value: {
                        platform: os,
                        mobile: false,
                        getHighEntropyValues: async () => ({ architecture: cpu, bitness: '64' }),
                    },
                });
            },
            { os, cpu },
        );
        await page.route(endpoint, (route) => route.fulfill({ json: result }));
        await page.goto('/');
        const trigger = page.getByRole('button', { name: 'Install Desktop App', exact: true });
        await trigger.click();
        const dialog = page.getByRole('dialog', { name: 'Install Desktop App' });
        await expect(dialog.getByRole('radio', { name: os, exact: true })).toBeChecked();
        await expect(dialog.getByRole('combobox', { name: 'Processor' })).toHaveText(processor);
        if (os === 'Windows' && cpu === 'arm') {
            await expect(dialog).toContainText('not available in this release');
            await expect(dialog.getByRole('link', { name: /Download/ })).toHaveCount(0);
        }
        await dialog.getByRole('combobox', { name: 'Processor' }).click();
        const override =
            cpu === 'arm'
                ? os === 'macOS'
                    ? 'Intel'
                    : 'x86-64 (Intel / AMD)'
                : os === 'macOS'
                  ? 'Apple Silicon'
                  : 'ARM64';
        await page.getByRole('option', { name: override, exact: true }).click();
        await dialog.getByRole('button', { name: 'Done', exact: true }).click();
        await trigger.click();
        await expect(dialog.getByRole('combobox', { name: 'Processor' })).toHaveText(override);
    });
}

test('MacIntel without architecture hints does not guess an Intel installer', async ({ page }) => {
    await desktopBrowser(page);
    await page.addInitScript(() => {
        Object.defineProperty(navigator, 'userAgentData', { value: undefined, configurable: true });
        Object.defineProperty(navigator, 'platform', { value: 'MacIntel', configurable: true });
        Object.defineProperty(navigator, 'userAgent', {
            value: 'Macintosh; Intel Mac OS X',
            configurable: true,
        });
    });
    await page.route(endpoint, (route) => route.fulfill({ json: result }));
    await page.goto('/');
    await page.getByRole('button', { name: 'Install Desktop App', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Install Desktop App' });
    await expect(dialog.getByRole('radio', { name: 'macOS', exact: true })).toBeChecked();
    await expect(dialog.getByRole('combobox', { name: 'Processor' })).toHaveText(
        'Choose your processor',
    );
    await expect(dialog.getByRole('link', { name: /Download/ })).toHaveCount(0);
});

for (const device of ['phone', 'android tablet', 'ipad', 'narrow desktop']) {
    test(`app installation stays hidden on ${device}`, async ({ page }) => {
        await page.setViewportSize({
            width: device === 'narrow desktop' ? 900 : 1200,
            height: 900,
        });
        await page.addInitScript((device) => {
            Object.defineProperty(navigator, 'userAgentData', {
                value: undefined,
                configurable: true,
            });
            Object.defineProperty(navigator, 'userAgent', {
                configurable: true,
                value:
                    device === 'phone'
                        ? 'iPhone Mobile'
                        : device === 'android tablet'
                          ? 'Android'
                          : 'Macintosh',
            });
            Object.defineProperty(navigator, 'platform', {
                configurable: true,
                value: device === 'ipad' ? 'MacIntel' : 'Linux',
            });
            Object.defineProperty(navigator, 'maxTouchPoints', {
                configurable: true,
                value: device === 'ipad' ? 5 : 0,
            });
        }, device);
        let requests = 0;
        await page.route(endpoint, (route) => {
            requests++;
            return route.fulfill({ json: result });
        });
        await page.goto('/');
        await expect(
            page.getByRole('button', { name: 'Install Desktop App', exact: true }),
        ).toBeHidden();
        await expect(page.getByText('Install Mobile App', { exact: true })).toHaveCount(0);
        expect(requests).toBe(0);
    });
}
