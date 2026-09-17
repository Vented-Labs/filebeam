#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)

docker run --rm -v "$root:/workspace:ro" -w /workspace swift:6.0 bash -ceu '
build=$(mktemp -d)
trap "rm -rf \"$build\"" EXIT
domain=mobile/ios/Packages/FilebeamDomain/Sources/FilebeamDomain
swiftc -emit-library -emit-module -enable-testing -module-name FilebeamDomain -emit-module-path "$build/FilebeamDomain.swiftmodule" -o "$build/libFilebeamDomain.so" "$domain"/*.swift
swiftc -emit-library -emit-module -enable-testing -module-name NativePolicy -I "$build" -L "$build" -lFilebeamDomain -emit-module-path "$build/NativePolicy.swiftmodule" -o "$build/libNativePolicy.so" mobile/ios/Platform/Rust/NativePolicy.swift
swiftc -parse-as-library -I "$build" -L "$build" -lNativePolicy -lFilebeamDomain -Xlinker -rpath -Xlinker "$build" -o "$build/native-policy-tests" mobile/ios/Tests/NativePolicyTests.swift -Xlinker -lXCTest
"$build/native-policy-tests"
'
