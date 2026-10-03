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
    parser.add_argument("--desktop", type=Path)
    parser.add_argument("--services", type=Path)
    parser.add_argument("--background", type=Path)
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--browser-test", action="append", default=[])
    parser.add_argument("--webrtc", action="store_true")
    args = parser.parse_args()
    for path in [args.desktop, args.services, args.background, args.cli]:
        if path is not None and not path.is_file():
            parser.error(f"missing native binary: {path}")
    artifacts = ROOT / ".filebeam" / "test-results"
    artifacts.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="native-backend-", dir=artifacts) as directory:
        state = Path(directory)
        for path in ["config", "cache", "public", "app/logs", "app/framework/views", "app/framework/cache/data"]:
            (state / path).mkdir(parents=True, exist_ok=True)
        # Isolate production assets from a developer's Vite hot-file and SSR server.
        public = state / "public"
        for asset in (ROOT / "backend/public").iterdir():
            if asset.name not in ("hot", "index.php", "frankenphp-worker.php"):
                (public / asset.name).symlink_to(asset, target_is_directory=asset.is_dir())
        (public / "index.php").write_text("""<?php
declare(strict_types=1);
define('LARAVEL_START', microtime(true));
require getenv('FILEBEAM_TEST_BACKEND').'/vendor/autoload.php';
$app = require getenv('FILEBEAM_TEST_BACKEND').'/bootstrap/app.php';
$app->usePublicPath(__DIR__);
$app->afterBootstrapping(Illuminate\\Foundation\\Bootstrap\\LoadConfiguration::class, static function ($app) {
    $app['config']->set('inertia.ssr.enabled', false);
});
$app->handleRequest(Illuminate\\Http\\Request::capture());
""")
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
                "FILEBEAM_ENABLED_TRANSFER_DRIVERS": '["http","webrtc"]' if args.webrtc else '["http"]', "FILEBEAM_DEFAULT_TRANSFER_DRIVER": "http",
                "FILEBEAM_WEBRTC_ICE_SERVERS": "[]", "FILEBEAM_WEBRTC_TESTS": "1" if args.webrtc else "0",
               "FILEBEAM_ANONYMOUS_UPLOADS_ENABLED": "true", "FILEBEAM_REGISTRATION_ENABLED": "true",
               "FILEBEAM_USERNAME_ROUTING_ENABLED": "true", "CHUNK_MAX_SIZE": "65552",
               "FILEBEAM_CREATIONS_PER_HOUR": "300", "FILEBEAM_WRITES_PER_MINUTE": "3000",
               "FILEBEAM_READS_PER_MINUTE": "3000", "FILEBEAM_ACCEPTANCE_INSTANCE": origin,
               "FILEBEAM_ACCEPTANCE_RESULTS": str(state / "native"), "NO_COLOR": "1",
                "FILEBEAM_TEST_BACKEND": str(ROOT / "backend"),
                "FILEBEAM_NATIVE_ACCEPTANCE_HISTORY": "1"}
        with (state / "server.log").open("w+") as log:
            try:
                run(["php", "artisan", "migrate", "--seed", "--force", "--no-interaction"], env, stdout=log, stderr=log)
            except subprocess.CalledProcessError as error:
                log.seek(0)
                raise RuntimeError("backend migration failed:\n" + log.read()) from error
            server = subprocess.Popen(["php", "-S", f"127.0.0.1:{port}",
                                       str(ROOT / "backend/vendor/laravel/framework/src/Illuminate/Foundation/resources/server.php")],
                                      cwd=public, env=env, stdout=log, stderr=log)
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
                if args.desktop:
                    run([str(args.desktop.resolve()), origin], env)
                if args.services:
                    run([str(args.services.resolve()), "all"], env)
                if args.background:
                    run([str(args.background.resolve())], {**env, "FILEBEAM_ACCEPTANCE_ALLOW_HTTP": "1"})
                if args.cli:
                    home = state / "cli-home"
                    home.mkdir()
                    (home / "config.toml").write_text(
                        f'check_updates = false\n[server]\nurl = "{origin}"\n')
                    source = state / "cli-source.bin"
                    source.write_bytes(bytes(range(251)) * 1000)
                    note = state / "attachment.md"
                    note.write_text("# PRIVATE_ATTACHED_NOTE\nUnicode 🦀\t\n", encoding="utf-8")
                    cli = [str(args.cli.resolve()), "--home", str(home), "--plain"]
                    uploaded = run([*cli, "up", str(source), "--note-file", str(note), "--note-title", "Read first", "--note-language", "markdown"], env, capture_output=True, text=True)
                    destination = state / "cli-download"
                    note_output = state / "received-note.md"
                    run([*cli, "down", uploaded.stdout.strip(), "--output", str(destination), "--note-output", str(note_output)], env)
                    assert (destination / source.name).read_bytes() == source.read_bytes()
                    assert note_output.read_bytes() == note.read_bytes()
                    repeated = subprocess.run([*cli, "down", uploaded.stdout.strip(), "--output", str(destination), "--note-output", str(note_output)], cwd=ROOT / "backend", env=env, capture_output=True, timeout=60)
                    assert repeated.returncode != 0
                    assert note_output.read_bytes() == note.read_bytes()
                    stdin_note = "Exact stdin note 🦀\n\t"
                    stdin_upload = run([*cli, "up", str(source), "--note-file", "-"], env, input=stdin_note, capture_output=True, text=True)
                    stdout_note = run([*cli, "down", stdin_upload.stdout.strip(), "--output", str(state / "stdin-download"), "--note-output", "-"], env, capture_output=True, text=True)
                    assert stdout_note.stdout == stdin_note
                    username = f"cli_note_{port}"
                    run([*cli, "account", "register", username, "cli-note@example.test", "--password-stdin"], env, input="CLI8!Attachment", capture_output=True, text=True)
                    run([*cli, "account", "key-setup", "--custody", "self"], env, capture_output=True, text=True)
                    run([*cli, "up", str(source), "--username", username, "--note-file", str(note)], env, capture_output=True, text=True)
                    inbox = run([*cli, "inbox", "list"], env, capture_output=True, text=True)
                    delivery_id = inbox.stdout.strip().split("\n")[0].split("\t")[0]
                    inbox_note = state / "inbox-note.md"
                    run([*cli, "inbox", "download", delivery_id, "--output", str(state / "inbox-download"), "--note-output", str(inbox_note)], env, capture_output=True, text=True)
                    assert inbox_note.read_bytes() == note.read_bytes()
                    assert "auto_update = false" in (home / "config.toml").read_text()
                    print("PASS CLI migrated-config upload/download hash and attached note")
                    run([*cli, "account", "logout"], env, capture_output=True, text=True)
                    run([*cli, "account", "register", f"history_{port}", f"history-{port}@example.test", "--password-stdin"], env,
                        input="Native8!History\n", capture_output=True, text=True)
                    owned = run([*cli, "up", str(source), "--retention-hours", "1"], env, capture_output=True, text=True)
                    identifier = owned.stdout.strip().split("#")[0].rsplit("/", 1)[-1]
                    history = run([*cli, "history", "list"], env, capture_output=True, text=True)
                    assert identifier in history.stdout
                    extended = run([*cli, "history", "extend", identifier, "--retention-hours", "2"], env, capture_output=True, text=True)
                    assert identifier in extended.stdout
                    run([*cli, "history", "delete", identifier], env, capture_output=True, text=True)
                    archived = run([*cli, "history", "list", "--status", "deleted"], env, capture_output=True, text=True)
                    assert identifier in archived.stdout
                    run([*cli, "account", "logout"], env, capture_output=True, text=True)
                    denied = subprocess.run([*cli, "history", "list"], cwd=ROOT / "backend", env=env, capture_output=True, text=True, timeout=30)
                    assert denied.returncode != 0
                    print("PASS CLI authenticated upload/history/extend/delete/logout")
                if args.browser_test:
                    subprocess.run([
                        "npx", "playwright", "test", *args.browser_test, "--workers=1",
                        "--output=" + str(artifacts / "native-backend-browser"),
                     ], cwd=ROOT, env={**env, "BASE_URL": origin, **({"BROWSER_TEST_CLI_BINARY": str(args.cli.resolve())} if args.cli else {})}, check=True, timeout=450)
            finally:
                server.terminate()
                server.wait(timeout=10)
    print("PASS native clients against Laravel backend")


if __name__ == "__main__":
    main()
