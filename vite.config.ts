import { defineConfig } from 'vite-plus';

const generatedPaths = [
    '**/node_modules/**',
    '**/vendor/**',
    '**/public/**',
    '**/bootstrap/ssr/**',
    '**/resources/js/actions/**',
    '**/resources/js/routes/**',
    '**/resources/js/wayfinder/**',
    '**/.agents/**',
    '**/.junie/**',
    '**/AGENTS.md',
    '**/boost.json',
    '**/opencode.json',
];

export default defineConfig({
    fmt: {
        ignorePatterns: generatedPaths,
        printWidth: 100,
        tabWidth: 4,
        singleQuote: true,
        semi: true,
        htmlWhitespaceSensitivity: 'css',
    },
    lint: {
        ignorePatterns: generatedPaths,
        options: { denyWarnings: true },
    },
});
