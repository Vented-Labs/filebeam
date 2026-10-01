# Desktop Guidelines

- Follow the repository-root `AGENTS.md` hygiene rules. All agent artifacts and
  temporary validation tools belong in root `.filebeam/`, never `docs/`.
- Use native Rust and pinned GPUI Kit components, without embedded web surfaces.
  Confirm APIs against the pinned crates.
- Retain UI state and workers outside render. Rendering must not perform I/O,
  dispatch work, persist settings, or recreate retained entities.
- Use the existing theme, semantic colors, Filebeam mark, and approved Iconsax
  assets with their opacity and license notices. Respect OS accessibility and
  reduced-motion preferences.
- Preserve invalid drafts and distinguish transfer, verification, export, pause,
  end-live, revocation, and local removal states.
- Background transfers require the native tray/menu-bar service. All explicit
  Quit paths use the same checkpoint/pause behavior.
- Implement end-user workflows natively; server administration remains web-only.
  Use the shared Desktop/CLI settings and updater contracts, with side-by-side
  `beam` and `filebeam` binaries under the same `vX.Y.Z` release tag.
- Run every local Cargo command through the resource-limited Docker runner:
  `bash scripts/desktop/run.sh cargo ...`. Do not run host Cargo.
