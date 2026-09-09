#!/usr/bin/env python3
"""Exercise the real binary in a PTY, including slow, single-chunk encrypted transfers."""
import fcntl
import base64
import http.server
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time
import zipfile

BINARY = str(Path(sys.argv[1]).resolve())
ID = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
CHUNK = 24_999_984
STATE = {}
ANSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")


class API(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def respond(self, status, data):
        body = json.dumps({"data": data}).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def body(self, slow=False):
        remaining = int(self.headers.get("Content-Length", "0"))
        data = bytearray()
        while remaining:
            block = self.rfile.read(min(65536, remaining))
            if not block:
                break
            data.extend(block)
            remaining -= len(block)
            if slow:
                time.sleep(0.012)
        return bytes(data)

    def do_GET(self):
        if self.path == "/api/v1/info":
            self.respond(200, dict(name="Filebeam", chunk_bytes=CHUNK, file_retention_hours=24,
                                   anonymous_uploads_enabled=True, enabled_drivers=["http"],
                                   maximum_transfer_bytes=1024**3, maximum_file_count=20))
        elif self.path == f"/api/v1/transfers/{ID}" and "manifest" in STATE:
            self.respond(200, dict(id=ID, protocol_version=1, chunk_bytes=CHUNK,
                                   encrypted_manifest=STATE["manifest"], items=STATE["items"]))
        elif "/chunks/" in self.path:
            body = STATE["chunks"][self.path]
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            try:
                for offset in range(0, len(body), 65536):
                    self.wfile.write(body[offset:offset + 65536])
                    self.wfile.flush()
                    time.sleep(0.01)
            except (BrokenPipeError, ConnectionResetError):
                pass
        else:
            self.respond(404, {})

    def do_POST(self):
        data = json.loads(self.body())
        if self.path == "/api/v1/transfers":
            items = [dict(id=f"01ARZ3NDEKTSV4RRFFQ69G5F{i:02}", position=i,
                          chunk_count=item["chunk_count"]) for i, item in enumerate(data["items"])]
            STATE.update(items=items, chunks={})
            STATE.pop("manifest", None)
            self.respond(201, dict(id=ID, share_url=f"/{ID}", chunk_bytes=CHUNK,
                                   items=items, upload_token="test-upload-token"))
        elif self.path.endswith("/complete"):
            STATE["manifest"] = data["encrypted_manifest"]
            self.respond(200, {})
        else:
            self.respond(404, {})

    def do_PUT(self):
        STATE["chunks"][self.path] = self.body(slow=True)
        try:
            self.respond(201, {})
        except (BrokenPipeError, ConnectionResetError):
            pass


class Terminal:
    def __init__(self, args, env, cwd, size=(100, 30), stdout_pipe=False):
        self.master, self.slave = pty.openpty()
        self.original = termios.tcgetattr(self.slave)
        self.resize(*size)

        def session():
            os.setsid()
            fcntl.ioctl(self.slave, termios.TIOCSCTTY, 0)

        self.process = subprocess.Popen([BINARY, *args], stdin=self.slave,
                                        stdout=subprocess.PIPE if stdout_pipe else self.slave,
                                        stderr=self.slave, cwd=cwd, env=env, preexec_fn=session)
        self.output = bytearray()

    def resize(self, width, height):
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))

    def pump(self, timeout=0.1):
        if select.select([self.master], [], [], timeout)[0]:
            try:
                self.output.extend(os.read(self.master, 1024 * 1024))
            except OSError:
                pass

    def until(self, text, timeout=10):
        deadline = time.monotonic() + timeout
        while text not in ANSI.sub(b"", self.output):
            self.pump()
            if self.process.poll() is not None or time.monotonic() > deadline:
                raise AssertionError(f"Missing {text!r}: {ANSI.sub(b'', self.output)[-1500:]!r}")

    def send(self, text):
        os.write(self.master, text)

    def finish(self, timeout=20):
        deadline = time.monotonic() + timeout
        while self.process.poll() is None:
            self.pump()
            if time.monotonic() > deadline:
                self.process.kill()
                raise AssertionError("Terminal command timed out")
        while select.select([self.master], [], [], 0.05)[0]:
            self.pump(0)
        restored = termios.tcgetattr(self.slave)
        assert restored == self.original, "Terminal mode was not restored"
        os.close(self.master)
        os.close(self.slave)
        return self.process.returncode, bytes(self.output)


