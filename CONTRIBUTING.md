# Contributing

## Local Setup

Use Docker Engine and Docker Compose. Follow the local setup commands in the [README](README.md), then run commands from the repository root:

```sh
./sail test
./sail npm --prefix .. run check
./sail run cargo test --manifest-path ../encryption/Cargo.toml
```

Browser tests run on the host against an isolated application instance: `BASE_URL=http://127.0.0.1:8000 npm run test:browser`. Install Chromium first with `npx playwright install chromium`.

Sail captures outgoing development email in Mailpit. Open [http://localhost:8025](http://localhost:8025) after running `./sail up -d`; the default `.env.example` mail settings also reach Mailpit from host-side Artisan commands. Override `FORWARD_MAILPIT_PORT` or `FORWARD_MAILPIT_DASHBOARD_PORT` if either port is already in use.

Use `backend/composer.json` for PHP commands and root `package.json` for JavaScript commands. Run the narrowest relevant checks before proposing a change.

## Contribution Rules

- Preserve browser encryption: plaintext files, notes, titles, filenames, and manifest metadata must not become API metadata or server logs.
- Keep storage private and keep changes compatible with SQLite, MySQL/MariaDB, and PostgreSQL unless a change explicitly documents otherwise.
- We need to support traditional shared hosting as well as more modern (FrankenPHP/Docker) installations 100% on every feature.
- Do not commit secrets, `.env` files, generated dependencies, build output, runtime state, or local tooling metadata.

Open a focused pull request with tests and documentation appropriate to the behavior changed. Report security issues through the [private vulnerability form](https://github.com/Vented-Labs/filebeam/security/advisories/new), not public issues.

## Release Packages

Maintainers create official packages from a Git commit:

```sh
RELEASE_PUBLIC_KEY=... scripts/release/package.sh v0.1.0 dist/release
```
