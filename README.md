# Filebeam

Filebeam is a self-hosted application for sharing encrypted files and notes. Content is encrypted in the browser before upload; the server stores ciphertext and the metadata needed to operate a transfer.

Filebeam is free and open-source software by [Vented](https://vented.com), released under the [MIT License](LICENSE).

## Features

- Browser-side encrypted files and notes, with optional password protection.
- Encrypted recipient delivery, expiry, deletion, and best-effort burn-on-read notes.
- Private local or S3-compatible ciphertext storage, with distributed or replicated placement.
- Account administration, registration controls, cron-driven background work, scheduled cleanup, and a release updater.

## Setup

Release installations require PHP 8.5+ and an HTTPS server whose web root is `backend/public`. Start with the [deployment guide](docs/deployment.md), or use the [Docker deployment guide](docs/docker.md) for the official GHCR images.

Shared hosting uses one minute-by-minute cron entry running `php artisan schedule:run` from `backend/`. A persistent queue worker is not required with the default database-backed background jobs.

For local development, install Docker Engine and Docker Compose, then run:

```sh
docker compose -f backend/compose.yaml build
docker compose -f backend/compose.yaml run --rm --no-deps laravel.test composer install --no-interaction
docker compose -f backend/compose.yaml run --rm --no-deps laravel.test bash -c 'if [ ! -f .env ]; then cp .env.example .env && php artisan key:generate; fi'
./sail up -d
./sail php artisan migrate --seed
./sail npm --prefix .. ci
./sail npm --prefix .. run build
```

## Documentation

- [Deployment and recovery](docs/deployment.md)
- [Docker deployment](docs/docker.md)
- [Security policy and limitations](SECURITY.md)
- [Encryption protocol](encryption/README.md)
- [Contributing](CONTRIBUTING.md)
- [UI workspace](ui/README.md)
- [Beam CLI](docs/beam-cli.md)
