#!/usr/bin/env sh
set -eu

home=${FILEBEAM_HOME:-"${HOME:?HOME is required}/.filebeam"}
root="$home/runtime/daemon"
owner=${3:-cli}

valid_owner() { case "$1" in ''|*[!0-9A-Za-z_-]*) return 1;; esac; }
valid_version() { printf '%s\n' "$1" | awk 'BEGIN { ok=0 } /^[0-9]+\.[0-9]+\.[0-9]+$/ { ok=1 } END { exit !ok }'; }
component_record() { printf '%s/component.json' "$root"; }

case ${1:-} in
    --remove)
        owner=${2:?Component owner is required}
        valid_owner "$owner" || { printf '%s\n' 'Invalid daemon component owner.' >&2; exit 64; }
        mkdir -p "$root/versions"
        [ ! -L "$root" ] && [ ! -L "$root/versions" ] || { printf '%s\n' 'Daemon component directories must not be symlinks.' >&2; exit 1; }
        lock="$root/install.lock"
        mkdir "$lock" 2>/dev/null || { printf '%s\n' 'Another Filebeam component install is in progress.' >&2; exit 1; }
        trap 'rmdir "$lock"' EXIT HUP INT TERM
        record=$(component_record)
        [ -f "$record" ] || { printf '%s\n' last; exit 0; }
        current=$(sed -n 's/.*"current":"\([0-9.]*\)".*/\1/p' "$record" | awk 'NR == 1 { print; exit }')
        tmp="$root/.component.$$.json"
        printf '{"schema":1,"owners":{' >"$tmp"
        first=true
        retained=false
        # Owner names are deliberately restricted, so this line-oriented parser only
        # accepts the canonical record format and never evaluates record contents.
        for existing in $(tr '{}' '\n' <"$record" | sed -n 's/^"\([0-9A-Za-z_-]*\)":true$/\1/p'); do
            [ "$existing" = "$owner" ] && continue
            if [ "$first" = true ]; then first=false; else printf ',' >>"$tmp"; fi
            printf '"%s":true' "$existing" >>"$tmp"
            retained=true
        done
        printf '},"current":%s}\n' "${current:+\"$current\"}" >>"$tmp"
        mv -f "$tmp" "$record"
        if [ "$retained" = true ]; then printf '%s\n' retained; else printf '%s\n' last; fi
        exit 0
        ;;
esac

binary=${1:?Path to a release-verified filebeamd binary is required}
version=${2:?Release version is required}
valid_owner "$owner" || { printf '%s\n' 'Invalid daemon component owner.' >&2; exit 64; }
valid_version "$version" || { printf '%s\n' 'Invalid daemon component version.' >&2; exit 64; }
[ -x "$binary" ] || { printf '%s\n' 'filebeamd must be a release-verified executable.' >&2; exit 1; }
mkdir -p "$root/versions"
[ ! -L "$root" ] && [ ! -L "$root/versions" ] || { printf '%s\n' 'Daemon component directories must not be symlinks.' >&2; exit 1; }
umask 077
lock="$root/install.lock"
mkdir "$lock" 2>/dev/null || { printf '%s\n' 'Another Filebeam component install is in progress.' >&2; exit 1; }
trap 'rmdir "$lock"' EXIT HUP INT TERM

record=$(component_record)
current=$( [ -f "$record" ] && sed -n 's/.*"current":"\([0-9.]*\)".*/\1/p' "$record" | awk 'NR == 1 { print; exit }' || : )
if [ -n "$current" ] && ! valid_version "$current"; then
    printf '%s\n' 'Daemon component record has an invalid version.' >&2
    exit 1
fi
effective_version=$version
downgrade=false
if [ -n "$current" ] && awk -v installed="$current" -v requested="$version" 'BEGIN { split(installed, a, "."); split(requested, b, "."); for (i=1; i<=3; i++) { if ((b[i]+0) < (a[i]+0)) exit 0; if ((b[i]+0) > (a[i]+0)) exit 1 } exit 1 }'; then
    effective_version=$current
    downgrade=true
fi
target="$root/versions/$effective_version"
if [ ! -x "$target/filebeamd" ]; then
    [ "$downgrade" = false ] || { printf '%s\n' 'Refusing to replace a missing newer daemon with an older component.' >&2; exit 1; }
    candidate="$root/versions/.${effective_version}.$$"
    mkdir "$candidate"
    install -m 0755 "$binary" "$candidate/filebeamd"
    mv "$candidate" "$target"
fi

launcher="$root/filebeamd"
temporary="$root/.filebeamd.$$"
printf '%s\n' '#!/usr/bin/env sh' 'set -eu' "exec \"\$(CDPATH='' cd -- \"\$(dirname -- \"\$0\")\" && pwd)/versions/$effective_version/filebeamd\" \"\$@\"" >"$temporary"
chmod 700 "$temporary"
mv -f "$temporary" "$launcher"
tmp="$root/.component.$$.json"
printf '{"schema":1,"owners":{' >"$tmp"
first=true
if [ -f "$record" ]; then
    for existing in $(tr '{}' '\n' <"$record" | sed -n 's/^"\([0-9A-Za-z_-]*\)":true$/\1/p'); do
        [ "$existing" = "$owner" ] && continue
        if [ "$first" = true ]; then first=false; else printf ',' >>"$tmp"; fi
        printf '"%s":true' "$existing" >>"$tmp"
    done
fi
if [ "$first" = true ]; then first=false; else printf ',' >>"$tmp"; fi
printf '"%s":true},"current":"%s"}\n' "$owner" "$effective_version" >>"$tmp"
mv -f "$tmp" "$record"
printf '%s\n' "$launcher"
