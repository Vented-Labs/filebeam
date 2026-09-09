import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readdir, readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, extname, join, relative } from 'node:path';
import { parse as parseSfc } from '@vue/compiler-sfc';

const require = createRequire(import.meta.url);
const ts = require('typescript');
const root = new URL('../', import.meta.url).pathname.replace(/\/$/, '');
const sourceRoots = ['ui/src', 'backend/resources/js', 'backend/resources/css', 'backend/resources/views', 'backend/app', 'scripts/og'];
const sourceFiles = ['vite.config.ts', 'backend/vite.config.ts'];
const sourceExtensions = new Set(['.vue', '.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs', '.css', '.scss', '.php', '.blade.php']);
const rendererFiles = new Set(['ui/src/components/primitives/Icon.vue', 'ui/src/components/primitives/icons.generated.ts', 'backend/app/Support/Icons/Iconsax.php', 'backend/resources/views/components/filebeam-icon.blade.php']);
const rawSvgFiles = new Set(['ui/src/components/primitives/Icon.vue', 'ui/src/components/primitives/icons.generated.ts', 'backend/app/Support/Icons/Iconsax.php']);
const filamentHeroiconMigration = 'backend/app/Support/Icons/FilamentIcons.php';
const identitySvgAssets = new Set(['/brand/filebeam-logo-header.svg', '/brand/filebeam-mark.svg']);
const approvedSource = {
    package: 'iconsax',
    version: '0.1.1',
    tarballIntegrity: 'sha512-/HXomG+SuhytKH/Yw4ZdunDGyWP02Osuk/pjony1k9CCMOsg7dUpnomu7HZUOHNu0yK0b3phErjH2IgUTJFjPA==',
    style: 'twotone',
};
const foreignCatalog = /(?:^|[\/@_-])(heroicons?|lucide(?:-react|-vue)?|fontawesome|fortawesome|phosphor|radix-icons|tabler-icons|material-icons|iconsax)(?:$|[\/@_-])/i;
const iconAsset = /(?:data:image\/svg\+xml|\.svg(?:[?#'"\s)]|$)|\bmask(?:-image)?\s*:|@font-face\b|font-family\s*:\s*['"]?(?:icon|heroicon|lucide|fontawesome))/i;

async function files(directory) {
    const absolute = join(root, directory);
    const entries = await readdir(absolute, { withFileTypes: true });
    return (await Promise.all(entries.map(async (entry) => entry.isDirectory() && entry.name !== 'node_modules'
        ? files(join(directory, entry.name))
        : sourceExtensions.has(extname(entry.name)) || entry.name.endsWith('.blade.php')
            ? [join(absolute, entry.name)]
            : []))).flat();
}

function add(errors, path, message) {
    errors.push(`${path}: ${message}`);
}

function moduleSpecifier(node) {
    return node && ts.isStringLiteral(node) ? node.text : null;
}

function isCatalogSpecifier(specifier) {
    return foreignCatalog.test(specifier) || /(?:^|\/)icons\.generated(?:$|\.)/.test(specifier);
}

function checkScript(path, source, errors) {
    const file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true, path.endsWith('.tsx') || path.endsWith('.jsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
    const checkSpecifier = (specifier) => {
        if (specifier && isCatalogSpecifier(specifier) && !rendererFiles.has(path)) add(errors, path, `direct icon catalog import or re-export: ${specifier}`);
    };
    const visit = (node) => {
        if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) checkSpecifier(moduleSpecifier(node.moduleSpecifier));
        if (ts.isCallExpression(node)) {
            const [first] = node.arguments;
            if (node.expression.kind === ts.SyntaxKind.ImportKeyword) checkSpecifier(moduleSpecifier(first));
            if (ts.isIdentifier(node.expression) && node.expression.text === 'require') checkSpecifier(moduleSpecifier(first));
            if (ts.isIdentifier(node.expression) && ['h', 'createElement'].includes(node.expression.text) && moduleSpecifier(first)?.toLowerCase() === 'svg') add(errors, path, 'programmatic SVG rendering bypasses the icon registry');
            if (ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === 'insertAdjacentHTML') add(errors, path, 'HTML injection API can render unreviewed icon markup');
        }
        if (ts.isBinaryExpression(node) && ['innerHTML', 'outerHTML'].includes(node.left.name?.text)) add(errors, path, 'HTML injection API can render unreviewed icon markup');
        if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) {
            if (node.tagName.getText(file).toLowerCase() === 'svg') add(errors, path, 'raw SVG bypasses the icon registry');
        }
        ts.forEachChild(node, visit);
    };
    visit(file);
}

function checkVue(path, content, errors) {
    const { descriptor, errors: parseErrors } = parseSfc(content, { filename: path });
    if (parseErrors.length) add(errors, path, 'cannot parse Vue SFC for icon policy');
    for (const block of [descriptor.script, descriptor.scriptSetup]) if (block) checkScript(path, block.content, errors);
    const template = descriptor.template?.content ?? '';
    if (!rawSvgFiles.has(path) && /<svg\b/i.test(template)) add(errors, path, 'raw SVG bypasses the icon registry');
    for (const match of template.matchAll(/<(?:img|image)\b[^>]+(?:src|href)\s*=\s*['"]([^'"]+\.svg)['"]/gi)) {
        if (!identitySvgAssets.has(match[1])) add(errors, path, 'SVG image bypasses the icon registry');
    }
    if (!rendererFiles.has(path) && /\bv-html\b|\binnerHTML\b|dangerouslySetInnerHTML\b/i.test(template)) add(errors, path, 'HTML injection API can render unreviewed icon markup');
    if (iconAsset.test(descriptor.styles.map((style) => style.content).join('\n'))) add(errors, path, 'CSS icon asset, font, mask, or SVG data URL bypasses the icon registry');
}

function checkPhp(path, content, errors) {
    if (path !== filamentHeroiconMigration && /(?:\\|\b)Heroicon::|@heroicons\//i.test(content)) add(errors, path, 'foreign Heroicons usage');
    const catalogAliases = [...content.matchAll(/^\s*use\s+App\\Support\\Icons\\Iconsax(?:\s+as\s+(\w+))?\s*;/gmi)].map((match) => match[1] ?? 'Iconsax');
    const catalogNames = ['\\\\App\\\\Support\\\\Icons\\\\Iconsax', ...(catalogAliases.length ? catalogAliases.map((name) => `\\b${name}`) : ['\\bIconsax'])];
    const catalogReference = new RegExp(`(?:${catalogNames.join('|')})::render\\s*\\(`, 'i');
    if (path !== 'backend/app/Support/Icons/Iconsax.php' && catalogReference.test(content) && !rendererFiles.has(path)) add(errors, path, 'direct icon catalog render outside the renderer boundary');
    if (!rawSvgFiles.has(path) && /<svg\b/i.test(content)) add(errors, path, 'raw SVG bypasses the icon registry');
    if (/\b(?:HtmlString|new\s+HtmlString)\b|->(?:html|raw)\s*\(/i.test(content)) add(errors, path, 'HTML injection API can render unreviewed icon markup');
}

export function violations(path, content) {
    const errors = [];
    if (path.endsWith('.vue')) checkVue(path, content, errors);
    else if (/\.(?:[cm]?[jt]sx?)$/.test(path)) checkScript(path, content, errors);
    else if (path.endsWith('.php')) checkPhp(path, content, errors);
    if (/\.(?:css|scss)$/.test(path) && iconAsset.test(content)) add(errors, path, 'CSS icon asset, font, mask, or SVG data URL bypasses the icon registry');
    return [...new Set(errors)];
}

function dependencies(manifest) {
    return Object.fromEntries(Object.entries({ ...manifest.dependencies, ...manifest.devDependencies, ...manifest.optionalDependencies }).sort(([a], [b]) => a.localeCompare(b)));
}

function resolvedNpm(lock) {
    return Object.fromEntries(Object.entries(lock.packages ?? {}).filter(([path, pkg]) => path.includes('node_modules/') && pkg.version).sort(([a], [b]) => a.localeCompare(b)).map(([path, pkg]) => [path, pkg.version]));
}

function resolvedComposer(lock) {
    return Object.fromEntries([...(lock.packages ?? []), ...(lock['packages-dev'] ?? [])].sort((a, b) => a.name.localeCompare(b.name)).map((pkg) => [pkg.name, pkg.version]));
}

function fingerprint(values) {
    return createHash('sha256').update(JSON.stringify(values)).digest('hex');
}

export function resolvedLockViolations(inventory, npmLock, composerLock) {
    const errors = [];
    const npm = resolvedNpm(npmLock);
    const composer = resolvedComposer(composerLock);
    if (Object.keys(npm).length !== inventory.npmResolved.count || fingerprint(npm) !== inventory.npmResolved.sha256) errors.push('package-lock.json: resolved package inventory differs from the closed review baseline');
    if (Object.keys(composer).length !== inventory.composerResolved.count || fingerprint(composer) !== inventory.composerResolved.sha256) errors.push('backend/composer.lock: resolved package inventory differs from the closed review baseline');
    if (composer['blade-ui-kit/blade-heroicons'] !== inventory.composerResolved.exceptions['blade-ui-kit/blade-heroicons']) errors.push('backend/composer.lock: the reviewed unused blade-heroicons exception changed');
    return errors;
}

async function checkDependencyInventory() {
    const inventory = JSON.parse(await readFile(join(root, 'icons/dependency-inventory.json'), 'utf8'));
    const npmLock = JSON.parse(await readFile(join(root, 'package-lock.json'), 'utf8'));
    const errors = [];
    const rootManifest = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
    const workspacePaths = (Array.isArray(rootManifest.workspaces) ? rootManifest.workspaces : rootManifest.workspaces?.packages ?? []).map((workspace) => `${workspace}/package.json`).sort();
    const reviewedPaths = Object.keys(inventory.npm).filter((path) => path !== 'package.json').sort();
    if (JSON.stringify(workspacePaths) !== JSON.stringify(reviewedPaths)) add(errors, 'package.json', 'workspace manifests differ from the reviewed icon-policy inventory');
    for (const [path, reviewed] of Object.entries(inventory.npm)) {
        const manifest = JSON.parse(await readFile(join(root, path), 'utf8'));
        if (JSON.stringify(dependencies(manifest)) !== JSON.stringify(reviewed)) add(errors, path, 'dependency declarations differ from the reviewed icon-policy inventory');
        const lockPath = path === 'package.json' ? '' : path.replace(/\/package\.json$/, '');
        const locked = npmLock.packages?.[lockPath];
        if (!locked) add(errors, path, 'package-lock.json has no workspace entry');
        for (const name of Object.keys(reviewed)) {
            const resolved = Object.keys(npmLock.packages ?? {}).some((entry) => entry === `node_modules/${name}` || entry.endsWith(`/node_modules/${name}`));
            if (!resolved && !name.startsWith('@filebeam/')) add(errors, path, `package-lock.json does not resolve ${name}`);
        }
    }
    const composer = JSON.parse(await readFile(join(root, 'backend/composer.json'), 'utf8'));
    const composerLock = JSON.parse(await readFile(join(root, 'backend/composer.lock'), 'utf8'));
    const reviewedComposer = inventory.composer['backend/composer.json'];
    const declaredComposer = Object.fromEntries(Object.entries({ ...composer.require, ...composer['require-dev'] }).filter(([name]) => name !== 'php' && !name.startsWith('ext-')).sort(([a], [b]) => a.localeCompare(b)));
    if (JSON.stringify(declaredComposer) !== JSON.stringify(reviewedComposer)) add(errors, 'backend/composer.json', 'dependency declarations differ from the reviewed icon-policy inventory');
    const lockedComposer = new Set([...composerLock.packages, ...composerLock['packages-dev']].map((pkg) => pkg.name));
    for (const name of Object.keys(reviewedComposer)) if (!lockedComposer.has(name)) add(errors, 'backend/composer.json', `composer.lock does not resolve ${name}`);
    return [...errors, ...resolvedLockViolations(inventory, npmLock, composerLock)];
}

async function checkApprovedSource() {
    const manifest = JSON.parse(await readFile(join(root, 'icons/approved.json'), 'utf8'));
    const errors = [];
    for (const [key, value] of Object.entries(approvedSource)) if (manifest.source?.[key] !== value) add(errors, 'icons/approved.json', `approved source ${key} is immutable (${value})`);
    if (errors.length) return errors;
    const packageLock = JSON.parse(await readFile(join(root, 'package-lock.json'), 'utf8'));
    const locked = packageLock.packages?.['node_modules/iconsax'];
    if (locked?.version !== approvedSource.version || locked.integrity !== approvedSource.tarballIntegrity) add(errors, 'package-lock.json', 'does not pin the immutable approved Iconsax source');
    const packageDirectory = dirname(dirname(require.resolve('iconsax')));
    const groups = ['ai', 'archive', 'arrow', 'astrology', 'building', 'business', 'call', 'car', 'christmas', 'computers-devices-electronics', 'content-edit', 'crypto', 'cryptocurrency', 'delivery', 'design-tools', 'emails-messages', 'essential', 'files', 'grid', 'location', 'money', 'notifications', 'programming', 'school-learning', 'search', 'security', 'settings', 'shop', 'support-like-question', 'time', 'type-paragraph-character', 'users', 'video-audio-image', 'weather'];
    const catalog = Object.assign({}, ...(await Promise.all(groups.map(async (group) => JSON.parse(await readFile(join(packageDirectory, 'dist/data', `${group}.json`), 'utf8'))))));
    const entries = Object.entries(manifest.icons).map(([name, source]) => [
        name,
        catalog[source]?.[approvedSource.style]?.replace(/^<svg\b[^>]*>/, '').replace(/<\/svg>$/, '').replaceAll('white', 'currentColor').replace(/\s+/g, ' ').trim(),
    ]);
    if (entries.some(([, markup]) => typeof markup !== 'string')) add(errors, 'icons/approved.json', 'contains an icon missing from the immutable Twotone source');
    const digest = createHash('sha256').update(JSON.stringify(entries)).digest('hex');
    const vue = await readFile(join(root, 'ui/src/components/primitives/icons.generated.ts'), 'utf8');
    const php = await readFile(join(root, 'backend/app/Support/Icons/Iconsax.php'), 'utf8');
    if (!new RegExp(`iconsaxSourceDigest\\s*=\\s*'${digest}'`).test(vue) || !new RegExp(`SOURCE_DIGEST\\s*=\\s*'${digest}'`).test(php)) add(errors, 'generated icon registries', 'source digest does not match the installed immutable Iconsax source');
    const expectedAssets = new Set(Object.keys(manifest.icons).map((name) => `${name}.svg`));
    const actualAssets = (await readdir(join(root, 'backend/resources/icons/iconsax'))).filter((name) => name.endsWith('.svg'));
    for (const name of actualAssets) if (!expectedAssets.delete(name)) add(errors, `backend/resources/icons/iconsax/${name}`, 'unapproved generated icon asset');
    for (const name of expectedAssets) add(errors, `backend/resources/icons/iconsax/${name}`, 'missing approved generated icon asset');
    return errors;
}

async function selfTest() {
    const fixtures = [
        ['tests/icon-policy/foreign-catalog-import.ts', 'direct icon catalog import or re-export'],
        ['tests/icon-policy/dynamic-catalog-loader.ts', 'direct icon catalog import or re-export'],
        ['tests/icon-policy/catalog-alias-reexport.ts', 'direct icon catalog import or re-export'],
        ['tests/icon-policy/raw-svg-template.vue', 'raw SVG bypasses the icon registry'],
        ['tests/icon-policy/injected-svg.ts', 'HTML injection API'],
        ['tests/icon-policy/icon-mask.css', 'CSS icon asset, font, mask, or SVG data URL'],
        ['tests/icon-policy/heroicon-reference.php', 'foreign Heroicons usage'],
        ['tests/icon-policy/iconsax-import-alias.php', 'direct icon catalog render outside the renderer boundary'],
        ['tests/icon-policy/iconsax-import.php', 'direct icon catalog render outside the renderer boundary'],
    ];
    for (const [path, expected] of fixtures) {
        const errors = violations(path, await readFile(join(root, path), 'utf8'));
        assert.ok(errors.some((error) => error.includes(expected)), `${path} should report ${expected}`);
    }
    assert.deepEqual(violations(filamentHeroiconMigration, 'Heroicon::OutlinedUsers'), []);
    const inventory = JSON.parse(await readFile(join(root, 'icons/dependency-inventory.json'), 'utf8'));
    const npmLock = JSON.parse(await readFile(join(root, 'package-lock.json'), 'utf8'));
    const composerLock = JSON.parse(await readFile(join(root, 'backend/composer.lock'), 'utf8'));
    const npmWithUnreviewedPackage = structuredClone(npmLock);
    npmWithUnreviewedPackage.packages['node_modules/lucide-vue-next'] = { version: '99.0.0' };
    assert.ok(resolvedLockViolations(inventory, npmWithUnreviewedPackage, composerLock).some((error) => error.includes('package-lock.json')));
    const composerWithUnreviewedPackage = structuredClone(composerLock);
    composerWithUnreviewedPackage.packages.push({ name: 'random/icon-pack', version: '1.0.0' });
    assert.ok(resolvedLockViolations(inventory, npmLock, composerWithUnreviewedPackage).some((error) => error.includes('composer.lock')));
}

if (process.argv.includes('--self-test')) await selfTest();

const dependencyErrors = await checkDependencyInventory();
const sourceIntegrityErrors = await checkApprovedSource();
const scannedFiles = [...(await Promise.all(sourceRoots.map(files))).flat(), ...sourceFiles.map((path) => join(root, path))];
const sourceErrors = (await Promise.all(scannedFiles.map(async (absolute) => {
    const path = relative(root, absolute);
    return violations(path, await readFile(absolute, 'utf8'));
}))).flat();
const errors = [...dependencyErrors, ...sourceIntegrityErrors, ...sourceErrors];
if (errors.length) throw new Error(`Icon policy violations:\n${errors.join('\n')}`);
