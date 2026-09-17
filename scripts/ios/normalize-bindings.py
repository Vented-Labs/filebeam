#!/usr/bin/env python3
"""Normalize a known UniFFI 0.32 C keyword emission without changing ABI layout."""
from pathlib import Path
import sys

bindings = Path(sys.argv[1]) if len(sys.argv) == 2 else None
if bindings is None or not bindings.is_dir():
    raise SystemExit("usage: normalize-bindings.py BINDINGS_DIRECTORY")

for path, old, new in (
    (bindings / "FilebeamCoreFFI.h", "`open`;", "open_;"),
    (bindings / "FilebeamCore.swift", "`open`:", "open_:"),
):
    contents = path.read_text()
    count = contents.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one {old!r} in {path}, found {count}")
    path.write_text(contents.replace(old, new))
