#!/usr/bin/env python3
import gzip
import os
import stat
import sys
import tarfile
import zipfile
from pathlib import Path


if len(sys.argv) != 5:
    raise SystemExit("Usage: archive.py FORMAT STAGE OUTPUT SOURCE_DATE_EPOCH")

archive_format, stage_arg, output_arg, epoch_arg = sys.argv[1:]
stage = Path(stage_arg)
output = Path(output_arg)
epoch = int(epoch_arg)
files = sorted(path for path in stage.rglob("*") if path.is_file())

if archive_format == "tar.gz":
    with output.open("xb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=epoch) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as archive:
                for path in files:
                    name = path.relative_to(stage).as_posix()
                    info = archive.gettarinfo(str(path), name)
                    info.uid = info.gid = 0
                    info.uname = info.gname = ""
                    info.mtime = epoch
                    info.mode = 0o755 if os.access(path, os.X_OK) else 0o644
                    with path.open("rb") as source:
                        archive.addfile(info, source)
elif archive_format == "zip":
    # ZIP timestamps cannot represent dates before 1980.
    timestamp = max(epoch, 315532800)
    date_time = __import__("datetime").datetime.fromtimestamp(
        timestamp, __import__("datetime").timezone.utc
    ).timetuple()[:6]
    with zipfile.ZipFile(output, mode="x", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in files:
            info = zipfile.ZipInfo(path.relative_to(stage).as_posix(), date_time)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (stat.S_IFREG | 0o755) << 16
            archive.writestr(info, path.read_bytes())
else:
    raise SystemExit("FORMAT must be tar.gz or zip")
