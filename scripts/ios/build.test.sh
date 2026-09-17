#!/usr/bin/env bash
set -euo pipefail

fixture=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-ios-build.XXXXXX")
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/scripts/ios" "$fixture/bin" "$fixture/Xcode Developer"
cp "$(dirname -- "$0")/"{build.sh,test.sh,lib.sh} "$fixture/scripts/ios/"
log="$fixture/commands.log"

cat >"$fixture/scripts/ios/build-rust.sh" <<'EOF'
#!/usr/bin/env bash
[[ ${DEVELOPER_DIR:-} == "$IOS_DEVELOPER_DIR" ]] || exit 1
printf 'build-rust %s\n' "$1" >> "$IOS_BUILD_TEST_LOG"
EOF
cat >"$fixture/scripts/ios/generate-project.sh" <<'EOF'
#!/usr/bin/env bash
[[ ${DEVELOPER_DIR:-} == "$IOS_DEVELOPER_DIR" ]] || exit 1
printf '%s\n' 'generate-project' >> "$IOS_BUILD_TEST_LOG"
EOF
cat >"$fixture/bin/xcodebuild" <<'EOF'
#!/usr/bin/env bash
[[ ${DEVELOPER_DIR:-} == "$IOS_DEVELOPER_DIR" ]] || exit 1
if [[ $1 == -version ]]; then printf 'Xcode 26.3\nBuild version 17C529\n'; exit; fi
{
    printf 'xcodebuild'
    printf ' %s' "$@"
    printf '\n'
} >> "$IOS_BUILD_TEST_LOG"
EOF
cat >"$fixture/bin/uname" <<'EOF'
#!/usr/bin/env bash
[[ $1 == -s ]] || exit 1
printf 'Darwin\n'
EOF
cat >"$fixture/bin/xcrun" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
chmod +x "$fixture/scripts/ios/build-rust.sh" "$fixture/scripts/ios/generate-project.sh" "$fixture/bin/"*
export IOS_DEVELOPER_DIR="$fixture/Xcode Developer" IOS_XCODE_VERSION=26.3
unset DEVELOPER_DIR

IOS_BUILD_TEST_LOG="$log" PATH="$fixture/bin:$PATH" bash "$fixture/scripts/ios/build.sh" Debug simulator
IOS_BUILD_TEST_LOG="$log" PATH="$fixture/bin:$PATH" bash "$fixture/scripts/ios/build.sh" Release device
IOS_BUILD_TEST_LOG="$log" PATH="$fixture/bin:$PATH" IOS_TEST_DESTINATION='platform=iOS Simulator,id=fixture' \
    bash "$fixture/scripts/ios/test.sh"

[[ $(<"$log") == *'build-rust debug'* ]] || { printf '%s\n' 'Debug build did not select the debug Rust profile' >&2; exit 1; }
[[ $(<"$log") == *'build-rust release'* ]] || { printf '%s\n' 'Release build did not select the release Rust profile' >&2; exit 1; }
[[ $(<"$log") == *'generic/platform=iOS Simulator'* ]] || { printf '%s\n' 'Debug build did not select the simulator destination' >&2; exit 1; }
[[ $(<"$log") == *'generic/platform=iOS CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO'* ]] || { printf '%s\n' 'Release build did not disable device code signing' >&2; exit 1; }
[[ $(<"$log") == *'xcodebuild test '* ]] || { printf '%s\n' 'Test runner did not select the pinned Xcode' >&2; exit 1; }
