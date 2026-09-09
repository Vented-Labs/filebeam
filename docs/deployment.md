# Deployment

## Install

Use a Filebeam release package with PHP 8.5+, required Laravel extensions, and SQLite, MySQL/MariaDB, or PostgreSQL. Configure HTTPS before exposing the service. Point the web server document root to `backend/public` and route application requests to `public/index.php`; never serve the package root or permit `.env` downloads. HTTP is suitable only for local setup.

For the official container images and their service layout, see [Docker deployment](docker.md).

Host-managed Octane or FrankenPHP is not a supported package deployment. Use the official Docker images when running Filebeam with its containerized FrankenPHP runtime; package deployments use the PHP web-server and cron model documented here.

On a fresh package, leave `backend/.env` absent. Do not copy `backend/.env.example`, run migrations, or create an application key before installation. Give the PHP runtime user temporary write access to `backend/`, `backend/storage`, `backend/bootstrap/cache`, and `backend/database` when using SQLite.

For the first bootstrap, open `/install` over direct HTTPS that securely reaches PHP, or use loopback-only access such as an SSH localhost tunnel. Forwarded headers are ignored until `backend/.env` exists, so do not expose first-run setup through a TLS proxy. Generate an installation token, then read `FILEBEAM_INSTALL_TOKEN` from server-side `backend/.env` and enter it in the installer. The browser does not display or download the token. Complete the database, cache, application URL, access, storage, chunk-size, automatic-update, and administrator steps. Database connections can use TCP or Unix sockets: MySQL/MariaDB take a socket file, while PostgreSQL takes a socket directory and port. Setup defaults to cookie sessions and finishes by running `php artisan optimize`. After installation, `/install` returns 404.

Cache defaults to local files. Choose Redis in the database/cache step to configure TCP, TLS, or a Unix socket, optional username/password, database number, and a unique key prefix. Enable the PhpRedis extension in both web PHP and the PHP binary used by cron. The cache check verifies reads, writes, counters, deletion, and exclusive locks without flushing existing data. It runs again before setup is applied. Cache data and locks use the selected Redis database (default `0`); cookie sessions and cron-processed database jobs remain unchanged.

Remove PHP write access to application code and `.env` after setup, while retaining runtime access to storage and the selected database/filesystems. If TLS terminates at a proxy, after the first bootstrap creates `backend/.env`, set `FILEBEAM_TRUSTED_PROXIES` to that proxy's IP address or CIDR and restart PHP before continuing setup through the proxy.

Automatic updates are opt-in (`FILEBEAM_AUTO_UPDATES_ENABLED=false` by default). When enabled, the daily scheduled check installs available compatible signed packages through the existing updater. Keep the package writable by the updater's runtime user, reserve sufficient disk quota for a database backup, and allow enough cron runtime for backup and update work. This updater supports only a single-node package filesystem. Container images remain immutable: deploy a new image instead of enabling package self-updates. After manually changing environment settings, rebuild cached configuration with `php artisan optimize`.

Release publishers must set `R2_ENDPOINT_URL` to the HTTPS Cloudflare R2 account API root (for example, `https://<account-id>.r2.cloudflarestorage.com`) and `R2_BUCKET` to the bucket name. A configured endpoint ending in exactly `/<R2_BUCKET>` (optionally with a trailing slash) is normalized to the account root; public download URLs and other endpoint paths are rejected. Publication writes bucket-relative keys such as `index.json` and `versions/v0.1.0/filebeam-v0.1.0.zip`.

For PostgreSQL, install both `pg_dump` and `pg_restore` in the updater's CLI environment. Their major version must exactly match the PostgreSQL server major version; patch versions need not match. For example, PostgreSQL 16.15 works with PostgreSQL 16 client tools, but PostgreSQL 17 tools fail the preflight. You can check an installed tool with `pg_dump --version`.

## Runtime

Shared hosting needs just one cron entry, configured after installation using the same PHP runtime user and environment. No persistent queue worker, Supervisor, or Redis is required:

```cron
* * * * * cd /path/to/filebeam/backend && php artisan schedule:run >> /dev/null 2>&1
```

The scheduler performs transfer cleanup, checks releases, and processes pending notifications and file deletions. With the default `QUEUE_CONNECTION=database` and `FILEBEAM_CRON_QUEUE_ENABLED=true`, it starts a short-lived job processor every minute, stops when empty or after 25 jobs / a 50-second processing budget, and prevents overlapping batches. Jobs and retries remain in the database until a later cron run; notifications and physical file deletion may therefore be delayed by a minute or more. Keep the default file cache, or another persistent lock-capable cache, for overlap protection.

The processing budget is checked between jobs, so a single slow job may take longer. The 60-second per-job timeout requires PHP's PCNTL extension and must remain shorter than the database queue's `retry_after` (90 seconds by default). Allow enough CLI runtime on the hosting plan for mail and storage operations. If the host forcibly terminates a scheduler, its batch lock expires after ten minutes; `php artisan schedule:clear-cache` clears a stale lock after confirming no batch is still running.

