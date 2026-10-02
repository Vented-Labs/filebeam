import { readFile, readdir } from 'node:fs/promises';
import { extname, join, relative } from 'node:path';
import { parse } from '@vue/compiler-sfc';

const root = new URL('../', import.meta.url).pathname;
const roots = ['ui/src', 'backend/resources/js', 'backend/resources/css'];
const literal = /#[0-9a-f]{3,8}\b|\b(?:rgba?|hsla?|oklch)\(/i;
const utility = /\b(?:bg|text|border|ring|shadow|from|via|to)-(?:violet|purple|fuchsia|indigo|slate|gray|zinc|neutral|white|black|red|green|amber|blue|yellow|emerald|rose|sky)(?:-\d+)?\b/;
const defined = new Set();
const used = new Map();
const errors = [];

function tokens(source, path) {
    for (const match of source.matchAll(/(--fb-[a-z0-9-]+)['"]?\s*:/g)) defined.add(match[1]);
    for (const match of source.matchAll(/var\((--fb-[a-z0-9-]+)/g)) used.set(match[1], path);
}

async function scan(directory) {
    for (const entry of await readdir(join(root, directory), { withFileTypes: true })) {
        const path = join(directory, entry.name);
        if (entry.isDirectory()) {
            if (!['node_modules', 'actions', 'routes', 'wayfinder'].includes(entry.name)) await scan(path);
            continue;
        }
        if (!['.css', '.vue', '.ts'].includes(extname(path))) continue;
        const source = await readFile(join(root, path), 'utf8');
        tokens(source, path);
        const component = path.endsWith('.vue') ? parse(source, { filename: path }).descriptor : null;
        const styles = component ? component.styles.map((style) => style.content).join('\n') : path.endsWith('.css') ? source : '';
        if (literal.test(styles)) errors.push(`${path}: use a --fb-* token for CSS colors`);
        if (component?.template && utility.test(component.template.content)) errors.push(`${path}: use theme tokens instead of fixed color utilities`);
    }
}

tokens(await readFile(new URL('../backend/resources/themes/default.css', import.meta.url), 'utf8'), 'default.css');
for (const directory of roots) await scan(directory);
for (const [name, path] of used) if (!defined.has(name)) errors.push(`${relative(root, join(root, path))}: undefined token ${name}`);
if (errors.length) throw new Error(errors.join('\n'));
console.log('Theme color and token contracts passed.');
