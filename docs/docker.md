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

Multiple web or queue nodes require shared staging at the same `FILEBEAM_STAGING_ROOT` path, with working cross-node `flock`. Node-local staging behind a load balancer is unsupported.

During initial setup, Filebeam uses regular PHP requests. Once setup completes, it switches to persistent FrankenPHP/Octane workers. The installation token is written only to Docker logs while installation is pending; it is removed from `/data/config/.env` on completion. Treat initial logs as sensitive. No application, database, cache, or storage passwords are passed as process arguments or written to logs.

Set `FILEBEAM_BOOTSTRAP_ON_START=true` to initialize pending setup locally before the web server starts. This is useful for orchestrators and trusted TLS proxies. The default is `false`. Bootstrap preserves pending and completed installations, including their application keys; it refuses inconsistent state or an existing SQLite database without its matching configuration. It does not create an administrator or complete installation.

## TLS And Health

Choose a TLS mode:

- `FILEBEAM_TLS=proxy` (default): HTTP on container port 8080 behind a trusted HTTPS proxy. Set `FILEBEAM_TRUSTED_PROXIES` to the proxy's address or CIDR.
- `FILEBEAM_TLS=certificate`: direct HTTPS on container port 8443 using mounted PEM files at `FILEBEAM_TLS_CERT_FILE` and `FILEBEAM_TLS_KEY_FILE`. Include intermediate certificates in the certificate file. Both files must be readable by UID/GID `10001:10001`. Replace the mounted files and restart the container to renew them.
- `FILEBEAM_TLS=auto`: direct HTTPS on container port 8443 with Caddy-managed certificates. Public certificate issuance requires DNS and ACME challenge traffic on external ports 80/443 to reach Caddy's corresponding container listeners. High published ports alone do not satisfy ACME challenges.

For direct HTTPS, set `APP_URL` to the root HTTPS URL users reach, including any non-default published port. Its hostname must match `FILEBEAM_SERVER_NAME` when that variable is supplied. HTTP on container port 8080 serves `/up` and redirects other requests to the complete `APP_URL`. The HTTP listener need not be published when using a supplied certificate.

Before the first bootstrap, forwarded headers are deliberately ignored because `/data/config/.env` does not yet exist. For proxy deployments, set `FILEBEAM_BOOTSTRAP_ON_START=true` and the explicit `FILEBEAM_TRUSTED_PROXIES` before starting the container, then read the installation token from its logs and complete `/install` through the HTTPS proxy. Without startup bootstrap, use direct HTTPS or a loopback-only listener such as an SSH localhost tunnel for the first bootstrap.

If direct HTTPS or loopback access is unavailable, the container owner can create the pending environment and token without a browser request:

```sh
docker exec --user 10001:10001 <container> php artisan filebeam:installation:bootstrap --no-interaction
docker logs <container>
```

Read the installation token from the supervisor logs, configure the trusted proxy address or CIDR, restart the container, and complete setup through that proxy. Treat the token and initial logs as sensitive.

The health check verifies HTTP and, in direct TLS modes, HTTPS with certificate validation. Supplied certificates are added to its trust bundle; set `FILEBEAM_TLS_CA_FILE` to a separate PEM CA bundle when needed. It also checks the selected role, database, cache, and scheduler state as applicable. Caddy's control API is loopback-only at `127.0.0.1:2019`, is not a public service, and logs only control errors; normal shutdown warnings are suppressed.

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

Omnibus Valkey recommends `vm.overcommit_memory=1`; configure it on the host. CI builds and tests both image variants on AMD64 and ARM64.

From the repository root:

```sh
scripts/docker/check-context.sh
scripts/docker/build.sh --variant all
scripts/docker/test.sh --variant light --suite full
scripts/docker/test.sh --variant omnibus --suite full
```

Pass `--platform linux/amd64` or `--platform linux/arm64` to the build script when needed. These commands build locally and do not publish images.
