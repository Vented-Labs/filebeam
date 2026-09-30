#!/usr/bin/env python3
"""Exercise native clients against an isolated, migrated Laravel backend."""
import argparse
import base64
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[2]


def run(arguments, env, **kwargs):
    return subprocess.run(arguments, cwd=ROOT / "backend", env=env,
                          check=True, timeout=180, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--desktop", required=True, type=Path)
    parser.add_argument("--services", required=True, type=Path)
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--browser-test", action="append", default=[])
    args = parser.parse_args()
    for path in [args.desktop, args.services, args.cli]:
        if path is not None and not path.is_file():
            parser.error(f"missing native binary: {path}")
    artifacts = ROOT / ".filebeam" / "test-results"
    artifacts.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="native-backend-", dir=artifacts) as directory:
        state = Path(directory)
        for path in ["config", "cache", "app/logs", "app/framework/views", "app/framework/cache/data"]:
            (state / path).mkdir(parents=True, exist_ok=True)
        (state / "config/.env").touch()
        (state / "database.sqlite").touch()
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        origin = f"http://127.0.0.1:{port}"
        env = {**os.environ,
               "APP_ENV": "testing", "APP_DEBUG": "false", "APP_URL": origin,
               "APP_KEY": "base64:" + base64.b64encode(os.urandom(32)).decode(),
               "APP_CONFIG_CACHE": str(state / "config-cache.php"),
               "APP_ROUTES_CACHE": str(state / "cache/routes.php"),
               "APP_EVENTS_CACHE": str(state / "cache/events.php"),
               "APP_SERVICES_CACHE": str(state / "cache/services.php"),
               "APP_PACKAGES_CACHE": str(state / "cache/packages.php"),
               "VIEW_COMPILED_PATH": str(state / "app/framework/views"),
               "DB_CONNECTION": "sqlite", "DB_DATABASE": str(state / "database.sqlite"), "DB_URL": "",
               "CACHE_STORE": "database", "SESSION_DRIVER": "cookie", "QUEUE_CONNECTION": "sync",
               "MAIL_MAILER": "log", "LOG_CHANNEL": "single",
               "FILEBEAM_CONTAINER": "true", "FILEBEAM_VARIANT": "light", "FILEBEAM_DATA_DIR": str(state),
               "FILEBEAM_FILESYSTEMS": "transfers", "FILEBEAM_FILESTORE_ROOT": str(state / "filestore"),
               "FILEBEAM_STAGING_ROOT": str(state / "staging"),
               "FILEBEAM_ENABLED_TRANSFER_DRIVERS": '["http"]', "FILEBEAM_DEFAULT_TRANSFER_DRIVER": "http",
               "FILEBEAM_ANONYMOUS_UPLOADS_ENABLED": "true", "FILEBEAM_REGISTRATION_ENABLED": "true",
               "FILEBEAM_USERNAME_ROUTING_ENABLED": "true", "CHUNK_MAX_SIZE": "65552",
               "FILEBEAM_CREATIONS_PER_HOUR": "300", "FILEBEAM_WRITES_PER_MINUTE": "3000",
               "FILEBEAM_READS_PER_MINUTE": "3000", "FILEBEAM_ACCEPTANCE_INSTANCE": origin,
               "FILEBEAM_ACCEPTANCE_RESULTS": str(state / "native"), "NO_COLOR": "1"}
        with (state / "server.log").open("w+") as log:
            try:
                run(["php", "artisan", "migrate", "--seed", "--force", "--no-interaction"], env, stdout=log, stderr=log)
            except subprocess.CalledProcessError as error:
                log.seek(0)
                raise RuntimeError("backend migration failed:\n" + log.read()) from error
            server = subprocess.Popen(["php", "-S", f"127.0.0.1:{port}",
                                       str(ROOT / "backend/vendor/laravel/framework/src/Illuminate/Foundation/resources/server.php")],
                                      cwd=ROOT / "backend/public", env=env, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 30
                while True:
                    try:
                        with urllib.request.urlopen(origin + "/api/v1/info", timeout=1) as response:
                            info = json.load(response)["data"]
                        assert info["default_driver"] == "http"
                        break
                    except (urllib.error.URLError, TimeoutError):
                        if server.poll() is not None or time.monotonic() > deadline:
                            log.seek(0)
                            raise RuntimeError("backend did not start:\n" + log.read())
                        time.sleep(.1)
                run([str(args.desktop.resolve()), origin], env)
                run([str(args.services.resolve()), "all"], env)
                if args.cli:
                    home = state / "cli-home"
                    home.mkdir()
                    (home / "config.toml").write_text(
                        f'check_updates = false\n[server]\nurl = "{origin}"\n')
                    source = state / "cli-source.bin"
                    source.write_bytes(bytes(range(251)) * 1000)
                    cli = [str(args.cli.resolve()), "--home", str(home), "--plain"]
                    uploaded = run([*cli, "up", str(source)], env, capture_output=True, text=True)
                    destination = state / "cli-download"
                    run([*cli, "down", uploaded.stdout.strip(), "--output", str(destination)], env)
                    assert (destination / source.name).read_bytes() == source.read_bytes()
                    assert "auto_update = false" in (home / "config.toml").read_text()
                    print("PASS CLI migrated-config upload/download hash")
                if args.browser_test:
                    subprocess.run([
                        "npx", "playwright", "test", *args.browser_test, "--workers=1",
                        "--output=" + str(artifacts / "native-backend-browser"),
                    ], cwd=ROOT, env={**env, "BASE_URL": origin}, check=True, timeout=180)
            finally:
                server.terminate()
                server.wait(timeout=10)
    print("PASS native clients against Laravel backend")


if __name__ == "__main__":
    main()
