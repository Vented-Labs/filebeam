# Product icons

`approved.json` defines the shared registry: pinned Iconsax TwoTone artwork,
reviewed brand identities, and original Filebeam artwork under `customIcons`.
See `NOTICE` for their separate attribution and licensing.

## Custom artwork

Canonical SVGs live in `icons/custom/`. Each manifest entry records its path,
SHA-256 hash and description. Names must be unique across the registry.

Custom drawings use a 24 × 24 viewBox, an inner group with `fill="none"`,
`stroke="currentColor"`, 1.5-unit strokes and round caps/joins. Secondary geometry
uses a group with `opacity="0.4"`. The supported drawing subset is groups and
absolute `M`, `L`, `H`, `V`, `C`, `Z` paths; external resources and other markup
are rejected.

| Name | Use |
| --- | --- |
| `http-server` | HTTP transport identity across graphical clients |
| `webrtc-p2p` | WebRTC transport identity across graphical clients |
| `end-to-end-encrypted` | Homepage encryption feature |
| `fast-and-simple` | Homepage speed/simplicity feature |

Use these names at their semantic call sites through the existing icon wrapper.
General-purpose archive, storage, lock and lightning artwork has distinct uses.

## Generation and checks

From the repository root:

```sh
npm run generate:icons
node scripts/desktop/generate-assets.mjs
npm run check:icons
npm run test:icons
node scripts/desktop/generate-assets.mjs --check
```

The shared generator writes Vue and PHP registries, Blade SVGs, Android transport
VectorDrawables and native SwiftUI transport paths. Mobile geometry is derived
from the canonical paths and composites secondary opacity once per layer.
The desktop generator embeds the same reviewed markup for Linux, macOS and
Windows. Generated outputs are checked in and must not be edited by hand.

The production Docker frontend stage uses `--web-only` for icon checks because
its build context excludes mobile sources. The standard repository checks also
verify the generated mobile assets.
