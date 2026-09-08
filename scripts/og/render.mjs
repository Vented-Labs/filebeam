import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';
import { createServer } from 'vite';

const output = new URL('../../backend/public/build/og/', import.meta.url);
const server = await createServer({
    configFile: fileURLToPath(new URL('./vite.config.mjs', import.meta.url)),
    server: { port: 0, strictPort: false },
});
let browser;

try {
    await server.listen();
    const origin = server.resolvedUrls.local[0];
    browser = await chromium.launch();
    const page = await browser.newPage({
        viewport: { width: 1200, height: 630 },
        deviceScaleFactor: 1,
        colorScheme: 'dark',
        reducedMotion: 'reduce',
        locale: 'en-US',
        timezoneId: 'UTC',
    });
    const errors = [];
    page.on('pageerror', (error) => errors.push(error.message));
    page.on('console', (message) => {
        if (message.type() === 'error') errors.push(message.text());
    });
    page.on('requestfailed', (request) => errors.push(`Request failed: ${request.url()}`));
    page.on('response', (response) => {
        if (response.status() >= 400) errors.push(`HTTP ${response.status()}: ${response.url()}`);
    });
    await page.route('**/*', (route) => {
        if (new URL(route.request().url()).origin === new URL(origin).origin) {
            return route.continue();
        }
        errors.push(`External asset forbidden: ${route.request().url()}`);
        return route.abort();
    });
    await mkdir(output, { recursive: true });
    const manifest = {};

    for (const variant of ['home', 'receive', 'transfer']) {
        await page.goto(`${origin}?variant=${variant}`, { waitUntil: 'networkidle' });
        const card = page.locator('.og-card');
        await card.waitFor();
        await page.evaluate(async () => {
            await document.fonts.ready;
            if (!document.fonts.check('750 64px "Inter Variable"')) {
                throw new Error('Inter font did not load');
            }
            await Promise.all([...document.images].map((image) => image.decode()));
            const bounds = document.querySelector('.og-card').getBoundingClientRect();
            for (const element of document.querySelectorAll('.og-copy, .og-copy h1, .og-footer')) {
                const box = element.getBoundingClientRect();
                if (
                    box.left < bounds.left ||
                    box.top < bounds.top ||
                    box.left + element.scrollWidth > bounds.right ||
                    box.top + element.scrollHeight > bounds.bottom
                ) {
                    throw new Error('Social card content is clipped');
                }
            }
            const copy = document.querySelector('.og-copy').getBoundingClientRect();
            const mark = document.querySelector('.og-mark').getBoundingClientRect();
            const footer = document.querySelector('.og-footer').getBoundingClientRect();
            if (copy.right + 32 > mark.left || copy.bottom + 32 > footer.top) {
                throw new Error('Social card content overlaps the artwork or footer');
            }
        });
        assert.deepEqual(errors, [], `Rendering ${variant} failed`);
        const png = await card.screenshot({ animations: 'disabled' });
        assert.equal(png.readUInt32BE(16), 1200);
        assert.equal(png.readUInt32BE(20), 630);
        assert.ok(png.length < 1_000_000, 'Social card must stay under 1 MB');
        const hash = createHash('sha256').update(png).digest('hex').slice(0, 16);
        const filename = `${variant}-${hash}.png`;
        await writeFile(new URL(filename, output), png);
        manifest[variant] = filename;
        console.log(`Rendered ${filename} (${Math.round(png.length / 1024)} KB)`);
    }

    await writeFile(new URL('manifest.json', output), `${JSON.stringify(manifest, null, 2)}\n`);
} finally {
    try {
        await browser?.close();
    } finally {
        await server.close();
    }
}
