# Social previews

Filebeam generates its default Open Graph images at build time. This keeps Chromium and image rendering out of the application runtime.

## Build output

Install the build-time browser once after `npm ci`, then run the normal frontend build:

```sh
npx playwright install --with-deps chromium
npm run build
```

To render only the cards, run:

```sh
npm run build:og
```

The backend build runs `vp build && npm run build:og --prefix ..`. The root `build:og` command runs `node scripts/og/render.mjs` and writes three 1200x630 PNGs plus a manifest to `backend/public/build/og/`:

- `home`
- `receive`
- `transfer`

Each manifest entry points to a hashed PNG filename. The renderer uses `ui/src/components/brand/OgCard.vue`, the existing `backend/public/brand` assets, and the installed `@fontsource-variable/inter` assets.

For local renderer work, start the standalone preview server with:

```sh
npm run preview:og
```

It listens on `http://127.0.0.1:5174`. Use `?variant=home`, `?variant=receive`, or `?variant=transfer` to inspect each card. The preview scales down on mobile; the renderer always captures at 1200x630. Run the build after changing the card component or its assets to refresh the generated assets used by a deployment.

After generating cards, run `npm run test:og` to check reproducible screenshots, PNG serving, and mobile scaling. Run `php scripts/release/validate-og.php backend/public/build/og` to validate dimensions and content hashes. CI and packaging run these checks before shipping assets. Pixel output is reproducible with the same installed Chromium version and platform; it is not guaranteed byte-identical across different operating systems or CPU architectures.

## Deployment behavior

Production images and release packages include the generated `public/build/og` files, not the standalone preview server. Chromium is installed only on build machines and Docker's frontend build stage; it is not present or needed at runtime.

Set `FILEBEAM_OG_IMAGE_URL` to an absolute HTTPS URL of a publicly accessible 1200x630 PNG to use one image for all supported public Filebeam pages. This is useful for custom branding, because changing runtime branding does not regenerate the default Filebeam artwork. Rebuild Laravel's configuration cache after changing the setting. The server does not fetch the override; social crawlers request it directly.

Metadata is rendered in Laravel's initial HTML for home, public recipient, and transfer routes. It does not require JavaScript or Inertia SSR. Account, inbox, authentication, and error pages do not get social cards. Tags are intentionally outside Inertia's managed head and describe the initial document, not subsequent client-side navigation.

The previews are generic. Private transfer details and recipient-specific content are never rendered into an image. If the OG manifest or its referenced image is absent, Filebeam continues serving pages and omits the image meta tags.
