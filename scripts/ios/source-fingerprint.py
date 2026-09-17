#!/usr/bin/env python3
"""Fingerprint the actual Rust/build inputs, including untracked worktree files."""

import hashlib
from pathlib import Path

root = Path(__file__).resolve().parents[2]
paths = set()
for crate in ("client-ffi", "client-core", "encryption", "transfer", "transfer-native"):
    directory = root / crate
    paths.update(path for path in (directory / "src").rglob("*") if path.is_file())
    paths.update(directory / name for name in (
        "Cargo.toml", "Cargo.lock", "build.rs", "uniffi.toml", "rust-toolchain.toml",
    ) if (directory / name).is_file())
paths.update((root / "scripts/ios" / name) for name in (
    "build-rust.sh", "generate-bindings.py", "normalize-bindings.py", "toolchain.lock",
    "source-fingerprint.py",
))
digest = hashlib.sha256()
for path in sorted(paths):
    digest.update(str(path.relative_to(root)).encode())
    digest.update(b"\0")
    digest.update(hashlib.sha256(path.read_bytes()).digest())
print(digest.hexdigest())
