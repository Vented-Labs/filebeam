import { defineConfig } from '@playwright/test';

export default defineConfig({
    testDir: './tests/browser',
    fullyParallel: false,
    timeout: 150_000,
    // A test that passes on its retry is reported as flaky instead of failing CI.
    retries: process.env.CI ? 1 : 0,
    use: {
        colorScheme: 'dark',
        baseURL: process.env.BASE_URL ?? 'http://localhost:8000',
        trace: 'retain-on-failure',
        screenshot: 'only-on-failure',
    },
    reporter: 'list',
});
