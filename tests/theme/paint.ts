import { expect, type Locator, type Page } from '@playwright/test';

export async function paintedContrast(page: Page, target: Locator, minimum = 4.5) {
    await target.scrollIntoViewIfNeeded();
    const box = (await target.boundingBox())!;
    const foreground = await target.evaluate((node) => getComputedStyle(node).color);
    await target.evaluate((node) => node.setAttribute('data-paint-probe', ''));
    const hiding = await page.addStyleTag({
        content:
            '[data-paint-probe], [data-paint-probe] * {color:transparent!important;-webkit-text-fill-color:transparent!important;fill:transparent!important;stroke:transparent!important;text-shadow:none!important}',
    });
    try {
        await page.evaluate(
            ({ x, y }) => {
                const pixel = document.createElement('span');
                pixel.id = 'theme-pixel-probe';
                pixel.style.cssText = `position:fixed;left:${x}px;top:${y}px;width:1px;height:1px;pointer-events:none;z-index:9999`;
                document.body.append(pixel);
            },
            { x: Math.floor(box.x + box.width / 2), y: Math.floor(box.y + box.height / 2) },
        );
        const png = await page.locator('#theme-pixel-probe').screenshot({ animations: 'disabled' });
        const result = await page.evaluate(
            async ({ png, foreground }) => {
                const image = new Image();
                image.src = `data:image/png;base64,${png}`;
                await image.decode();
                const canvas = document.createElement('canvas');
                canvas.width = canvas.height = 1;
                const ctx = canvas.getContext('2d')!;
                ctx.drawImage(image, 0, 0);
                const background = [...ctx.getImageData(0, 0, 1, 1).data].slice(0, 3);
                ctx.fillStyle = foreground;
                ctx.fillRect(0, 0, 1, 1);
                const ink = [...ctx.getImageData(0, 0, 1, 1).data].slice(0, 3);
                const luminance = (rgb: number[]) => {
                    const linear = rgb
                        .map((value) => value / 255)
                        .map((value) =>
                            value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4,
                        );
                    return linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
                };
                const a = luminance(ink),
                    b = luminance(background);
                return {
                    foreground,
                    background,
                    ratio: (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05),
                };
            },
            { png: png.toString('base64'), foreground },
        );
        expect(result.ratio, JSON.stringify(result)).toBeGreaterThanOrEqual(minimum);
    } finally {
        await hiding.evaluate((node) => node.parentNode?.removeChild(node));
        await target.evaluate((node) => node.removeAttribute('data-paint-probe'));
        await page.locator('#theme-pixel-probe').evaluate((node) => node.remove());
    }
}
