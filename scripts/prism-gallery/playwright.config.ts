import { defineConfig } from '@playwright/test';

export default defineConfig({
    testDir: '.',
    testMatch: 'gallery.spec.ts',
    outputDir: '../../test-results/prism-gallery',
    fullyParallel: false,
    use: {
        baseURL: 'http://127.0.0.1:4178',
        viewport: { width: 1440, height: 1000 },
        colorScheme: 'dark',
        locale: 'en-US',
        timezoneId: 'UTC',
    },
    webServer: {
        command: 'npm run dev:prism',
        cwd: new URL('../..', import.meta.url).pathname,
        url: 'http://127.0.0.1:4178',
        reuseExistingServer: false,
    },
});
