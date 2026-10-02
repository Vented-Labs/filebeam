import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFile, readdir } from 'node:fs/promises';
import { extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse as parseVue } from '@vue/compiler-sfc';
import * as css from 'css-tree';
import ts from 'typescript';

const root = fileURLToPath(new URL('../', import.meta.url));
const contracts = JSON.parse(execFileSync('php', [join(root, 'scripts/themes/palette.php'), '--contracts'], { encoding: 'utf8', maxBuffer: 1024 * 1024 }));
const errors = [];
// Literal definitions live only in the PHP adapters, generated fallback, mail slots,
// bundled artwork and independent fixtures. None is an application component root.
const roots = ['ui/src', 'backend/resources/js', 'backend/resources/css', 'scripts/prism-gallery'];
const runtime = {
    'ui/src/components/layout/AppearancePopover.vue': { '--fb-preset-color': 'color', '--fb-preset-ink': 'color' },
    'ui/src/components/upload/FilePond.vue': { '--fb-pond-art-scale': 'non-color' },
    'ui/src/styles.css': { '--fb-focus-safe-inset': 'non-color' },
    'ui/src/components/layout/AnimatedHeight.vue': { '--fb-focus-safe-inset': 'non-color' },
    'ui/src/components/download/TransferUnlock.vue': { '--fb-focus-safe-inset': 'non-color' },
};
const colorProperty = /^(?:color|background-color|(?:border(?:-(?:top|right|bottom|left|inline|block)(?:-start|-end)?)?|outline|text-decoration|column-rule)-color|fill|stroke|caret-color|accent-color|stop-color)$/;
const paintedProperty = /^(?:background(?:-image|-color)?|border(?:-.+)?|outline(?:-.+)?|box-shadow|text-shadow|color|fill|stroke|caret-color|accent-color|text-decoration(?:-color)?|filter)$/;
const utility = /\b(?:bg|text|border|ring|outline|fill|stroke|caret|decoration|shadow|from|via|to)-(?:violet|purple|fuchsia|indigo|slate|gray|zinc|neutral|white|black|red|green|amber|blue|yellow|emerald|rose|sky|teal|orange|cyan|lime|pink|stone)(?:-\d+)?\b/;
const cssProperty = (value) => value.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);

function declaration(property, value, path) {
    const prop = cssProperty(property);
    if (!paintedProperty.test(prop) && !value.includes('--fb-')) return;
    let ast;
    try { ast = css.parse(value, { context: 'value', parseCustomProperty: true }); }
    catch { errors.push(`${path}: invalid ${prop}: ${value}`); return; }
    const references = [];
    css.walk(ast, (node) => {
        if (node.type === 'Function' && node.name === 'var') {
            const name = node.children.first?.name;
            if (!name?.startsWith('--fb-')) return;
            const type = contracts.types[name] ?? runtime[path]?.[name];
            if (!type) errors.push(`${path}: undefined or out-of-scope token ${name}`);
            if (type && colorProperty.test(prop) && type !== 'color') errors.push(`${path}: ${name} is ${type}, invalid in ${prop}`);
            if ((prop === 'color' || prop === 'accent-color' || prop === 'outline-color' || path.endsWith('/SmoothProgress.vue')) && /^--fb-brand(?:-|$)/.test(name)) errors.push(`${path}: decorative brand role ${name} in functional ${prop}`);
            if (/^background/.test(prop) && /^--fb-shadow/.test(name)) errors.push(`${path}: shadow color ${name} is not an interaction surface`);
            references.push(name);
        }
        const literal = node.type === 'Hash' || (node.type === 'Function' && /^(?:rgb|rgba|hsl|hsla|hwb|lab|lch|oklab|oklch|color)$/.test(node.name)) || (node.type === 'Identifier' && !['transparent', 'currentcolor'].includes(node.name.toLowerCase()) && css.lexer.matchType('named-color', node).matched);
        if (literal && paintedProperty.test(prop)) errors.push(`${path}: use a semantic token instead of ${css.generate(node)} in ${prop}`);
    });
    if (!paintedProperty.test(prop) || references.some((name) => !contracts.types[name])) return;
    // Validate the full substituted value, in every scope, rather than finding one
    // valid color inside an otherwise invalid gradient/border/shadow expression.
    for (const [preset, modes] of Object.entries(contracts.profiles)) {
        for (const [mode, tokens] of Object.entries(modes)) {
            const resolved = css.clone(ast);
            css.walk(resolved, { visit: 'Function', enter(node, item, list) {
                if (node.name !== 'var') return;
                const token = node.children.first?.name;
                if (tokens[token]) list.replace(item, css.parse(tokens[token], { context: 'value' }).children);
            } });
            // A non-theme variable may be supplied by Reka or a measured layout.
            if (css.generate(resolved).includes('var(')) continue;
            const result = css.lexer.matchProperty(prop, resolved);
            if (result.error) {
                errors.push(`${path}: ${preset}/${mode} invalid ${prop}: ${value} (${result.error.rawMessage})`);
                return;
            }
        }
    }
}

