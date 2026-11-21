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
    cd guest
    cargo test --lib
    CARGO_PROFILE_RELEASE_LTO=off cargo build --release --target x86_64-unknown-none

The build must be run from `guest/` (its linker settings are in `guest/.cargo/config.toml`). Checked on
21 November 2026 at this pin: 99 library tests pass; the ELF is 149,304 bytes.

## Next

The look is to be redrawn to match the PCP OS desktop (`pcp-os` issue 334), then the model worker.
