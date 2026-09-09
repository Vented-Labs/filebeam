import { readdir, readFile } from 'node:fs/promises';
import { extname, join, relative } from 'node:path';
import { violations } from '../check-icons.mjs';

const root = new URL('../../', import.meta.url).pathname.replace(/\/$/, '');
const galleryRoot = join(root, 'scripts/prism-gallery');
const sourceExtensions = new Set([
    '.vue',
    '.ts',
    '.tsx',
    '.js',
    '.jsx',
    '.mjs',
    '.cjs',
    '.css',
    '.scss',
]);

async function files(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    return (
        await Promise.all(
            entries.map((entry) => {
                const path = join(directory, entry.name);
                if (entry.isDirectory()) return entry.name === 'node_modules' ? [] : files(path);
                return sourceExtensions.has(extname(entry.name)) ? [path] : [];
            }),
        )
    ).flat();
}

const errors = (
    await Promise.all(
        (await files(galleryRoot)).map(async (absolute) =>
            violations(relative(root, absolute), await readFile(absolute, 'utf8')),
        ),
    )
).flat();

if (errors.length) throw new Error(`Gallery icon policy violations:\n${errors.join('\n')}`);