The admin panel can flag a missing scheduler heartbeat; restore cron before expecting background work to catch up. Configure mail separately if the deployment needs account emails or notifications. Failed jobs can be inspected with `php artisan queue:failed` and retried with `php artisan queue:retry <id>`.

Dedicated servers may optionally run a persistent `php artisan queue:work` instead. Set `FILEBEAM_CRON_QUEUE_ENABLED=false` and rebuild configuration with `php artisan optimize` when doing so. The cron processor does not consume Redis or other queue connections; those deployments retain their own worker setup.

Set every PHP server, proxy, and storage-provider request limit to accommodate `CHUNK_MAX_SIZE`. Its default is 25,000,000 encrypted bytes; authenticated encryption adds a 16-byte tag to each plaintext chunk. Storage is private. Keep filesystem definitions and credentials available for stores containing old chunks.

Adaptive uploads also require private, disk-backed staging. By default this is `storage/app/transfer-staging`; set `FILEBEAM_STAGING_ROOT` to a persistent, private local directory when application storage is not persistent. Do not use RAM as the default, a web-served path, or S3/object storage for staging. The runtime user must be able to create files and use `flock` in the directory. See [Adaptive transfers](adaptive-transfers.md) for capacity, multi-instance, and cleanup requirements.

See [WebRTC transfers](webrtc.md) before enabling the storage-free WebRTC driver. It covers STUN/TURN credentials, shared-cache signaling requirements, peer/relay warnings, limits, and the HTTP fallback behavior.

Run the scheduler cron entry above in every deployment. Its 15-minute transfer-pruning task removes expired staging reservations and files; this cleanup is mandatory for staging capacity to recover. After deploying a release that includes database migrations, run the routine deployment migration manually before serving the release:

```sh
cd /path/to/filebeam/backend
php artisan migrate --force
```

## Backup And Recovery

Back up these items together before upgrades or maintenance:

- `backend/.env`, including `APP_KEY` and installation state.
- `backend/storage/app/installation`.
- The selected database.
- All configured ciphertext storage, including local transfer storage or S3-compatible buckets.

Restore the matching set together. Do not replace `.env` or installation state during an upgrade. If file permissions are tightened after installation, temporarily restore the required application write access only for the update, then tighten them again.

The packaged updater is invoked from the package root:

```sh
php update.php
php update.php --status
php update.php --check-backup
php update.php --recover
```

`php update.php --check-backup` does not run migrations or modify the database. Use it before enabling automatic updates to verify connectivity and matching PostgreSQL client versions. Backup creation still fails closed if dumping, archive validation, or storage operations fail. Configure PostgreSQL TLS through `DB_SSLMODE` and, when required, `DB_SSLROOTCERT`, `DB_SSLCERT`, and `DB_SSLKEY`; Laravel and the updater use the same settings.

Before changing code or running migrations, the updater enters maintenance mode, signals queue workers to finish, and obtains an exclusive activity gate. It waits up to 120 seconds for web requests, cron activity, and jobs holding shared locks to finish, then creates the database backup. It only proceeds to code replacement and migrations after that backup succeeds. When first deploying this activity-gate release, drain and restart any old persistent workers before enabling updates; old processes cannot retroactively participate in the gate. Do not run external database writers or manual maintenance commands during an update.

For PostgreSQL, each updater backup is a native custom-format archive at `.filebeam/database-backups/database-*.dump`. The updater creates the backup directory with mode `0700` and archives with mode `0600`. It validates the PostgreSQL custom archive (`PGDMP` header), runs `pg_restore --list`, and fully parses it with `pg_restore --file=/dev/null` before continuing. Database credentials are provided through a private, ephemeral `PGPASSFILE`; passwords are never placed on the command line.

Use `--recover` only for an interrupted update before migrations begin. Once migrations begin, `--recover` refuses to continue: restore the database backup and deploy manually. The updater never automatically performs a destructive restore.

### PostgreSQL Manual Restore

Keep maintenance mode enabled until the restored application has been verified. Restore a PostgreSQL archive into an existing, empty database allocated for the recovery. This accommodates shared-hosting accounts without `CREATEDB` privileges and avoids acting on the active database. Configure the restored application's database name and credentials in `backend/.env` to match that allocated database.

Use a private `PGPASSFILE` containing the connection credentials, or omit `PGPASSFILE` and enter the password interactively. Do not put a password on the command line:

```sh
PGPASSFILE=/path/to/private/pgpass pg_restore \
    --no-owner \
    --no-acl \
    --exit-on-error \
    --host=postgres.example.com \
    --port=5432 \
    --username=filebeam \
    --dbname=filebeam_restore \
    /path/to/private/database-backup.dump
```

This backup contains the database only. Restore the matching encrypted ciphertext storage separately, and retain the matching `backend/.env` and installation state separately; the database archive cannot recover them.
