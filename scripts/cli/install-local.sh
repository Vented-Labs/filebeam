#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
install_dir=${FILEBEAM_HOME:-$HOME/.filebeam}
instance=${FILEBEAM_INSTANCE:-http://localhost:${APP_PORT:-8000}}

usage() {
    printf 'Usage: %s [--dir DIRECTORY] [--instance URL]\n' "$0" >&2
    exit 64
}

while (($#)); do
    case $1 in
        --dir) [[ $# -ge 2 ]] || usage; install_dir=$2; shift 2 ;;
        --instance) [[ $# -ge 2 ]] || usage; instance=$2; shift 2 ;;
        -h|--help) usage ;;
        *) usage ;;
    esac
done

[[ $instance =~ ^https?://[][[:alnum:].:_-]+/?$ ]] || {
    printf '%s\n' 'The local instance must be an HTTP origin without a path.' >&2
    exit 1
}

"$root/scripts/cli/build.sh"
binary="$root/cli/target/release/beam"
[[ -x $binary ]] || { printf 'Expected %s after the Docker build.\n' "$binary" >&2; exit 1; }

mkdir -p "$install_dir/bin" "$install_dir/cache"
[[ -f $install_dir/config.toml ]] || : > "$install_dir/config.toml"
install -m 0755 "$binary" "$install_dir/bin/beam-local"
cat > "$install_dir/bin/beam" <<EOF
#!/bin/sh
FILEBEAM_INSTANCE='$instance'
export FILEBEAM_INSTANCE
exec "\$(dirname "\$0")/beam-local" "\$@"
EOF
chmod 0755 "$install_dir/bin/beam"

path_line="export PATH=\"$install_dir/bin:\$PATH\""
touch "$HOME/.bashrc"
grep -Fqx "$path_line" "$HOME/.bashrc" || printf '\n%s\n' "$path_line" >> "$HOME/.bashrc"

printf 'Installed local beam to %s/bin/beam\n' "$install_dir"
printf 'Instance: %s\n' "$instance"
printf 'Run: export PATH="%s/bin:$PATH" && beam\n' "$install_dir"
