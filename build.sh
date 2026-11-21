#!/bin/sh
# Build the guest so that the result does not depend on where this repository is checked out: the
# compiler is told to record this folder as /tailorbird. Rust 1.97.0 is used unless RUSTUP_TOOLCHAIN says otherwise.
set -eu
root=$(cd "$(dirname "$0")" && pwd -P)
cd "$root"
RUSTUP_TOOLCHAIN=${RUSTUP_TOOLCHAIN:-1.97.0} CARGO_PROFILE_RELEASE_LTO=off cargo build --locked --release --target x86_64-unknown-none \
  --config "target.x86_64-unknown-none.rustflags=[\"--remap-path-prefix=$root=/tailorbird\"]" "$@"
sha256sum target/x86_64-unknown-none/release/tailorbird
