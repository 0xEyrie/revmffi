#!/bin/bash
set -o errexit -o nounset -o pipefail
mkdir -p artifacts
export CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

# No stripping implemented (see https://github.com/CosmWasm/wasmvm/issues/222#issuecomment-2260007943).
export DYLD_LIBRARY_PATH="./api"
echo "Starting aarch64-unknown-linux-gnu build"
export CC=clang
export CXX=clang++
export qemu_aarch64="qemu-aarch64 -L /usr/aarch64-linux-gnu"
export CC_aarch64_unknown_linux_gnu=clang
export AR_aarch64_unknown_linux_gnu=llvm-ar
export CFLAGS_aarch64_unknown_linux_gnu="--sysroot=/usr/aarch64-linux-gnu"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER="$qemu_aarch64"
(cd librevm && cargo build --release --target aarch64-unknown-linux-gnu)
cp "./target/aarch64-unknown-linux-gnu/release/librevmapi.so" artifacts/librevmapi.aarch64.so
