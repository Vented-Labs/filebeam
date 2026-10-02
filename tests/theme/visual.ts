import { expect, type Locator, type Page } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';

/** THEME_CAPTURE=1 retains review images before intentionally updating baselines. */
export function visual(subject: Page | Locator) {
    return {
        async toHaveScreenshot(
            name: string,
            options: { animations?: 'disabled'; mask?: Locator[] } = {},
        ) {
            if (process.env.THEME_CAPTURE === '1') {
                const directory = resolve('.filebeam/theme-visual-review');
                await mkdir(directory, { recursive: true });
                await subject.screenshot({
                    ...options,
                    animations: 'disabled',
                    path: resolve(directory, name),
                });
            }
            await expect
                .soft(subject)
                .toHaveScreenshot(name, { animations: 'disabled', ...options });
        },
    };
}
