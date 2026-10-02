import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vite';

const backendRequire = createRequire(new URL('../../backend/package.json', import.meta.url));
const { default: tailwindcss } = await import(backendRequire.resolve('@tailwindcss/vite'));
const paletteScript = fileURLToPath(new URL('../themes/palette.php', import.meta.url));

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
    plugins: [
        tailwindcss(),
        vue(),
        {
            name: 'filebeam-gallery-theme',
            configureServer(server) {
                server.middlewares.use((request, response, next) => {
                    const match =
                        /^\/_theme-preview\/(instance|purple|blue|teal|green|amber|orange|rose)\/(logo|logo-light|mark)\.svg$/.exec(
                            new URL(request.url, 'http://localhost').pathname,
                        );
                    if (!match) return next();
                    response.setHeader('Content-Type', 'image/svg+xml');
                    response.end(
                        execFileSync('php', [paletteScript, '--asset', match[1], match[2]], {
                            encoding: 'utf8',
                        }),
                    );
                });
            },
            transformIndexHtml() {
                const generated = JSON.parse(
                    execFileSync('php', [paletteScript, '--gallery'], { encoding: 'utf8' }),
                );
                return [
                    {
                        tag: 'style',
                        attrs: { id: 'filebeam-theme' },
                        children: generated.css,
                        injectTo: 'head',
                    },
                    {
                        tag: 'script',
                        attrs: { id: 'filebeam-appearance', type: 'application/json' },
                        children: JSON.stringify(generated.appearance),
                        injectTo: 'head',
                    },
                ];
            },
        },
    ],
    server: {
        host: '127.0.0.1',
        port: 4178,
        strictPort: true,
        fs: { allow: [fileURLToPath(new URL('../..', import.meta.url))] },
    },
});
