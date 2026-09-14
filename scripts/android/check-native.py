#!/usr/bin/env python3
"""Check the packaged Rust/JNA libraries, including ELF and APK 16-KiB alignment."""
import argparse
from pathlib import Path
import struct
import zipfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("apk", type=Path)
args = parser.parse_args()
libraries = 0
rust = False
with zipfile.ZipFile(args.apk) as apk:
    for entry in apk.infolist():
        if not entry.filename.startswith("lib/") or not entry.filename.endswith(".so"):
            continue
        libraries += 1
        rust |= entry.filename.endswith("/libfilebeam_client_ffi.so")
        data = apk.read(entry)
        assert data[:4] == b"\x7fELF", f"Not ELF: {entry.filename}"
        if data[4] != 2:  # The 16-KiB requirement applies to the 64-bit ABIs.
            continue
        assert data[5] == 1, "Expected a little-endian Android ELF"
        offset = struct.unpack_from("<Q", data, 32)[0]
        size, count = struct.unpack_from("<HH", data, 54)
        segments = [struct.unpack_from("<IIQQQQQQ", data, offset + index * size) for index in range(count)]
        for kind, _, file_offset, address, _, _, memory_size, alignment in segments:
            if kind == 1:
                assert alignment >= 16384, f"4-KiB LOAD segment: {entry.filename}"
                assert file_offset % 16384 == address % 16384, f"Misaligned LOAD: {entry.filename}"
            if kind == 0x6474E552:
                end = address + memory_size
                protected_end = (end + 16383) // 16384 * 16384
                # Older prebuilts can leave a gap before the next writable LOAD
                # instead of padding RELRO itself. Rounding protection to a page
                # is safe only if it cannot cover any writable tail/next mapping.
                for load, flags, _, start, _, _, length, _ in segments:
                    if load == 1 and flags & 2 and start + length > end:
                        assert end == protected_end or start >= protected_end, f"RELRO covers writable data: {entry.filename}"
        assert entry.compress_type == zipfile.ZIP_STORED, f"Native library must be uncompressed: {entry.filename}"
        apk.fp.seek(entry.header_offset)
        header = apk.fp.read(30)
        name_size, extra_size = struct.unpack_from("<HH", header, 26)
        assert (entry.header_offset + 30 + name_size + extra_size) % 16384 == 0, f"APK ZIP alignment: {entry.filename}"
        print(f"16-KiB aligned: {entry.filename}")
assert rust, "The APK is missing the Rust client library"
print(f"Checked {libraries} native libraries in {args.apk.name}")
