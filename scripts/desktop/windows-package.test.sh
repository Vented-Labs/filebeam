#!/usr/bin/env bash
set -euo pipefail
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/input with spaces" "$tmp/output with spaces"
printf 'executable fixture\n' > "$tmp/input with spaces/filebeam.exe"
cat > "$tmp/bin/cygpath" <<'PY'
#!/usr/bin/env python3
import sys
assert sys.argv[1] == "-w"
print("Z:" + sys.argv[2].replace("/", "\\"))
PY
cat > "$tmp/bin/makensis" <<'PY'
#!/usr/bin/env python3
import os
from pathlib import Path
import sys
assert os.environ["MSYS_NO_PATHCONV"] == "1"
def native_path(value):
    assert value.startswith("Z:\\") and "/" not in value, value
    return Path(value[2:].replace("\\", "/"))
defines = dict(argument[2:].split("=", 1) for argument in sys.argv[1:-1])
assert native_path(defines["INPUT"]).read_bytes() == b"executable fixture\n"
assert native_path(defines["ICON"]).is_file()
assert native_path(sys.argv[-1]).is_file()
native_path(defines["OUTPUT"]).write_bytes(b"installer fixture")
PY
chmod +x "$tmp/bin/cygpath" "$tmp/bin/makensis"
PATH="$tmp/bin:$PATH" bash "$root/scripts/desktop/package-windows-installer.sh" \
    "$tmp/input with spaces/filebeam.exe" v1.2.3 "$tmp/output with spaces"
test -s "$tmp/output with spaces/filebeam-desktop-v1.2.3-windows-x86_64-setup.exe"
python3 - "$tmp/output with spaces/filebeam-desktop-v1.2.3-windows-x86_64.zip" <<'PY'
import sys
import zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    assert archive.read("filebeam/filebeam.exe") == b"executable fixture\n"
PY
