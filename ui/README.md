# Filebeam UI

`@filebeam/ui` is Filebeam's private workspace package for shared Vue transfer, note, account, and layout components. It is source code for this repository, not a separately published package.

Build from the repository root:

```sh
npm ci
npm run build
```

The root build generates the encryption WebAssembly package before building the backend assets. Backend workspace commands alone do not rebuild it.

## Clipboard input

On the Send screen, paste text to open Notes or paste an image to add it to Files.
Existing drafts are preserved. Paste inside an editor or form field behaves
normally. Use the browser or keyboard paste action anywhere on the Send page;
image availability depends on the browser and clipboard source. **Choose files**
is available when the browser cannot expose a clipboard image. Sharing still
requires the Send action.

## Theme tokens

`App\Support\Theming\ThemeDefinition` defines authored light/dark color roles,
fixed syntax and semantic colors, and typed compatibility aliases. `Palette`
composes these definitions; `CustomPalette` derives contrast-checked roles for
arbitrary instance seeds. Components consume `--fb-*` variables. Use color tokens
for foregrounds and borders, background tokens for backgrounds, and shadow tokens
for elevation. Appearance changes preserve mounted components and editor state.

`backend/resources/themes/default.css` is a generated fallback, not palette input.
Regenerate and verify it from the repository root:

```sh
php scripts/themes/palette.php --write-default
npm run check:themes
```

SVG artwork, raster compatibility, mixed-mode email, and Filament have separate
adapters. `php scripts/themes/palette.php --write-artwork` updates the bundled
default lockups when their explicit artwork definitions change.

`ThemeEffects` composes decorative gradients separately: dark recipes preserve
their original lighting, while light recipes use authored tints and falloffs.
The Notes wash belongs to its toolbar. `EditorPalette` gives the opaque editor,
gutters, selections, and keyword/type/heading accents their own contrast-checked
preset colors. Strings, numbers, errors, and other semantic syntax retain their
distinct mode-specific colors.

The footer paintbrush opens the appearance popup. Guest preferences are browser-local;
authenticated preferences are saved under `users.settings.appearance`. Preset IDs
are defined by `ThemePreset`; public palettes and instance branding use separate
resolvers. The component gallery uses the same PHP palette generator.

After building, `npm run test:theme` checks an isolated application and
`npm run test:prism` checks production component fixtures and the editor. Both
include visual regressions. Use `THEME_CAPTURE=1` to retain review images, inspect
them, then explicitly pass `-- --update-snapshots` for intentional baseline changes.
