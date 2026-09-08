# Docker Deployment

Official multi-architecture images are published to GitHub Container Registry only:

- [`ghcr.io/vented-labs/filebeam/light`](https://github.com/Vented-Labs/filebeam/pkgs/container/filebeam%2Flight)
- [`ghcr.io/vented-labs/filebeam/omnibus`](https://github.com/Vented-Labs/filebeam/pkgs/container/filebeam%2Fomnibus)

Use a published version tag in strict SemVer form, with its current build revision, and pin production deployments by manifest digest. `light` uses SQLite, file cache, and database-backed queues by default; external database and cache services are optional. `omnibus` includes PostgreSQL and Valkey for a small single-host deployment; those services use internal Unix sockets and must not be published.

Mount `/data` for configuration, application state (including updates), Caddy state, and Omnibus database/cache data. Mount `/storage` for local ciphertext storage, or configure S3-compatible storage. Back up `/data`, storage (or every configured bucket), and the database as one matching set. Restore PostgreSQL from a consistent `pg_dump` archive with `pg_restore`, not by copying live data files.

Adaptive-upload staging is private local disk data, not ciphertext storage and not an S3/object-store path. The default `storage/app/transfer-staging` resolves to `/data/app/transfer-staging`, so it is persistent when `/data` is mounted. When setting `FILEBEAM_STAGING_ROOT`, mount that exact private path and make it writable by UID/GID `10001:10001`; never place it under a web-served mount. A tmpfs is an optional external mount only when its loss on restart is acceptable. See [Adaptive transfers](adaptive-transfers.md).

## Startup And Roles

`FILEBEAM_ROLE` selects one role per `light` container:

- `all` runs web, queue, scheduler, and normal startup preparation.
- `web`, `queue`, and `scheduler` run only that long-lived responsibility.
- `migrate` runs `php artisan migrate --force --no-interaction` once and exits.

`FILEBEAM_MIGRATE_ON_START` accepts `auto` (default), `true`, or `false`. `auto` prepares only the `all` role; `true` prepares every long-lived role; `false` disables startup preparation. Preparation and the explicit `migrate` role take an exclusive `flock` at `/data/app/installation/runtime.lock`.

For split web, queue, and scheduler deployments, run the `migrate` role as a one-shot deployment step before starting the long-lived roles, then use `FILEBEAM_MIGRATE_ON_START=false`.

Multiple web or queue nodes require a single shared staging directory at the same `FILEBEAM_STAGING_ROOT` path, with working cross-node `flock` semantics. The implementation has no owner routing; do not use node-local staging behind a load balancer. On shared hosting, use an ordinary private directory with correct runtime permissions, not a RAM mount or a web root.

During initial setup, Filebeam uses regular PHP requests. Once setup completes, it switches to persistent FrankenPHP/Octane workers. The installation token is written only to Docker logs while installation is pending; it is removed from `/data/config/.env` on completion. Treat initial logs as sensitive. No application, database, cache, or storage passwords are passed as process arguments or written to logs.

## TLS And Health

Set `FILEBEAM_TLS=proxy` (default) behind a trusted TLS proxy, or `FILEBEAM_TLS=auto` with `FILEBEAM_SERVER_NAME` or `APP_URL` for built-in Caddy TLS. With `auto`, HTTP on port 8080 serves `/up` and redirects other requests to HTTPS on 8443.

Before the first bootstrap, forwarded headers are deliberately ignored because `/data/config/.env` does not yet exist. Reach `/install` over direct HTTPS that securely reaches PHP, or over a loopback-only listener such as an SSH localhost tunnel; do not expose first-run setup through a TLS proxy. After bootstrap creates the environment file, configure `FILEBEAM_TRUSTED_PROXIES` with the proxy's IP address or CIDR and restart the container before using the proxy for setup.

If direct HTTPS or loopback access is unavailable, the container owner can create the pending environment and token without a browser request:

```sh
docker exec --user 10001:10001 <container> php -r 'require "/opt/filebeam/backend/vendor/autoload.php"; $app = require "/opt/filebeam/backend/bootstrap/app.php"; $app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap(); $app->make(App\Support\Installation\InstallationState::class)->bootstrap($app->make(App\Support\Installation\EnvironmentWriter::class));'
docker logs <container>
```

Read the installation token from the supervisor logs, configure the trusted proxy address or CIDR, restart the container, and complete setup through that proxy. Treat the token and initial logs as sensitive.

The health check verifies HTTP and, when automatic TLS is enabled, HTTPS with certificate validation. It also checks the selected role, database, cache, and scheduler state as applicable. Caddy's control API is loopback-only at `127.0.0.1:2019`, is not a public service, and logs only control errors; normal shutdown warnings are suppressed.

## Resource Controls

Set these environment variables to size the runtime:

- `FILEBEAM_PG_SHARED_BUFFERS` (Omnibus, default `128MB`) and `FILEBEAM_PG_MAX_CONNECTIONS` (default `100`).
- `FILEBEAM_VALKEY_MAXMEMORY` (Omnibus, default `256mb`).
- `FILEBEAM_WORKERS` (default `2`), `FILEBEAM_THREADS` (default `4`), and `MAX_REQUESTS` (default `500`).
- `CHUNK_MAX_SIZE`, up to 25,000,000 encrypted bytes.
- `FILEBEAM_MAX_REQUEST_BYTES` configures Caddy's request-body limit and defaults to `25000000`.

PHP's packaged `post_max_size` and `upload_max_filesize` default to `25000000`. Raising `FILEBEAM_MAX_REQUEST_BYTES` does not raise those PHP limits; configure PHP INI drop-ins when changing request limits. The application's maximum chunk size remains 25,000,000 bytes.

## Restricted Light Deployment

`light` is tested with a read-only root filesystem when writable directories are supplied. For rootless operation, pre-own `/data`, `/storage`, and their required subdirectories by UID/GID `10001:10001`; create a nonempty `/storage/primary` before mounting it to avoid Docker volume copy-up. Run it with:

```sh
docker run --user 10001:10001 --read-only --cap-drop ALL --security-opt no-new-privileges \
  --tmpfs /run:rw,nosuid,nodev,uid=10001,gid=10001,mode=0755,size=64m \
  --tmpfs /tmp:rw,nosuid,nodev,uid=10001,gid=10001,mode=1777,size=64m \
  -v filebeam-data:/data -v filebeam-storage:/storage \
  ghcr.io/vented-labs/filebeam/light:<version>
```

The `omnibus` image starts its initializer and supervisor as root, then runs Filebeam, PostgreSQL, and Valkey under distinct non-root daemon accounts. Keep an orchestrator stop grace period of at least 180 seconds.

## Host And Verification

Omnibus Valkey recommends `vm.overcommit_memory=1`. This remains the host administrator's decision: containers and local scripts do not change it. Set it through normal host configuration for production; ephemeral CI runners configure it before Omnibus checks. Warnings remain diagnostic; functional, health, shutdown, error, and fatal failures still fail the Docker tests. The Docker CI matrix builds and tests `light` and `omnibus` natively on AMD64 and ARM64.

From the repository root:

```sh
scripts/docker/check-context.sh
scripts/docker/build.sh --variant all
scripts/docker/test.sh --variant light --suite full
scripts/docker/test.sh --variant omnibus --suite full
```

Pass `--platform linux/amd64` or `--platform linux/arm64` to the build script when needed. These commands build locally and do not publish images.
