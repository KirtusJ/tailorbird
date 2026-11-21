# Tailorbird

Tailorbird is the PCP OS application for working with a local model worker. This repository is its
home. The kernel and the guest library stay in `pcp-os`, pinned here as a submodule.

## What is here, and what is not

- `guest/`: the native user-mode program, imported with its history from the public draft
  `KirtusJ/pcp-os` pull request 134 (Session A, head `3c762e6`). It is a **disconnected shell**: it
  draws, takes keys and reports NO SERVICE. No model, service, permission enforcement or installer is
  in this repository.
- `pcp-os/`: submodule at `745e8831cb25090ce478fcf246dcf6c8a65ec7b2` (branch `z3/tailorbird-audit`:
  master `53a551a` with pull request 134 merged). The guest needs `Display::current()` from that
  tree's `store/native/guest`; master does not have it yet. The pin moves to master once it does.
- The connected implementation recorded in `pcp-os` issue 132 (private revisions `e725d56`,
  `6cc9946`) is **not here** and has not been seen by the people filling this repository.
- Nothing here has been run on hardware.

## Build and test

Rust 1.97.0 with `rust-src` and the `x86_64-unknown-none` target, and `lld`.

    git submodule update --init
    cargo test --lib
    sh build.sh

`build.sh` records this folder as `/tailorbird` and the compiler's sources as `/rustc/<commit>`, so the program is the same file wherever the
repository and the compiler are installed. Checked on 21 November 2026 at this pin, offline, in two folders, the second
holding only the four pieces of `pcp-os` the build reads (guest library, `font.rs`, `font-2x.bin`,
`guest.ld`): 99 library tests pass; the ELF is 149,304 bytes, SHA-256
`f67d41ab467690545f1fc53760503c3ca399f68557760d433775e7da7b6e60b7`. Another compiler version or linker
may give another file.

## Next

The look is to be redrawn to match the PCP OS desktop (`pcp-os` issue 334), then the model worker.