function stylesheet(source, path, declarationsOnly = false) {
    const ast = css.parse(source, { context: declarationsOnly ? 'declarationList' : 'stylesheet', parseCustomProperty: true });
    css.walk(ast, { visit: 'Declaration', enter(node) { declaration(node.property, css.generate(node.value), path); } });
}

function classes(value, path) {
    if (utility.test(value)) errors.push(`${path}: fixed color utility`);
    for (const match of value.matchAll(/\b(bg|text|border|ring|outline|fill|stroke|caret|decoration|from|via|to)-\[([^\]]+)\]/g)) {
        const prop = { bg: 'background', text: 'color', border: 'border-color', ring: 'outline-color', outline: 'outline-color', fill: 'fill', stroke: 'stroke', caret: 'caret-color', decoration: 'text-decoration-color', from: 'color', via: 'color', to: 'color' }[match[1]];
        if (/^(?:length|size):/.test(match[2])) continue;
        declaration(prop, match[2].replace(/^color:/, '').replaceAll('_', ' '), path);
    }
}

function script(source, path) {
    const ast = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
    function visit(node) {
        if (ts.isStringLiteralLike(node)) classes(node.text, path);
        if (ts.isPropertyAssignment(node) && (ts.isIdentifier(node.name) || ts.isStringLiteral(node.name)) && ts.isStringLiteralLike(node.initializer)) declaration(node.name.text, node.initializer.text, path);
        ts.forEachChild(node, visit);
    }
    visit(ast);
}

function template(node, path) {
    if (node.type === 1) {
        for (const prop of node.props) {
            if (prop.type === 6 && prop.value) {
                if (prop.name === 'style') stylesheet(prop.value.content, path, true);
                else if (prop.name === 'class') classes(prop.value.content, path);
                else if (colorProperty.test(prop.name)) declaration(prop.name, prop.value.content, path);
            }
            if (prop.type === 7 && prop.exp) {
                if (prop.arg?.content === 'style') script(`const style = (${prop.exp.content})`, path);
                else if (prop.arg?.content === 'class') classes(prop.exp.content, path);
                else if (colorProperty.test(prop.arg?.content ?? '')) script(`const style = { '${prop.arg.content}': ${prop.exp.content} }`, path);
            }
        }
    }
    for (const child of node.children ?? []) template(child, path);
}

async function scan(directory) {
    for (const entry of await readdir(join(root, directory), { withFileTypes: true })) {
        const path = join(directory, entry.name);
        if (entry.isDirectory()) {
            if (!['node_modules', 'actions', 'routes', 'wayfinder'].includes(entry.name)) await scan(path);
            continue;
        }
        if (!['.css', '.vue', '.ts'].includes(extname(path))) continue;
        if (path.endsWith('.spec.ts')) continue;
        // Generated Iconsax/platform artwork is checked by check:icons, not a CSS style object.
        if (path === 'ui/src/components/primitives/icons.generated.ts') continue;
        const source = await readFile(join(root, path), 'utf8');
        if (path.endsWith('.vue')) {
            const component = parseVue(source, { filename: path }).descriptor;
            for (const style of component.styles) stylesheet(style.content, path);
            if (component.template?.ast) template(component.template.ast, path);
            for (const block of [component.script, component.scriptSetup]) if (block) script(block.content, path);
        } else if (path.endsWith('.css')) stylesheet(source, path);
        else script(source, path);
    }
}

// These reject the failure modes the scanner guards, including nested fallbacks.
for (const [property, value] of [['border-color', 'var(--fb-action-bg)'], ['color', 'var(--fb-text, #fff)'], ['fill', 'orange'], ['color', 'var(--fb-preset-color)'], ['color', 'var(--fb-brand)'], ['background', 'var(--fb-shadow-soft)']]) {
    const before = errors.length;
    declaration(property, value, 'scanner-fixture.vue');
    assert(errors.length > before, `Scanner accepted ${property}: ${value}`);
    errors.length = before;
}
for (const [preset, modes] of Object.entries(contracts.profiles)) {
    for (const [mode, tokens] of Object.entries(modes)) {
        assert.deepEqual(Object.keys(tokens).sort(), Object.keys(contracts.types).sort(), `${preset}/${mode} token completeness`);
        for (const [name, type] of Object.entries(contracts.types)) {
            const property = { color: 'color', background: 'background', shadow: 'box-shadow' }[type];
            if (property && css.lexer.matchProperty(property, tokens[name]).error) errors.push(`${preset}/${mode}: invalid ${type} token ${name}: ${tokens[name]}`);
        }
    }
}
for (const source of ['<template><span style="color:#fff" /></template>', '<template><svg fill="orange" /></template>', '<template><span class="bg-lime-200" /></template>', '<template><span :style="{ color: \'#fff\' }" /></template>']) {
    const before = errors.length;
    template(parseVue(source).descriptor.template.ast, 'scanner-fixture.vue');
    assert(errors.length > before, `Scanner accepted ${source}`);
    errors.length = before;
}
for (const directory of roots) await scan(directory);
if (errors.length) throw new Error([...new Set(errors)].join('\n'));
console.log('Theme types, scoped references, literals and all 14 profiles passed.');
