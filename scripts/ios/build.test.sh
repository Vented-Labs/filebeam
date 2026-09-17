#!/usr/bin/env bash
set -euo pipefail

fixture=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-ios-build.XXXXXX")
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/scripts/ios" "$fixture/bin"
cp "$(dirname -- "$0")/build.sh" "$(dirname -- "$0")/lib.sh" "$fixture/scripts/ios/"
log="$fixture/commands.log"

cat >"$fixture/scripts/ios/build-rust.sh" <<'EOF'
#!/usr/bin/env bash
printf 'build-rust %s\n' "$1" >> "$IOS_BUILD_TEST_LOG"
EOF
cat >"$fixture/scripts/ios/generate-project.sh" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' 'generate-project' >> "$IOS_BUILD_TEST_LOG"
EOF
cat >"$fixture/bin/xcodebuild" <<'EOF'
#!/usr/bin/env bash
{
    printf 'xcodebuild'
    printf ' %s' "$@"
    printf '\n'
} >> "$IOS_BUILD_TEST_LOG"
EOF
chmod +x "$fixture/scripts/ios/build-rust.sh" "$fixture/scripts/ios/generate-project.sh" "$fixture/bin/xcodebuild"

IOS_BUILD_TEST_LOG="$log" PATH="$fixture/bin:$PATH" bash "$fixture/scripts/ios/build.sh" Debug simulator
IOS_BUILD_TEST_LOG="$log" PATH="$fixture/bin:$PATH" bash "$fixture/scripts/ios/build.sh" Release device

[[ $(<"$log") == *'build-rust debug'* ]] || { printf '%s\n' 'Debug build did not select the debug Rust profile' >&2; exit 1; }
[[ $(<"$log") == *'build-rust release'* ]] || { printf '%s\n' 'Release build did not select the release Rust profile' >&2; exit 1; }
[[ $(<"$log") == *'generic/platform=iOS Simulator'* ]] || { printf '%s\n' 'Debug build did not select the simulator destination' >&2; exit 1; }
[[ $(<"$log") == *'generic/platform=iOS CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO'* ]] || { printf '%s\n' 'Release build did not disable device code signing' >&2; exit 1; }
