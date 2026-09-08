import { fileURLToPath } from 'node:url';
import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vite';

export default defineConfig({
    root: fileURLToPath(new URL('.', import.meta.url)),
    publicDir: fileURLToPath(new URL('../../backend/public', import.meta.url)),
    plugins: [vue()],
    server: {
        host: '127.0.0.1',
        port: 5174,
        strictPort: true,
        fs: { allow: [fileURLToPath(new URL('../..', import.meta.url))] },
    },
});
