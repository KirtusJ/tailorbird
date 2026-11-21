# Tailorbird

Tailorbird is the PCP OS application for working with a local model worker. This repository is its
home. The kernel and the guest library stay in `pcp-os`, pinned here as a submodule.

## What is here, and what is not

- `guest/`: the native user-mode program, imported with its history from the public work in
  `KirtusJ/pcp-os`: pull request 134 (Session A, head `3c762e6`) and the later stack up to pull
  request 293 (head `fe8b67a`). It is a **disconnected shell**: it
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
repository and the compiler are installed. Measured on 21 November 2026, offline, Rust 1.97.0, pcp-os pin `745e883`:

| Source | Library tests | ELF bytes | SHA-256 |
|---|---|---|---|
| pull request 134 only (this repository's #1) | 99 pass | 149,304 | `f67d41ab467690545f1fc53760503c3ca399f68557760d433775e7da7b6e60b7` |
| with the stack to 293 (this tree) | 111 pass | 149,384 | `c1e79cee1b5a645062d15dc2cd9de2cf2d4306d9de34edc395052a7c2b727b00` |

Another compiler version or linker may give another file. `guest/tests/qemu_smoke.py` and
`view_scale.py` read the font from the submodule; they have not been run from this repository yet, and
the workflow that ran them in `pcp-os` is not ported. Earlier runs there do not qualify this build.

## Next

The look is to be redrawn to match the PCP OS desktop (`pcp-os` issue 334), then the model worker.
