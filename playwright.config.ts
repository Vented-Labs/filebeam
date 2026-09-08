import { defineConfig } from '@playwright/test';

export default defineConfig({
    testDir: './tests/browser',
    fullyParallel: false,
    timeout: 150_000,
    use: {
        baseURL: process.env.BASE_URL ?? 'http://localhost:8000',
        trace: 'retain-on-failure',
        screenshot: 'only-on-failure',
    },
    reporter: 'list',
});
