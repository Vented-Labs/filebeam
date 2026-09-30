# Desktop Development

The desktop client uses Rust and GPUI Kit. Run local Cargo commands through the
Docker runner from the repository root:

```sh
bash scripts/desktop/check.sh
bash scripts/desktop/build.sh
bash scripts/desktop/dev.sh
bash scripts/desktop/run.sh cargo test --manifest-path desktop/Cargo.toml --locked
```

The runner limits builds to 4 GiB of memory, two CPUs, and one Cargo job. It caches
builds under `${XDG_CACHE_HOME:-$HOME/.cache}/filebeam/desktop/<worktree-hash>`;
`FILEBEAM_DESKTOP_CACHE_DIR` overrides this location. Binaries are in its `target/`
directory (`/target` inside the container). Set `FILEBEAM_DESKTOP_BUILD_IMAGE=true`
to rebuild the tooling image.

## Run with local settings

The runner uses an isolated home by default. To launch on Wayland with an existing
Filebeam home owned by the current user:

```sh
FILEBEAM_DESKTOP_HOME="$HOME/.filebeam" \
FILEBEAM_DESKTOP_WAYLAND=1 \
FILEBEAM_DESKTOP_DRI=1 \
bash scripts/desktop/run.sh cargo run --manifest-path desktop/Cargo.toml --bin filebeam -- --home /filebeam-home
```

This grants the container access to the selected settings directory, compositor,
and GPU. The process holds the build-runner lock until it exits.

## Assets

`desktop/assets/filebeam-mark.svg` is the source for platform icons. Generating
them requires ImageMagick and `icotool`:

```sh
bash scripts/desktop/generate-icons.sh
node scripts/desktop/generate-assets.mjs
```

Platform icons are written to `desktop/packaging/icons/`. The native asset registry
uses `icons/approved.json`; its palette is checked against `desktop/assets/colors.json`.

To install a per-user Linux development launcher:

```sh
bash scripts/desktop/install-dev.sh
gtk-launch io.filebeam.desktop
```

The installer refuses to overwrite a non-development launcher.

## UI fixtures

The `visual-test` feature enables synthetic UI scenarios without starting transfer
workers or reading user settings:

```sh
bash scripts/desktop/run.sh cargo run --manifest-path desktop/Cargo.toml --example visual_studio --features visual-test -- transfers-mixed
```
