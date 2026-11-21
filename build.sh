#!/bin/sh
# Build the guest so that the result does not depend on where this repository or the compiler is
# installed: this folder is recorded as /tailorbird and the compiler's own sources as /rustc/<its commit>.
# The digest printed is of the file Cargo says it wrote. Rust 1.97.0 unless RUSTUP_TOOLCHAIN says otherwise.
set -eu
root=$(cd "$(dirname "$0")" && pwd -P)
cd "$root"
export RUSTUP_TOOLCHAIN=${RUSTUP_TOOLCHAIN:-1.97.0}
sysroot=$(rustc --print sysroot)
commit=$(rustc -vV | sed -n 's/^commit-hash: //p')
[ -n "$commit" ] || { echo "the compiler does not say which commit it is" >&2; exit 1; }
written=$(CARGO_PROFILE_RELEASE_LTO=off cargo build --locked --release --target x86_64-unknown-none --message-format=json \
  --config "target.x86_64-unknown-none.rustflags=[\"--remap-path-prefix=$root=/tailorbird\", \"--remap-path-prefix=$sysroot/lib/rustlib/src/rust=/rustc/$commit\"]" "$@" \
  | sed -n 's/.*"executable":"\([^"]*\)".*/\1/p')
[ "$(printf '%s\n' "$written" | grep -c .)" = 1 ] || { echo "Cargo did not name exactly one program: $written" >&2; exit 1; }
sha256sum "$written"
