import { createHash } from 'node:crypto';
import { appendFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium, expect, test } from '@playwright/test';

const results = process.env.PEER_RESULTS!;
const cases = [
    {
        name: 'small',
        link: 'http://127.0.0.1:8019/01M2JTHACDQWEMMQJJNKD01K1H#k=v1.Ap0Vt3T9n75icSbjcXFWI_kEwIJPfOrEAzpr2rgUHPY',
        digest: 'fbbab289f7f94b25736c58be46a994c441fd02552cc6022352e3d86d2fab7c83',
    },
    {
        name: '513mib',
        link: 'http://127.0.0.1:8019/01M2JTKH1XTWXD08P1R54R7H8A#k=v1.621rmXfbIkBKafikOxyXTFcH0EkwwGadB3oB9AOKeLA',
        digest: '099be0a986c59c84e1f003edab074a30969079f00c8cf557a023b82074e4e8e1',
    },
];

test.setTimeout(7_200_000);

for (const item of cases) {
    test(`browser OPFS verifies Android ${item.name} upload`, async () => {
        const context = await chromium.launchPersistentContext(
            join(results, 'android-cross', 'opfs'),
            {
                headless: true,
            },
        );
        const page = await context.newPage();
        const hash = createHash('sha256');
        await page.exposeBinding('__androidPeerWrite', async (_source, bytes: Buffer) => {
            hash.update(Buffer.from(bytes));
        });
        await page.addInitScript(() => {
            Object.defineProperty(window, 'showSaveFilePicker', {
                configurable: true,
                value: async () => {
                    const root = await navigator.storage.getDirectory();
                    const name = `android-peer-${crypto.randomUUID()}`;
                    const file = await root.getFileHandle(name, { create: true });
                    (window as any).__androidPeerRemove = () => root.removeEntry(name);
                    return {
                        createWritable: async () => {
                            const writable = await file.createWritable();
                            return {
                                write: async (bytes: Uint8Array) => {
                                    await (window as any).__androidPeerWrite(bytes);
                                    await writable.write(bytes);
                                },
                                close: async () => writable.close(),
                                abort: async () => writable.abort(),
                            };
                        },
                    };
                },
            });
        });
        try {
            await page.goto(item.link);
            await page.getByRole('button', { name: 'Download files', exact: true }).click();
            await expect(page.getByRole('heading', { name: '1 file downloaded' })).toBeVisible({
                timeout: 7_000_000,
            });
            expect(hash.digest('hex')).toBe(item.digest);
            await page.evaluate(async () => await (window as any).__androidPeerRemove());
            await appendFile(
                join(results, 'android-cross', 'browser-hashes.tsv'),
                `${item.name}\t${item.digest}\n`,
            );
        } finally {
            await context.close();
        }
    });
}

test('stages a browser upload for the persistent Android download runner', async ({ page }) => {
    test.setTimeout(90_000);
    await page.goto(process.env.BASE_URL!);
    await page
        .locator('#filebeam-picker')
        .setInputFiles(join(results, 'fixtures', 'web-to-cli.txt'));
    const creation = page.waitForResponse(
        (response) =>
            response.request().method() === 'POST' &&
            new URL(response.url()).pathname === '/api/v1/transfers',
        { timeout: 60_000 },
    );
    await page.getByRole('button', { name: /Encrypt and share|Send encrypted/ }).click();
    expect((await creation).ok()).toBe(true);
    await expect(page.locator('#share-link')).toBeVisible({ timeout: 60_000 });
    await writeFile(
        join(results, 'android-cross', 'browser-upload-link.txt'),
        `${await page.locator('#share-link').inputValue()}\n`,
    );
});
