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

Color roles are defined in `backend/resources/themes/default.css` and consumed as
`--fb-*` CSS variables. `App\Support\Theming\Palette` generates runtime palettes
from those defaults. Keep component colors, interaction states, and decorative
effects in tokens; the default dark values are the visual compatibility baseline.
Appearance changes preserve mounted components and editor state.

The footer paintbrush opens the appearance popup. Guest preferences are browser-local;
authenticated preferences are saved under `users.settings.appearance`. Preset IDs
are defined by `ThemePreset`; public palettes and instance branding use separate
resolvers. The component gallery uses the same PHP palette generator.
