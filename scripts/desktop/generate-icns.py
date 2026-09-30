#!/usr/bin/env python3
"""Build a PNG-backed macOS ICNS file without requiring macOS iconutil."""

import struct
import sys
from pathlib import Path


CHUNK_TYPES = {
    16: b"icp4",
    32: b"icp5",
    64: b"icp6",
    128: b"ic07",
    256: b"ic08",
    512: b"ic09",
    1024: b"ic10",
}


def png_dimensions(data: bytes) -> tuple[int, int]:
    if data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        raise ValueError("expected a PNG image")
    return struct.unpack(">II", data[16:24])


def main() -> None:
    output = Path(sys.argv[1])
    chunks = []
    for name in sys.argv[2:]:
        data = Path(name).read_bytes()
        width, height = png_dimensions(data)
        if width != height or width not in CHUNK_TYPES:
            raise ValueError(f"unsupported ICNS source dimensions: {name}")
        chunks.append(CHUNK_TYPES[width] + struct.pack(">I", len(data) + 8) + data)
    payload = b"".join(chunks)
    output.write_bytes(b"icns" + struct.pack(">I", len(payload) + 8) + payload)


if __name__ == "__main__":
    if len(sys.argv) != 9:
        raise SystemExit("usage: generate-icns.py OUTPUT 16.png ... 1024.png")
    main()
