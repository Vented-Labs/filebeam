# Contributing

## Local Setup

Use Docker Engine and Docker Compose. Follow the local setup commands in the [README](README.md), then run commands from the repository root:

```sh
./sail test
./sail npm --prefix .. run check
./sail run cargo test --manifest-path ../encryption/Cargo.toml
```

Browser tests run on the host against an isolated application instance. Set `SAIL_APP_URL=http://127.0.0.1:8000` in `backend/.env`, then recreate the application container and use the same URL for Playwright so same-origin authentication stays within the Vue document:

```sh
./sail up -d
BASE_URL=http://127.0.0.1:8000 npm run test:browser
```

Install Chromium first with `npx playwright install chromium`. Sail sources `backend/.env`, so passing `SAIL_APP_URL` only as a shell environment variable does not override the value in that file.

Sail captures outgoing development email in Mailpit. Open [http://localhost:8025](http://localhost:8025) after running `./sail up -d`; the default `.env.example` mail settings also reach Mailpit from host-side Artisan commands. Override `FORWARD_MAILPIT_PORT` or `FORWARD_MAILPIT_DASHBOARD_PORT` if either port is already in use.

Use `backend/composer.json` for PHP commands and root `package.json` for JavaScript commands. Run the narrowest relevant checks before proposing a change.

## Continuous Integration

All Actions jobs use Blacksmith's Ubuntu 24.04 runners: `blacksmith-4vcpu-ubuntu-2404` for AMD64, `blacksmith-4vcpu-ubuntu-2404-arm` for ARM64 Docker coverage, and `blacksmith-8vcpu-ubuntu-2404` for the Beam CLI test job. CI containers have no Docker CPU or memory caps, and Cargo uses the available CPUs. Local CLI scripts retain their default limits.

Tests run when a PR is opened, updated, or reopened, and on pushes to `master`. Release workflows call the same suite with the validated release commit; tag pushes do not start a second standalone Tests run. New PR updates cancel superseded Tests, Docker, and Beam CLI runs.

Frontend build/checks, Rust, and PHP static checks start independently. The compiled frontend is shared with isolated SQLite, MySQL, Sail PostgreSQL/Redis, distribution, and browser jobs. Main browser tests run in two Sail shards with two workers each; small-chunk tests use their own SQLite application. The final `Test` check requires every job and matrix entry to succeed.

### Blacksmith caching

- Checkouts use `useblacksmith/checkout`, retaining the requested source ref and fetch settings. Container-mounted source checkouts use `dissociate: true` so Git objects remain accessible inside Docker.
- Production, Sail, and CLI tooling builds use `useblacksmith/setup-docker-builder` with a separate cache key per Dockerfile. Production variants share their common layers, and CI/release builds share the same workload key. Blacksmith handles architecture separation and builder cleanup; avoid replacing or pruning the managed builder before its post-job cache save.
- Existing `docker buildx` commands use the managed builder directly. Sail explicitly selects it via `docker compose build --builder`. Release images push directly from BuildKit; locally tested images are loaded into Docker.
- CLI tooling is built once per job. `BEAM_CARGO_CACHE_DIR` shares Cargo downloads between disposable containers, and the CLI test job caches downloads and compilation output between runs.
- Docker container-image caching is automatic on Blacksmith as it rolls out to organizations; it needs no extra action or Docker data-directory mounts. This is separate from the persistent build-layer cache.

Run `actionlint` from the repository root when editing workflows. `.github/actionlint.yaml` declares the Blacksmith runner labels. Shared PHP, Node, Rust, and MinIO setup lives in `.github/actions/`. Third-party actions are pinned to full commit SHAs; add new revisions to the repository's Actions allowlist before running CI.

## Contribution Rules

- Preserve browser encryption: plaintext files, notes, titles, filenames, and manifest metadata must not become API metadata or server logs.
- Keep storage private and keep changes compatible with SQLite, MySQL/MariaDB, and PostgreSQL unless a change explicitly documents otherwise.
- We need to support traditional shared hosting as well as more modern (FrankenPHP/Docker) installations 100% on every feature.
- Do not commit secrets, `.env` files, generated dependencies, build output, runtime state, or local tooling metadata.

Open a focused pull request with tests and documentation appropriate to the behavior changed.

## Filing Issues

Use the Bug report or Feature request templates. Questions belong in [Discussions](https://github.com/Vented-Labs/filebeam/discussions). Report security issues through the [private vulnerability form](https://github.com/Vented-Labs/filebeam/security/advisories/new), not public issues.

## Release Packages

Maintainers create official packages from a Git commit:

```sh
RELEASE_PUBLIC_KEY=... scripts/release/package.sh v0.1.0 dist/release
```
