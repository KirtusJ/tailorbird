# Tailorbird

Tailorbird is the PCP OS application for working with a local model worker. This repository is its
home. The kernel and the guest library stay in `pcp-os`, pinned here as a submodule.

## What is here, and what is not

- `guest/`: the native user-mode program, imported with its history from the public work in
  `KirtusJ/pcp-os`: pull request 134 (Session A, head `3c762e6`) and the later stack up to pull
  request 293 (head `fe8b67a`). It is a **disconnected shell**: it
  draws, takes keys and reports NO SERVICE. No model, service, permission enforcement or installer is
  in this repository.
- `pcp-os/`: submodule at `070c9d5925a2c208c33c60db92e51156555407c8`, a commit of pcp-os `master`
  (it has `Display::current()` since pcp-os pull request 337). The four files the build reads are
  the same as at the earlier pin `745e883`, and the program built is the same file.
- The connected implementation recorded in `pcp-os` issue 132 (private revisions `e725d56`,
  `6cc9946`) is **not here** and has not been seen by the people filling this repository.
- Nothing here has been run on hardware.

## Build and test

Rust 1.97.0 with `rust-src` and the `x86_64-unknown-none` target, and `lld`.

    git submodule update --init
    cargo test --lib
    sh build.sh

`build.sh` records this folder as `/tailorbird` and the compiler's sources as `/rustc/<commit>`, so the program is the same file wherever the
repository and the compiler are installed. Measured on 21 November 2026, offline, Rust 1.97.0, pcp-os pin `745e883` (unchanged by the move to `070c9d5`):

| Source | Library tests | ELF bytes | SHA-256 |
|---|---|---|---|
| pull request 134 only (this repository's #1) | 99 pass | 149,304 | `f67d41ab467690545f1fc53760503c3ca399f68557760d433775e7da7b6e60b7` |
| with the stack to 293 (this tree) | 111 pass | 149,384 | `c1e79cee1b5a645062d15dc2cd9de2cf2d4306d9de34edc395052a7c2b727b00` |

Another compiler version or linker may give another file. The
[`Tailorbird native guest` workflow](.github/workflows/tailorbird-guest.yml) runs on pull requests
and manual dispatch, using Rust 1.97.0 and the exact proposed commit with its submodule:

- Debug and release library tests, the framebuffer padding/page-flip test, and static high-address
  ELF64 inspection. `build.sh --offline` produces the guest once.
- Separate PC and Q35 keyboard/scanout/offline-refusal/exit tests at an explicitly requested
  1024x768 against the **current OS submodule**.
- Separate 1920x1080 and 800x600 scale, glyph, refusal and complete launcher-return tests against
  **historical kernel `8f5992807e9b9c0bb8ce64e366c85ee4add31599`**. `view_scale.py`'s exact launcher
  pixels and power key describe that desktop. These jobs do not qualify the current desktop's
  launcher or power flow. The workflow requires its font sources to match the guest's.

Every QEMU job downloads the producer's ELF and verifies its SHA-256 before use. Native jobs run
independently and retain original PPM frames, serial logs, source/compiler identities and result
records for 14 days, including failures. A pass applies only to the source and bytes recorded in
that run; earlier `pcp-os` runs do not qualify this build. These are model-free emulation checks.

On Linux with QEMU, after building the guest and the pinned kernel:

    sh pcp-os/PCP_OS_Rust/crates/pcpos-kernel/run.sh build
    python3 guest/tests/qemu_smoke.py \
      pcp-os/PCP_OS_Rust/crates/pcpos-kernel/target/pcpos-kernel.elf32 \
      target/x86_64-unknown-none/release/tailorbird --machine pc --output native-captures/pc

Repeat with `--machine q35 --output native-captures/q35`. Output directories must be new. The
historical scale commands and exact kernel checkout are recorded in the workflow. Both Python
runners refuse optimized Python, which would otherwise remove their assertions.

## Next

The look is to be redrawn to match the PCP OS desktop (`pcp-os` issue 334), then the model worker.

The isolated `guest/src/project_location.rs` module supplies exact project destinations for the
fresh backend: a configurable build-folder value (default `taylor`), a selected project beneath
it, and a single filename beneath that project. A selected project and its pending destinations
retain their original folder when the default changes. Unsupported names fail without repair or
truncation. The first prototype accepts lowercase ASCII paths within the kernel's current limits.
This is tested location data, not a connected setting or filesystem feature: it creates nothing,
checks no availability or collisions, and supplies no permission or durability guarantee. Z3 owns
the settings, project list, kernel namespace and Files integration; C3 owns this destination-binding
logic. Preview, kernel request and result opening must eventually use the same exact destination.

`BuildFolder::parse_project_file(full_path)` lets a browser recover the selected project and exact
file destination from a returned name. It requires the current folder plus a component separator,
then exactly one project and one filename; similar prefixes, outside paths and unsupported layouts
are rejected. The returned values do not prove that the entry exists or that listing succeeded.
An unavailable, incomplete or unsupported listing must not be presented as an empty folder.

`build_folder_snapshot::Snapshot` decodes the agreed version-1 kernel listing without allocation
or I/O. The controller must read the actual `dev/build-folder` descriptor through EOF, refuse
more than 64 KiB (including an extra-byte check at the limit), and close it before parsing. The
decoder validates all records before exposing any entry, retains the kernel's current root and
launch ID even for an empty list, and offers `parse_for` to check that context on refresh. It
rejects malformed, truncated, unsorted, duplicate or out-of-root records. `is_known_empty` is
true only for a complete zero-entry list; `file_counts` keeps unsupported layouts as other files.
The viewer must still read the selected file and verify actual bytes against its creation receipt;
listing sizes and the decoder are not proof that a file exists, is unchanged, or was created.
