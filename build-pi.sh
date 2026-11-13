#!/bin/sh
# Builds static ntu-bus binaries for 64-bit and 32-bit Raspberry Pi OS into dist/.
set -eu
cd "$(dirname "$0")"
mkdir -p dist
for pair in aarch64-unknown-linux-musl:aarch64 armv7-unknown-linux-musleabihf:armv7; do
    target=${pair%%:*}
    name=${pair##*:}
    cargo zigbuild --release --target "$target"
    cp "target/$target/release/ntu-bus" "dist/ntu-bus-$name"
done
ls -l dist
