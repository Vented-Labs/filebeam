import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';

const root = new URL('../', import.meta.url);
const drawingGroup =
    '<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">';

// Deliberately support only the drawing contract used by the reviewed custom assets.
export function customPaths(svg) {
    const paths = [];
    const opacity = [1];
    const inner = svg
        .replace(/^<svg\b[^>]*>/, '')
        .replace(/<\/svg>\s*$/, '')
        .trim();
    let cursor = 0;
    for (const match of inner.matchAll(/<g\b[^>]*>|<\/g>|<path d="([^"]+)"\/>/g)) {
        if (inner.slice(cursor, match.index).trim())
            throw new Error('Unsupported custom SVG content.');
        cursor = match.index + match[0].length;
        if (match[0] === drawingGroup) opacity.push(opacity.at(-1));
        else if (match[0] === '<g opacity="0.4">') opacity.push(opacity.at(-1) * 0.4);
        else if (match[0] === '</g>') {
            if (opacity.length === 1) throw new Error('Unbalanced custom SVG group.');
            opacity.pop();
        } else if (match[1]) {
            if (opacity.length < 2 || !/^[MLHVCZ0-9.\s-]+$/.test(match[1]))
                throw new Error('Unsupported custom SVG path.');
            paths.push({ d: match[1], opacity: opacity.at(-1) });
        } else throw new Error('Unsupported custom SVG group.');
    }
    if (
        inner.slice(cursor).trim() ||
        opacity.length !== 1 ||
        !paths.length ||
        !inner.startsWith(drawingGroup) ||
        !paths.some((path) => path.opacity === 0.4)
    )
        throw new Error('Invalid custom SVG drawing contract.');
    return paths;
}

export async function loadCustomIcons(manifest) {
    return Promise.all(
        Object.entries(manifest.customIcons ?? {}).map(async ([name, entry]) => {
            if (
                !/^[a-z][a-z0-9-]*$/.test(name) ||
                entry.path !== `icons/custom/${name}.svg` ||
                !entry.description ||
                manifest.icons[name] ||
                manifest.brandIcons?.[name]
            )
                throw new Error(`Invalid custom icon entry: ${name}.`);
            const svg = await readFile(new URL(entry.path, root), 'utf8');
            if (createHash('sha256').update(svg).digest('hex') !== entry.sha256)
                throw new Error(`Custom icon hash mismatch: ${name}.`);
            if (
                !svg.startsWith(
                    '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" aria-hidden="true" focusable="false">',
                )
            )
                throw new Error(`Invalid custom SVG root: ${name}.`);
            const paths = customPaths(svg);
            const markup = svg
                .replace(/^<svg\b[^>]*>/, '')
                .replace(/<\/svg>\s*$/, '')
                .replace(/\s+/g, ' ')
                .trim();
            return { name, markup, paths, description: entry.description };
        }),
    );
}
