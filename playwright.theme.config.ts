import { defineConfig } from '@playwright/test';

const baseURL = process.env.THEME_BASE_URL ?? 'http://127.0.0.1:8033';
if (
    !process.env.THEME_TEST_INSTANCE ||
    !['127.0.0.1', 'localhost', '[::1]'].includes(new URL(baseURL).hostname)
) {
    throw new Error('Run theme tests with npm run test:theme against its isolated application.');
}

export default defineConfig({
    testDir: './tests/theme',
    workers: 1,
    fullyParallel: false,
    timeout: 60_000,
    outputDir: '.filebeam/test-results/themes',
    reporter: 'list',
    updateSnapshots: 'none',
    snapshotPathTemplate: '{testDir}/snapshots/{testFilePath}/{arg}{ext}',
    use: {
        baseURL,
        connectOptions: process.env.THEME_BROWSER_WS
            ? { wsEndpoint: process.env.THEME_BROWSER_WS }
            : undefined,
        locale: 'en-US',
        timezoneId: 'UTC',
        deviceScaleFactor: 1,
        trace: 'retain-on-failure',
        screenshot: 'only-on-failure',
    },
});
