import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vite';

const backendRequire = createRequire(new URL('../../backend/package.json', import.meta.url));
const { default: tailwindcss } = await import(backendRequire.resolve('@tailwindcss/vite'));

export default defineConfig({
    root: fileURLToPath(new URL('.', import.meta.url)),
    publicDir: fileURLToPath(new URL('../../backend/public', import.meta.url)),
    resolve: {
        alias: [
            {
                find: '../../lib/branding',
                replacement: fileURLToPath(new URL('./branding.ts', import.meta.url)),
            },
            {
                find: fileURLToPath(new URL('../../ui/src/lib/branding.ts', import.meta.url)),
                replacement: fileURLToPath(new URL('./branding.ts', import.meta.url)),
            },
        ],
    },
    plugins: [tailwindcss(), vue()],
    server: {
        host: '127.0.0.1',
        port: 4178,
        strictPort: true,
        fs: { allow: [fileURLToPath(new URL('../..', import.meta.url))] },
    },
});
