#!/usr/bin/env python3
"""Fault/restart integration tests for the real `beam` binary.

This deliberately uses only the standard library.  Upload fixtures are made by
the CLI itself, so every downloaded manifest and chunk is genuine Filebeam V1
ciphertext rather than a mock crypto implementation.
"""
import hashlib
import base64
import http.server
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import threading
import time
import traceback

BINARY = str(Path(sys.argv[1]).resolve())
CHUNK = int(os.environ.get("BEAM_TRANSFER_TEST_CHUNK_BYTES", "65536"))
FULL_BYTES = int(os.environ.get("BEAM_TRANSFER_TEST_FULL_BYTES", str(25 * 1024 * 1024)))
ID_PREFIX = "01ARZ3NDEKTSV4RRFFQ69G5F"


class State:
    def __init__(self):
        self.lock = threading.Lock()
        self.transfers = {}
        self.next_mode = "normal"
        self.sequence = 0
        self.events = []

    def event(self, method, path):
        with self.lock:
            self.events.append((method, path))

    def new_transfer(self, request):
        with self.lock:
            self.sequence += 1
            ident = ID_PREFIX + f"{self.sequence:02}"[-2:]
            mode, self.next_mode = self.next_mode, "normal"
            items = [{"id": f"{ID_PREFIX}{n:02}", "position": n,
                      "chunk_count": item["chunk_count"]}
                     for n, item in enumerate(request["items"])]
            transfer = {"id": ident, "mode": mode, "items": items, "chunks": {},
                        "manifest": None, "complete_calls": 0, "put_calls": {},
                        "get_calls": {}, "ranges": [], "stages": {}, "gate": threading.Event(), "stalls": 0}
            self.transfers[ident] = transfer
            return transfer


STATE = State()


