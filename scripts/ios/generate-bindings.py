#!/usr/bin/env python3
"""Run UniFFI with the crate working directory needed by cargo metadata."""

import argparse
import subprocess
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    subprocess.run(
        [
            "cargo", "+1.98.0", "run", "--locked", "--features", "bindgen",
            "--bin", "uniffi-bindgen", "--", "generate",
            "--library", str(arguments.library.resolve()), "--language", "swift",
            "--out-dir", str(arguments.out_dir.resolve()), "--no-format",
        ],
        cwd=root / "client-ffi",
        check=True,
    )


if __name__ == "__main__":
    main()
