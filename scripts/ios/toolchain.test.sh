#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"

fixture=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-ios-toolchain.XXXXXX")
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/bin" "$fixture/Xcode Developer"
cat >"$fixture/bin/uname" <<'EOF'
#!/usr/bin/env bash
case $1 in -s) printf 'Darwin\n' ;; -m) printf 'arm64\n' ;; *) exit 1 ;; esac
EOF
cat >"$fixture/bin/xcodebuild" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ $1 == -version ]]
printf 'probe\n' >> "$IOS_TOOLCHAIN_TEST_LOG"
[[ ${IOS_TOOLCHAIN_TEST_MODE:-valid} != failure ]] || exit 23
printf 'Xcode %s\n' "${IOS_TOOLCHAIN_TEST_VERSION:-26.3}"
# Separate writes reproduce Xcode's broken pipe when a parser exits early.
sleep 0.1
printf 'Build version 17C529\n'
EOF
cat >"$fixture/bin/xcrun" <<'EOF'
#!/usr/bin/env bash
[[ $* == '--sdk iphoneos --show-sdk-version' ]] || exit 1
printf '26.2\n'
EOF
chmod +x "$fixture/bin/"*
export PATH="$fixture/bin:$PATH" IOS_TOOLCHAIN_TEST_LOG="$fixture/probes"
IOS_DEVELOPER_DIR="$fixture/Xcode Developer"
IOS_XCODE_VERSION=26.3

expected=$(printf 'xcode=26.3\nbuild=17C529\nsdk=26.2\narch=arm64')
[[ $(toolchain_identity) == "$expected" ]] || die 'toolchain identity lost Xcode output'
[[ $(<"$IOS_TOOLCHAIN_TEST_LOG") == probe ]] || die 'toolchain identity should use a single version probe'

for mode in mismatch failure; do
    if IOS_TOOLCHAIN_TEST_MODE="$mode" IOS_TOOLCHAIN_TEST_VERSION=26.2 \
        IOS_DEVELOPER_DIR="$IOS_DEVELOPER_DIR" \
        bash -e -u -o pipefail -c 'source "$1"; require_macos' _ \
        "$root/scripts/ios/lib.sh" >"$fixture/$mode.log" 2>&1; then
        die "toolchain probe accepted $mode"
    fi
done

# Exercise the complete installer, including a cache left by the binary-only layout.
repository="$fixture/repository with spaces"
bundle="$fixture/release/xcodegen"
mkdir -p "$repository/scripts/ios" "$bundle/bin" "$bundle/share/xcodegen/SettingPresets/Platforms"
cp "$root/scripts/ios/"{bootstrap.sh,lib.sh} "$repository/scripts/ios/"
cat >"$bundle/bin/xcodegen" <<'EOF'
#!/usr/bin/env bash
[[ $1 == --version ]] || exit 1
printf 'Version: 2.41.0\n'
EOF
printf 'base settings\n' > "$bundle/share/xcodegen/SettingPresets/base.yml"
printf 'iOS settings\n' > "$bundle/share/xcodegen/SettingPresets/Platforms/iOS.yml"
printf 'license\n' > "$bundle/LICENSE"
printf 'fixture archive\n' > "$fixture/archive.zip"
digest=$(shasum -a 256 "$fixture/archive.zip" | awk '{print $1}')
printf 'XCODEGEN_SHA256=%s\n' "$digest" > "$repository/scripts/ios/toolchain.lock"
cat >"$fixture/bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
output=
while (($#)); do
    case $1 in
        --output) output=$2; shift 2 ;;
        *) shift ;;
    esac
done
cp "$IOS_TOOLCHAIN_TEST_ARCHIVE" "${output:?}"
printf 'download\n' >> "$IOS_TOOLCHAIN_TEST_DOWNLOADS"
EOF
cat >"$fixture/bin/ditto" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ $1 == -x && $2 == -k && -f $3 ]]
cp -R "$IOS_TOOLCHAIN_TEST_BUNDLE" "$4/xcodegen"
EOF
cat >"$fixture/bin/rustup" <<'EOF'
#!/usr/bin/env bash
if [[ $* == 'run 1.98.0 rustc --version' ]]; then printf 'rustc 1.98.0\n'; fi
EOF
chmod +x "$fixture/bin/"*
export IOS_DEVELOPER_DIR IOS_TOOLCHAIN_TEST_ARCHIVE="$fixture/archive.zip"
export IOS_TOOLCHAIN_TEST_BUNDLE="$bundle" IOS_TOOLCHAIN_TEST_DOWNLOADS="$fixture/downloads"

bash "$repository/scripts/ios/bootstrap.sh" > "$fixture/install.log"
installed="$repository/.ios-tools/xcodegen-2.41.0"
[[ -x "$installed/bin/xcodegen" ]] || die 'XcodeGen executable was not installed'
for file in LICENSE share/xcodegen/SettingPresets/base.yml share/xcodegen/SettingPresets/Platforms/iOS.yml; do
    cmp "$bundle/$file" "$installed/$file" || die "XcodeGen installation lost $file"
done
bash "$repository/scripts/ios/bootstrap.sh" > "$fixture/cached-install.log"
[[ $(<"$IOS_TOOLCHAIN_TEST_DOWNLOADS") == download ]] || die 'complete XcodeGen installation was not reused'
rm -rf "$installed/share"
bash "$repository/scripts/ios/bootstrap.sh" > "$fixture/repaired-install.log"
[[ $(<"$IOS_TOOLCHAIN_TEST_DOWNLOADS") == $(printf 'download\ndownload') ]] || die 'binary-only XcodeGen cache was not repaired'
cmp "$bundle/share/xcodegen/SettingPresets/Platforms/iOS.yml" "$installed/share/xcodegen/SettingPresets/Platforms/iOS.yml"
