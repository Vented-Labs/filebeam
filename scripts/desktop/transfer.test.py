#!/usr/bin/env python3
"""Runs the headless DesktopClient example against the shared CLI HTTP fixture."""
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import threading
import http.server
import re
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("transfer_fixture", ROOT / "scripts/cli/transfer.test.py")
fixture = importlib.util.module_from_spec(spec)
old_argv = sys.argv[:]
sys.argv = ["transfer.test.py", "/bin/true"]
spec.loader.exec_module(fixture)
sys.argv = old_argv

class DesktopAPI(fixture.API):
    """Add public inspect fields while retaining the CLI fixture's wire protocol."""
    def json(self, status, data, headers=None):
        if re.fullmatch(r"/api/v1/transfers/[^/]+", self.path) and isinstance(data, dict):
            data = {"kind": "files", "status": "complete", "driver": "http", **data}
        return super().json(status, data, headers)

    def do_POST(self):
        if self.path == "/api/v1/transfers" and getattr(fixture.STATE, "pause_at", None) == fixture.STATE.sequence + 1:
            fixture.STATE.next_mode = "upload-stall"
        return super().do_POST()

def run(pause=False):
    fixture.STATE = fixture.State()
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), DesktopAPI)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    env = {**os.environ, "FILEBEAM_DISABLE_UPDATE_CHECK": "1"}
    if pause:
        env["FILEBEAM_NATIVE_ACCEPTANCE_PAUSE"] = "1"
        fixture.STATE.pause_at = 4
        marker = Path(tempfile.mkdtemp(prefix="filebeam-pause-")) / "ack"
        ready = marker.with_name("ready")
        env["FILEBEAM_NATIVE_ACCEPTANCE_PAUSE_ACK"] = str(marker)
        env["FILEBEAM_NATIVE_ACCEPTANCE_PAUSE_READY"] = str(ready)
        fixture.STATE.upload_stall_marker = ready
        def release_after_pause():
            deadline = time.monotonic() + 90
            while time.monotonic() < deadline:
                if marker.exists():
                    for transfer in fixture.STATE.transfers.values():
                        if transfer["mode"] == "upload-stall":
                            transfer["gate"].set()
                            return
                time.sleep(.02)
        threading.Thread(target=release_after_pause, daemon=True).start()
    command = [str(BINARY), f"http://127.0.0.1:{server.server_port}"]
    result = subprocess.run(command, cwd=ROOT, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=180)
    server.shutdown(); thread.join()
    if result.returncode:
        raise SystemExit(result.stderr)
    print(result.stdout, end="")

subprocess.run([
    "cargo", "build", "--locked", "--manifest-path", "desktop/Cargo.toml",
    "--features", "headless-acceptance", "--example", "native_acceptance",
], cwd=ROOT, check=True)
BINARY = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "desktop/target")) / "debug/examples/native_acceptance"
run()
run(True)
