#!/usr/bin/env bash
# Canonical Linux binding check for the production UniFFI source and ABI shape.
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
rust_version=1.98.0
target_dir=${CARGO_TARGET_DIR:-"$root/client-ffi/target"}
output=$(mktemp -d)
trap 'rm -rf "$output"' EXIT

CARGO_TARGET_DIR="$target_dir" cargo "+$rust_version" build --manifest-path "$root/client-ffi/Cargo.toml" --locked --lib
python3 "$root/scripts/ios/generate-bindings.py" --library "$target_dir/debug/libfilebeam_client_ffi.so" --out-dir "$output/bindings"
python3 "$root/scripts/ios/normalize-bindings.py" "$output/bindings"
mkdir "$output/include"
cp "$output/bindings/FilebeamCoreFFI.h" "$output/include/"
cp "$output/bindings/FilebeamCoreFFI.modulemap" "$output/include/module.modulemap"
docker run --rm --mount "type=bind,src=$output,dst=/work,readonly" --workdir /work swift:6.2-noble \
    swiftc -swift-version 6 -strict-concurrency=complete -typecheck bindings/FilebeamCore.swift -I include
if [[ -f "$root/mobile/ios/Platform/Rust/NativeFilebeamService.swift" ]]; then
    mkdir "$output/modules"
    docker run --rm --mount "type=bind,src=$output,dst=/work" --workdir /work swift:6.2-noble \
        swiftc -swift-version 6 -strict-concurrency=complete -emit-module -module-name FilebeamCore bindings/FilebeamCore.swift -I include -emit-module-path modules/FilebeamCore.swiftmodule
    docker run --rm --mount "type=bind,src=$root/mobile/ios/Packages/FilebeamDomain/Sources/FilebeamDomain,dst=/domain,readonly" --mount "type=bind,src=$output,dst=/work" --workdir /work swift:6.2-noble \
        sh -c 'swiftc -swift-version 6 -strict-concurrency=complete -emit-module -module-name FilebeamDomain /domain/*.swift -emit-module-path modules/FilebeamDomain.swiftmodule'
    cp "$root/mobile/ios/Platform/Rust/"*.swift "$output/"
    docker run --rm --mount "type=bind,src=$output,dst=/work,readonly" --workdir /work swift:6.2-noble \
        sh -c 'swiftc -swift-version 6 -strict-concurrency=complete -typecheck *.swift -I include -I modules'
fi
printf '%s\n' 'Linux UniFFI Swift binding typecheck passed.'