class API(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *_): pass
    def json(self, status, data, headers=None):
        body = json.dumps({"data": data}, separators=(",", ":")).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        for key, value in (headers or {}).items(): self.send_header(key, value)
        self.end_headers(); self.wfile.write(body)
    def read_body(self):
        remaining = int(self.headers.get("Content-Length", "0")); out = bytearray()
        while remaining:
            block = self.rfile.read(min(65536, remaining))
            if not block: break
            out.extend(block); remaining -= len(block)
        return bytes(out)
    def transfer(self):
        match = re.match(r"/api/v1/transfers/([^/]+)", self.path)
        return STATE.transfers.get(match.group(1)) if match else None
    def do_GET(self):
        STATE.event("GET", self.path)
        path = self.path.split("?", 1)[0]
        if path == "/api/v1/info":
            return self.json(200, {"name":"Filebeam", "chunk_bytes":CHUNK,
                "file_retention_hours":24, "anonymous_uploads_enabled":True,
                "enabled_drivers":["http"], "maximum_transfer_bytes":2**31,
                "maximum_file_count":25, "upload_concurrency":2, "download_concurrency":2,
                "transfer_capabilities":{"upload_status":True,"download_ranges":True}})
        transfer = self.transfer()
        if not transfer: return self.json(404, {})
        if path.endswith("/upload-status"):
            chunks = [{"item_id": item, "position": int(position),
                       "ciphertext_bytes":len(value), "checksum":hashlib.sha256(value).hexdigest()}
                      for (item, position), value in transfer["chunks"].items()]
            return self.json(200, {"status":"pending", "protocol_version":1,
                "chunk_bytes":CHUNK, "chunks":chunks, "next_cursor":None})
        if re.fullmatch(r"/api/v1/transfers/[^/]+", self.path):
            return self.json(200, {"id":transfer["id"], "protocol_version":1,
                "chunk_bytes":CHUNK, "encrypted_manifest":transfer["manifest"],
                "items":transfer["items"], "download_concurrency":2,
                "transfer_capabilities":{"download_ranges":True}})
        match = re.match(r".*/items/([^/]+)/chunks/(\d+)$", self.path)
        if not match: return self.json(404, {})
        key = (match.group(1), match.group(2)); body = transfer["chunks"].get(key)
        if body is None: return self.json(404, {})
        calls = transfer["get_calls"].get(key, 0) + 1; transfer["get_calls"][key] = calls
        start = 0; range_header = self.headers.get("Range")
        if range_header:
            transfer["ranges"].append(range_header)
            start = int(range_header.split("=")[1].split("-")[0])
            if transfer["mode"] == "bad-etag": etag = '"changed"'
            else: etag = '"cipher-v1"'
            payload = body[start:]
            self.send_response(206); self.send_header("Content-Range", f"bytes {start + (1 if transfer['mode'] == 'bad-range' else 0)}-{len(body)-1}/{len(body)}")
        else:
            etag = '"cipher-v1"'; payload = body; self.send_response(200)
        self.send_header("ETag", etag); self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        if transfer["mode"] == "download-stall" and calls == 1:
            transfer["gate"].set(); transfer["gate"].wait(30); return
        if transfer["mode"] in ("get-reset", "bad-etag", "bad-range") and calls == 1:
            self.wfile.write(payload[:max(1, len(payload)//2)]); self.wfile.flush()
            self.connection.shutdown(2); self.connection.close(); return
        try: self.wfile.write(payload)
        except (BrokenPipeError, ConnectionResetError): pass
    def do_POST(self):
        STATE.event("POST", self.path); body = self.read_body()
        if self.path == "/api/v1/transfers":
            transfer = STATE.new_transfer(json.loads(body)); result = {"id":transfer["id"],
                "share_url":"/" + transfer["id"], "chunk_bytes":CHUNK,
                "items":transfer["items"], "upload_token":"fixture-token"}
            if transfer["mode"] == "stage": result["upload_transport"] = {"version":1,
                "part_min_bytes":16384,"part_max_bytes":65536,"request_target_ms":100,
                "request_budget_ms":100,"part_max_count":1000}
            if transfer["mode"].startswith("stage-"): result["upload_transport"] = {"version":1,
                "part_min_bytes":16384,"part_max_bytes":65536,"request_target_ms":100,
                "request_budget_ms":100,"part_max_count":1000}
            return self.json(201, result)
        transfer = self.transfer()
        if not transfer: return self.json(404, {})
        stage = re.match(r".*/uploads/([^/]+)/complete$", self.path)
        if stage:
            session = transfer["stages"][stage.group(1)]; session["state"] = "complete"
            transfer["chunks"][(session["item"], session["position"])] = bytes(session["data"])
            return self.json(200, self.stage_status(stage.group(1), session))
        if self.path.endswith("/complete"):
            transfer["complete_calls"] += 1; transfer["manifest"] = json.loads(body)["encrypted_manifest"]
            if transfer["mode"] == "complete-lost-ack" and transfer["complete_calls"] == 1:
                self.connection.shutdown(2); self.connection.close(); return
            return self.json(200, {})
        return self.json(404, {})
    def stage_status(self, ident, session):
        return {"id":ident,"state":session["state"],"offset":len(session["data"]),
                "ciphertext_bytes":session["bytes"],"checksum":session["checksum"]}
    def do_PUT(self):
        STATE.event("PUT", self.path); transfer = self.transfer()
        if not transfer: return self.json(404, {})
        stage = re.match(r".*/chunks/(\d+)/uploads/([^/]+)$", self.path)
        part = re.match(r".*/chunks/(\d+)/uploads/([^/]+)/parts/(\d+)$", self.path)
        direct = re.match(r".*/items/([^/]+)/chunks/(\d+)$", self.path)
        if stage:
            value = json.loads(self.read_body()); item = re.search(r"/items/([^/]+)/", self.path).group(1)
            session = transfer["stages"].setdefault(stage.group(2), {"item":item,"position":stage.group(1),
                "bytes":value["ciphertext_bytes"],"checksum":value["checksum"],"data":bytearray(),"state":"receiving"})
            return self.json(200, self.stage_status(stage.group(2), session))
        if part:
            session = transfer["stages"].get(part.group(2))
            if not session: return self.json(410, {})
            offset = int(part.group(3)); value = self.read_body()
            if transfer["mode"] == "stage-lost" and not session["data"]:
                transfer["mode"] = "stage"; transfer["stages"].pop(part.group(2), None); return self.json(410, {})
            if transfer["mode"] == "stage-busy" and not session["data"]:
                transfer["mode"] = "stage"; return self.json(423, {}, {"Retry-After":"0"})
            if transfer["mode"] == "stage-lost-ack" and not session["data"]:
                session["data"].extend(value); transfer["mode"] = "stage"
                self.connection.shutdown(2); self.connection.close(); return
            if offset != len(session["data"]) or hashlib.sha256(value).hexdigest() != self.headers.get("X-Filebeam-Part-Checksum"):
                return self.json(409, {})
            session["data"].extend(value); return self.json(200, self.stage_status(part.group(2), session))
        if not direct: return self.json(404, {})
        key = (direct.group(1), direct.group(2)); calls = transfer["put_calls"].get(key, 0) + 1; transfer["put_calls"][key] = calls
        value = self.read_body()
        if transfer["mode"] == "stage" and calls == 1: return self.json(413, {})
        if transfer["mode"] == "upload-retry" and calls == 1: return self.json(429, {}, {"Retry-After":"0"})
        if transfer["mode"] == "upload-retry" and calls == 2: return self.json(503, {})
        transfer["chunks"][key] = value
        if transfer["mode"] == "upload-stall" and transfer["stalls"] == 0:
            transfer["stalls"] += 1; transfer["gate"].set(); transfer["gate"].wait(30)
        if transfer["mode"] == "upload-retry" and calls == 3:
            self.connection.shutdown(2); self.connection.close(); return
        return self.json(201, {})


def run(args, env, cwd, timeout=90):
    return subprocess.run([BINARY, "--plain", *map(str, args)], cwd=cwd, env=env,
                          stdin=subprocess.DEVNULL, capture_output=True, timeout=timeout)
def require(result, text=""):
    assert result.returncode == 0, (text + "\n" + result.stderr.decode(errors="replace"))
    return result.stdout.decode().strip()
def upload(root, env, files, mode="normal"):
    STATE.next_mode = mode
    return require(run(["up", *files], env, root), "upload failed")
def download(root, env, link, name, ok=True):
    result = run(["down", link, "--output", root/name], env, root)
    if ok: require(result, "download failed")
    else: assert result.returncode != 0, "malformed range/ETag unexpectedly succeeded"
    return result
def transfer_for(link): return STATE.transfers[link.split("/")[-1].split("#")[0]]
def saved_id(env, root, before):
    result = run(["transfers"], env, root)
    require(result, "checkpoint list failed")
    ids = {line.split("\t", 1)[0] for line in result.stdout.decode().splitlines() if line}
    created = ids - before
    assert len(created) == 1, f"expected one new checkpoint, found {created!r}"
    return created.pop()
def wait_gate(transfer):
    assert transfer["gate"].wait(10), "fixture did not observe blocked request"
def kill(process):
    process.send_signal(signal.SIGKILL); process.wait(10)


def main():
    assert Path(BINARY).is_file(), f"missing test binary: {BINARY}"
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), API)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    failures = []
    with tempfile.TemporaryDirectory(prefix="beam-transfer-") as directory:
        root = Path(directory); home = root/"private-home"
        home.mkdir(mode=0o700)
        env = {**os.environ, "FILEBEAM_INSTANCE":f"http://127.0.0.1:{server.server_port}",
               "FILEBEAM_HOME":str(home), "FILEBEAM_DISABLE_UPDATE_CHECK":"1", "NO_COLOR":"1"}
        def case(name, fn):
            try: fn(); print("PASS:", name)
            except Exception as error:
                failures.append((name, traceback.format_exc())); print("FAIL:", name, error, file=sys.stderr)
        def basic():
            zero=root/"zero"; tail=root/"tail.bin"; many=[]
            zero.write_bytes(b""); tail.write_bytes(bytes(range(251))*((CHUNK*2+17)//251+1)); tail.write_bytes(tail.read_bytes()[:CHUNK*2+17])
            for n in range(6):
                path=root/f"many-{n}.txt"; path.write_bytes(f"file {n}".encode()); many.append(path)
            link=upload(root,env,[zero,tail,*many]); download(root,env,link,"basic")
            assert (root/"basic"/"zero").read_bytes()==b""
            assert (root/"basic"/"tail.bin").read_bytes()==tail.read_bytes()
            assert len(list((root/"basic").iterdir())) == 8
        def retries():
            source=root/"retry.bin"; source.write_bytes(os.urandom(CHUNK*3+3)); link=upload(root,env,[source],"upload-retry")
            transfer=transfer_for(link); assert max(transfer["put_calls"].values()) >= 4
            download(root,env,link,"retry"); assert (root/"retry"/source.name).read_bytes()==source.read_bytes()
        def staged():
            source=root/"stage.bin"; source.write_bytes(os.urandom(CHUNK+9)); link=upload(root,env,[source],"stage")
            transfer=transfer_for(link); assert transfer["stages"] and transfer["chunks"]
            download(root,env,link,"stage-out")
        def staged_recovery():
            for mode in ("stage-lost", "stage-busy", "stage-lost-ack"):
                source=root/(mode+".bin"); source.write_bytes(os.urandom(CHUNK+9)); link=upload(root,env,[source],mode)
                download(root,env,link,mode+"-out")
        def zip_and_collision():
            folder=root/"zip folder"; (folder/"nested").mkdir(parents=True)
            (folder/"nested"/"same.txt").write_text("nested"); (folder/"same.txt").write_text("root")
            link=require(run(["up",folder,"--zip"],env,root),"ZIP upload failed")
            download(root,env,link,"zip-out"); assert (root/"zip-out"/"zip folder.zip").is_file()
            source=root/"CON"; source.write_bytes(b"collision"); link=upload(root,env,[source])
            output=root/"collision-out"; output.mkdir(); (output/"CON").write_bytes(b"old")
            download(root,env,link,"collision-out"); assert (output/"CON").read_bytes()==b"old"
            assert any(path.read_bytes()==b"collision" for path in output.iterdir() if path.name != "CON")
        def range_reset():
            source=root/"range.bin"; source.write_bytes(os.urandom(CHUNK+99)); link=upload(root,env,[source])
            transfer_for(link)["mode"]="get-reset"; download(root,env,link,"range-out")
            assert transfer_for(link)["ranges"], "truncated ciphertext was not resumed with Range"
            assert (root/"range-out"/source.name).read_bytes()==source.read_bytes()
        def bad_etag():
            source=root/"etag.bin"; source.write_bytes(os.urandom(CHUNK+9)); link=upload(root,env,[source])
            transfer_for(link)["mode"]="bad-etag"; download(root,env,link,"etag-out",False)
            assert not list((root/"etag-out").glob("*")), "bad ETag published output"
        def bad_range():
            source=root/"bad-range.bin"; source.write_bytes(os.urandom(CHUNK+9)); link=upload(root,env,[source])
            transfer_for(link)["mode"]="bad-range"; download(root,env,link,"bad-range-out",False)
        def completion_lost_ack():
            source=root/"complete.bin"; source.write_bytes(os.urandom(CHUNK+1)); link=upload(root,env,[source],"complete-lost-ack")
            assert transfer_for(link)["complete_calls"] >= 2, "completion acknowledgement was not reconciled"
        def large():
            source=root/"full-25mb.bin"; source.write_bytes(os.urandom(FULL_BYTES)); link=upload(root,env,[source])
            download(root,env,link,"full-out",); assert (root/"full-out"/source.name).read_bytes()==source.read_bytes()
        def killed_upload_resume():
            source=root/"kill-upload.bin"; source.write_bytes(os.urandom(CHUNK*3+1))
            sequence=STATE.sequence
            before={line.split("\t",1)[0] for line in run(["transfers"],env,root).stdout.decode().splitlines() if line}
            STATE.next_mode="upload-stall"; process=subprocess.Popen([BINARY,"--plain","up",str(source)],cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            transfer=None
            deadline=time.monotonic()+10
            while transfer is None and time.monotonic()<deadline:
                transfer=next((t for t in STATE.transfers.values() if t["id"] == ID_PREFIX + f"{sequence+1:02}"[-2:]),None); time.sleep(.02)
            wait_gate(transfer); kill(process); ident=saved_id(env,root,before)
            receipt=require(run(["resume",ident],env,root),"killed upload resume failed")
            assert receipt.count("#k=v1.")==1 and transfer["chunks"], f"resume did not produce one receipt: {receipt!r}"
        def killed_download_resume_and_lock():
            source=root/"kill-download.bin"; source.write_bytes(os.urandom(CHUNK*2+7)); link=upload(root,env,[source])
            transfer=transfer_for(link); transfer["mode"]="download-stall"
            before={line.split("\t",1)[0] for line in run(["transfers"],env,root).stdout.decode().splitlines() if line}
            process=subprocess.Popen([BINARY,"--plain","down",link,"--output",str(root/"kill-out")],cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            wait_gate(transfer); kill(process); ident=saved_id(env,root,before)
            checkpoint=home/"transfers"/ident
            share=checkpoint/"share-key"; expected=base64.urlsafe_b64decode(link.split("#k=v1.")[1]+"===")
            assert share.read_bytes()==expected, f"resume checkpoint lost original share key ({len(share.read_bytes())} != {len(expected)})"
            transfer["gate"].set(); require(run(["resume",ident],env,root),"killed download resume failed")
            assert (root/"kill-out"/source.name).read_bytes()==source.read_bytes()
        def source_changed_before_resume():
            source=root/"changed-source.bin"; source.write_bytes(os.urandom(CHUNK*2+1)); STATE.next_mode="upload-stall"
            sequence=STATE.sequence
            before={line.split("\t",1)[0] for line in run(["transfers"],env,root).stdout.decode().splitlines() if line}
            process=subprocess.Popen([BINARY,"--plain","up",str(source)],cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            deadline=time.monotonic()+10; transfer=None
            while transfer is None and time.monotonic()<deadline:
                transfer=next((t for t in STATE.transfers.values() if t["id"] == ID_PREFIX + f"{sequence+1:02}"[-2:]),None); time.sleep(.02)
            wait_gate(transfer); kill(process); ident=saved_id(env,root,before); before=sum(transfer["put_calls"].values())
            source.write_bytes(os.urandom(CHUNK*2+1)); result=run(["resume",ident],env,root)
            assert result.returncode != 0 and sum(transfer["put_calls"].values())==before, "changed source issued a new PUT"
        case("V1 encrypted zero/tail/many-file upload and download", basic)
        case("429, 503, accepted-body/lost-ack direct upload reconciliation", retries)
        case("413 direct fallback to staged parts and completion", staged)
        case("staged 410 reset, 423 Retry-After, and lost part ACK reconcile offsets", staged_recovery)
        case("ZIP transfer and pre-existing sanitized output collision", zip_and_collision)
        case("reset GET resumes with authenticated Range/ETag", range_reset)
        case("changed ETag continuation is rejected without publication", bad_etag)
        case("malformed Content-Range is rejected without publication", bad_range)
        case("lost completion acknowledgement is idempotently reconciled", completion_lost_ack)
        case("SIGKILL upload checkpoint resumes with one completed receipt", killed_upload_resume)
        case("SIGKILL download retains original key and resumes", killed_download_resume_and_lock)
        case("source mutation aborts before nonce reuse or a new PUT", source_changed_before_resume)
        if os.environ.get("BEAM_TRANSFER_TEST_FULL", "1") != "0": case("25 MiB multichunk encrypted roundtrip", large)
        # Checkpoint material contains both ciphertext and link keys and must never be public.
        transfers = home/"transfers"
        if transfers.exists():
            assert all((path.stat().st_mode & 0o077) == 0 for path in transfers.rglob("*") if path.is_file()), "private checkpoint permissions"
    server.shutdown()
    if failures:
        report = Path(os.environ.get("BEAM_TRANSFER_TEST_REPORT", str(Path(tempfile.gettempdir()) / "filebeam-transfer-fault-report.txt")))
        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text("\n\n".join(f"{name}\n{detail}" for name, detail in failures))
        print(f"{len(failures)} scenario(s) failed; details: {report}", file=sys.stderr)
        raise SystemExit(1)

if __name__ == "__main__": main()
