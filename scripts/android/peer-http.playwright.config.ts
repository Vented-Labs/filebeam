import { defineConfig } from '@playwright/test';

export default defineConfig({
    testDir: '.',
    testMatch: process.env.PEER_HTTP_SPEC ?? 'peer-http.spec.ts',
    fullyParallel: false,
    timeout: 300_000,
    use: { baseURL: process.env.BASE_URL ?? 'http://127.0.0.1:8019' },
    reporter: 'list',
});
