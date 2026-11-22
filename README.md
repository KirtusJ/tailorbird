# Tailorbird

<img align="right" src="tailorbird.svg" alt="Taylor, a small tailorbird in a yellow hard hat, with a green back and a long cocked tail, sewing the edge of a leaf with a thread held in her bill" width="120">

Tailorbird is the PCP OS application for working with a local model worker. This repository is its
home. The kernel and the guest library stay in `pcp-os`, pinned here as a submodule.
The [catalogue](CATALOG.md) says what each folder is.

## Brand

A draft, put up for the owner to correct ([issue 30](https://github.com/KirtusJ/tailorbird/issues/30)). Nothing here is settled until the owner says so.

- **Name.** Tailorbird is the program. Taylor is who you talk to in it; she is "she".
- **The mark.** [`tailorbird.svg`](tailorbird.svg): Taylor herself. She is a tailorbird, a small bird with a green back and a long tail held up, in a yellow hard hat because she works on a building site, who builds by sewing leaves together with thread, using her bill as the needle. She is her own character and not Surfbird's parrot; the two marks share only the dark rounded tile, so that they sit together as programs of one system.
- **The small mark.** [`tailorbird-icon.svg`](tailorbird-icon.svg): her head and hard hat on a square tile, for places too small for the whole drawing. The full mark stops reading below about 64 pixels wide; this one keeps the hat and face at 16.
- **In the program.** Taylor is already on the screen: a figure of sixteen by fourteen squares in [`guest/src/view.rs`](guest/src/view.rs), green back, pale front, tail held up, an amber builder's hat, and a small hammer held out in front of her. The files above were drawn after it and do not yet match it in two things: the hat is yellow in the files and amber on the screen, and the files show her sewing where the screen gives her a hammer. Which way to bring them together is the owner's to say. The Programs list of PCP OS shows no picture for any program yet.
- **What she is for.** To help a person who is not a professional get something built or fixed, on their own computer, without the network.
- **How she speaks.** Plainly and briefly. She says what she did, what she could not do, and what she does not know. She helps and does not hover. She never says something happened unless PCP OS confirmed it.
- **What she will not do.** Act without asking where PCP OS requires asking, keep an ability the person told her to give up, or claim to be more than a program running a model on this computer.
- **Colours.** The screen keeps the ones the program already draws with ([`guest/src/lib.rs`](guest/src/lib.rs)): ground `#10283f`, panel `#18364f`, text `#f2f7fc`, quiet text `#9fb4c8`, teal `#41bad7`, amber `#e3a24b` for what needs attention, green `#8fe0c2` for what is well, red `#f08a72` for what is not. Taylor's own colours in the mark are hard-hat yellow `#f2c230`, leaf green `#7fae5c` and cream `#f1f4e4`.

## Source and runtime

- The [native foreground](guest/src/main.rs) opens the local model channel when PCP OS starts a companion. It displays Loading and Ready, sends messages, displays streamed or completed replies, and handles Stop. When no channel is available it reports that state. The [conversation test](guest/tests/talk_native.py) uses a controlled worker. Results with a real model identify the separate worker, firmware and medium. [0.5B worker trial](https://github.com/KirtusJ/tailorbird/pull/24#issuecomment-7078837787)
- The foreground can request a note, browse projects and reopen files. PCP OS presents the approval screen and returns the result. The [native note test](guest/tests/note_roundtrip.py) checks refusal, approval and reopening.
- `pcp-os/` supplies the kernel and guest library. The 22 November foreground measurement below used PCP OS submodule `070c9d5925a2c208c33c60db92e51156555407c8`. Check the gitlink of the Tailorbird revision being built.
- The original import came from [PCP OS #134](https://github.com/KirtusJ/pcp-os/pull/134) and [#293](https://github.com/KirtusJ/pcp-os/pull/293). That foreground displayed NO SERVICE. The public [note](https://github.com/KirtusJ/tailorbird/pull/17) and [conversation](https://github.com/KirtusJ/tailorbird/pull/19) integrations followed it. The observed boots cited here ran in emulation.

## Build and test

Rust 1.97.0 with `rust-src` and the `x86_64-unknown-none` target, and `lld`.

    git submodule update --init
    cargo test --lib
    sh build.sh

`build.sh` records this folder as `/tailorbird` and the compiler's sources as `/rustc/<commit>`, so the program is the same file wherever the
repository and the compiler are installed. The first two rows record the 21 November 2026 import measurements with Rust 1.97.0 and PCP OS pin `745e883`. The `ab12d96a` row was reproduced in two checkout paths with Rust 1.97.0 and PCP OS pin `070c9d5`, using `cargo test --locked --offline --release --lib` and `sh build.sh --offline`.

| Source | Library tests | ELF bytes | SHA-256 |
|---|---|---|---|
| pull request 134 only (this repository's #1) | 99 pass | 149,304 | `f67d41ab467690545f1fc53760503c3ca399f68557760d433775e7da7b6e60b7` |
| historical import through [PCP OS #293](https://github.com/KirtusJ/pcp-os/pull/293) | 111 pass | 149,384 | `c1e79cee1b5a645062d15dc2cd9de2cf2d4306d9de34edc395052a7c2b727b00` |
| [22 November, `ab12d96a`](https://github.com/KirtusJ/tailorbird/tree/ab12d96a24c4d461971123dfb57b3f18f01ec103) | 149 pass | 147,880 | `afc5bc5ad9b94accaf915fe85366606668e1e2b03c8aeeee95f3b1c113d5e115` |
| [22 November, merged conversation foreground `5492c0df`](https://github.com/KirtusJ/tailorbird/tree/5492c0df93e56aa08959c308ecb95f4d9f847738) | 209 pass | 212,352 | `076fd4812c7beaad28de2a28a1265c3640f754e126edf4e27de09f8b3c31488e` |

The last row is one offline Linux build with the same Rust version and submodule pin, using the documented release test and build commands. Image receipts identify the foreground executable that their own builders produced.

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

## Project and file components

The isolated `guest/src/project_location.rs` module supplies exact project destinations for the
fresh backend: a configurable build-folder value (default `taylor`), a selected project beneath
it, and a single filename beneath that project. A selected project and its pending destinations
retain their original folder when the default changes. Unsupported names fail without repair or
truncation. The first prototype accepts lowercase ASCII paths within the kernel's current limits.
The location type binds the destination used by the note proposal and project browser. The guest requests creation through the kernel and reports the receipt for that request. [Foreground implementation](guest/src/main.rs)

`guest/src/file_job.rs` holds the first single-file job outside model text: exact request, project,
destination, ordered steps, immutable proposal binding, and kernel result. A successful write or a
generated claim of completion does not enable Open. The controller must read the current launch's
kernel receipt, validate its wire form and supply matching identity/digests. A confirmed creation
enables that exact target; only a complete read with matching length, digest and EOF marks it verified.
Stop and uncertain transport preserve already-known effects. The controller still owns the actual
proposed bytes, hashing, device transport and viewer. This module does not authenticate caller-supplied
events, enforce permissions or establish model correctness. The foreground connects the project list and read-only text viewer through the [project browser](guest/src/projects.rs). Build-folder changes apply on the next
launch; the current job retains its original destination. The live-work renderer can show all 120
destination bytes across two rows; an independent pinned-font check covers all four supported sizes.

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
