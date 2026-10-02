import { defineConfig } from '@playwright/test';

const browserName = process.env.THEME_BROWSER ?? 'chromium';
if (!['chromium', 'firefox', 'webkit'].includes(browserName))
    throw new Error('Unknown theme browser');

export default defineConfig({
    testDir: '.',
    testMatch: [
        'gallery.spec.ts',
        'placement.spec.ts',
        'theme.spec.ts',
        'editor-colors.spec.ts',
        'gradients.spec.ts',
    ],
    outputDir: '../../.filebeam/test-results/prism-gallery',
    snapshotPathTemplate: '{testDir}/snapshots/{testFilePath}/{arg}{ext}',
    fullyParallel: false,
    updateSnapshots: 'none',
    use: {
        browserName: browserName as 'chromium' | 'firefox' | 'webkit',
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