def main():
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), API)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        source = root / "QA 日本語.bin"
        source.write_bytes(bytes(range(256)) * (32 * 1024))
        env = {**os.environ, "TERM": "xterm-256color", "COLORTERM": "truecolor", "LANG": "C.UTF-8",
               "FILEBEAM_INSTANCE": f"http://127.0.0.1:{server.server_port}", "FILEBEAM_HOME": str(root / "home")}
        upload = Terminal(["up", str(source)], env, root, stdout_pipe=True)
        upload.until(b"Uploading")
        code, output = upload.finish()
        assert code == 0, output
        link = upload.process.stdout.read().decode().strip()
        assert link.startswith(env["FILEBEAM_INSTANCE"] + "/") and "#k=v1." in link
        assert b"\x1b" not in link.encode()
        assert len(re.findall(rb"\b(?:[1-9]|[1-9][0-9])%", ANSI.sub(b"", output))) > 1, "Single chunk had no intermediate progress"
        assert b"\x1b[?1049h" not in output, "Inline command took over the terminal"
        assert b"\x1b[?25h" in output, "Inline command left cursor hidden"
        print("PASS: inline upload, granular single-chunk progress, clean stdout")

        download = Terminal(["down", link, "--output", str(root / "download")], env, root, stdout_pipe=True)
        download.until(b"Downloading")
        download.resize(60, 20)
        code, output = download.finish()
        assert code == 0, output
        assert (root / "download" / source.name).read_bytes() == source.read_bytes()
        assert b"Downloaded and verified" in ANSI.sub(b"", output)
        print("PASS: download, resize, byte-for-byte verification")

        keyless = Terminal(["down", link.split("#")[0], "--output", str(root / "keyless")], env, root)
        keyless.until(b"Decryption key:")
        key = link.split("#k=")[1]
        keyless.send(key.encode() + b"\r")
        code, output = keyless.finish()
        assert code == 0, output
        assert key.encode() not in output, "Secret input was echoed"
        assert (root / "keyless" / source.name).read_bytes() == source.read_bytes()
        print("PASS: masked key prompt and terminal restoration")

        plain_key = Terminal(["--plain", "down", link.split("#")[0], "--output", str(root / "plain-key")], env, root)
        plain_key.until(b"Decryption key:")
        plain_key.send(key.encode() + b"\r")
        code, output = plain_key.finish()
        assert code == 0, output
        assert b"\x1b" not in output and key.encode() not in output
        print("PASS: plain secret prompt emits no escape sequences")

        cancelled = Terminal(["down", link, "--output", str(root / "cancelled")], env, root)
        cancelled.until(b"Downloading")
        cancelled.process.send_signal(signal.SIGINT)
        code, output = cancelled.finish()
        assert code != 0
        assert not list((root / "cancelled").glob("*")), "Cancellation left a partial file"
        assert b"cancelled" in ANSI.sub(b"", output).lower()
        print("PASS: cancellation cleans temporary download and cursor")

        plain = subprocess.run([BINARY, "--plain", "down", link, "--output", str(root / "plain")],
                               cwd=root, env=env, capture_output=True, timeout=20)
        assert plain.returncode == 0, plain.stderr
        assert b"\x1b" not in plain.stdout + plain.stderr
        assert (root / "plain" / source.name).read_bytes() == source.read_bytes()
        print("PASS: plain and redirected output")

        screen = Terminal([], env, root)
        screen.until(b"Choose what to share")
        screen.send(b"/QA\r ")
        screen.pump(0.3)
        screen.send(b"u")
        screen.until(b"Uploading")
        screen.until(b"Your encrypted link is ready")
        screen.send(b"c")
        screen.until(b"Copy requested")
        screen.send(b"q")
        code, output = screen.finish()
        assert code == 0, output
        assert b"\x1b[?1049h" in output and b"\x1b[?1049l" in output
        targets = re.findall(rb"\x1b]8;;(http[^\x1b]+)\x1b\\", output)
        assert targets and b"#k=v1." in targets[0], "TUI hyperlink lost the key fragment"
        clipboard = re.search(rb"\x1b]52;c;([^\x07]+)\x07", output)
        assert clipboard and base64.b64decode(clipboard[1]) == targets[0], "Copy did not contain the full link"
        for removed in [b"private by design", b"Delivered with a link.", b"Sharing something good.", b"Encrypting on your device"]:
            assert removed not in ANSI.sub(b"", output)
        tui_link = targets[0]
        print("PASS: full-screen search, selection, upload, receipt and cleanup")
        print("PASS: full hyperlink/clipboard targets and requested copy removals")

        screen = Terminal([], env, root, size=(80, 24))
        screen.until(b"Choose what to share")
        screen.send(b"d")
        screen.until(b"A link is all you need")
        screen.send(b"\x1b[200~" + link.encode() + b"\x1b[201~")
        screen.send(b"\r\x15" + str(root / "tui-download").encode() + b"\r")
        # The preceding upload replaced the fixture, with its own encryption key.
        screen.until(b"attention")
        screen.send(b"q")
        code, output = screen.finish()
        assert code == 0, output
        print("PASS: receive form, bracketed paste and error receipt")

        receiver = Terminal([], env, root, size=(80, 24))
        receiver.until(b"Choose what to share")
        receiver.send(b"d")
        receiver.until(b"A link is all you need")
        receiver.send(b"\x1b[200~" + tui_link + b"\x1b[201~")
        receiver.send(b"\r\x15" + str(root / "tui-good").encode() + b"\r")
        receiver.until(b"Downloaded and verified")
        receiver.send(b"q")
        code, output = receiver.finish()
        assert code == 0, output
        assert (root / "tui-good" / source.name).read_bytes() == source.read_bytes()
        idle = Terminal([], env, root)
        idle.until(b"Choose what to share")
        idle.process.send_signal(signal.SIGTERM)
        code, output = idle.finish()
        assert code == 0 and b"\x1b[?1049l" in output
        print("PASS: full-screen receive success and SIGTERM restoration")

        bundle = root / "Folder QA"
        (bundle / "nested").mkdir(parents=True)
        (bundle / "empty").mkdir()
        (bundle / "readme.txt").write_text("root document")
        (bundle / "nested" / "readme.txt").write_text("nested document")
        (bundle / ".hidden.txt").write_text("hidden document")
        (bundle / "nested" / "cycle").symlink_to(bundle)

        def command(*args):
            return subprocess.run([BINARY, "--plain", *map(str, args)], cwd=root, env=env,
                                  capture_output=True, stdin=subprocess.DEVNULL, timeout=20)

        missing = command("up", bundle)
        assert missing.returncode != 0 and b"--zip or --individual" in missing.stderr
        conflict = command("up", bundle, "--zip", "--individual")
        assert conflict.returncode != 0

        zipped = command("up", bundle, "--zip")
        assert zipped.returncode == 0, zipped.stderr
        assert len(STATE["items"]) == 1, "ZIP must count as one file"
        saved = command("down", zipped.stdout.decode().strip(), "--output", root / "zipped")
        assert saved.returncode == 0, saved.stderr
        with zipfile.ZipFile(root / "zipped" / "Folder QA.zip") as archive:
            assert archive.read("Folder QA/readme.txt") == b"root document"
            assert archive.read("Folder QA/nested/readme.txt") == b"nested document"
            assert archive.read("Folder QA/.hidden.txt") == b"hidden document"
            assert "Folder QA/empty/" in archive.namelist()
            assert not any("cycle" in name for name in archive.namelist())
        print("PASS: ZIP directory roundtrip, nested/hidden files, empty folders, symlink loop")

        individual = command("up", bundle, "--individual")
        assert individual.returncode == 0, individual.stderr
        assert len(STATE["items"]) == 3
        saved = command("down", individual.stdout.decode().strip(), "--output", root / "individual")
        assert saved.returncode == 0, saved.stderr
        outputs = list((root / "individual").iterdir())
        assert len(outputs) == 3
        assert {file.read_text() for file in outputs} == {"root document", "nested document", "hidden document"}
        assert (root / "individual" / "readme (2).txt").exists()
        print("PASS: individual directory roundtrip and duplicate filename disambiguation")

        for index in range(22):
            (bundle / f"file-{index}.txt").write_text("file limit")
        limited = command("up", bundle, "--individual")
        assert limited.returncode != 0 and b"25 individual files" in limited.stderr and b"--zip" in limited.stderr
        choice = Terminal(["up", str(bundle)], env, root, stdout_pipe=True)
        choice.until(b"ZIP and send")
        choice.send(b"1\r")
        code, output = choice.finish()
        assert code == 0, output
        assert len(STATE["items"]) == 1
        print("PASS: directory prompt, explicit flags, and file-limit preflight")

        cli_link = Terminal(["up", str(bundle), "--zip"], env, root, size=(60, 20))
        code, output = cli_link.finish()
        assert code == 0, output
        targets = re.findall(rb"\x1b]8;;(http[^\x1b]+)\x1b\\", output)
        assert targets and b"#k=v1." in targets[-1]
        assert len(targets[-1]) > 60, "Fixture should exercise a wrapped link"
        print("PASS: wrapped inline link carries its complete click target")
    server.shutdown()


if __name__ == "__main__":
    main()
