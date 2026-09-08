import { defineConfig } from '@playwright/test';

export default defineConfig({
    testDir: '.',
    testMatch: 'card.spec.ts',
    outputDir: '../../test-results/og',
    fullyParallel: true,
    use: {
        baseURL: 'http://127.0.0.1:5174',
        viewport: { width: 1200, height: 630 },
        deviceScaleFactor: 1,
        colorScheme: 'dark',
        reducedMotion: 'reduce',
        locale: 'en-US',
        timezoneId: 'UTC',
    },
    webServer: {
        command: 'npm run preview:og',
        cwd: new URL('../..', import.meta.url).pathname,
        url: 'http://127.0.0.1:5174',
        reuseExistingServer: false,
    },
});
