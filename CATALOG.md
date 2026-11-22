# Catalogue of this repository

Every entry at the top of the repository, and every folder one level down in `guest`, with one line on what it is. `guest/tests/check_catalog.py` fails when a tracked path of that kind is missing here or a path listed here no longer exists, so this page cannot fall behind the tree. The check is PCP OS's own, run from the pinned `pcp-os`, with this repository's folders.

"In the release" says whether the path's content is part of what a person boots or installs. "not determined" means nobody has yet established it; it is not a quiet yes.

| Path | What it is | In the release |
|---|---|---|
| `.cargo` | Linker settings for the native build. | no |
| `.github` | Continuous integration workflows. | no |
| `.gitignore` | Paths git ignores. | no |
| `.gitmodules` | Where the `pcp-os` submodule comes from. | no |
| `build.sh` | Builds the program so that it is the same file wherever it is built. | no |
| `Cargo.lock` | The exact versions of what the program is built from. | no |
| `Cargo.toml` | The Rust workspace. | no |
| `CATALOG.md` | This page. | no |
| `docs` | Notes on the design: the live-work presentation. | no |
| `guest` | Tailorbird itself: the program that runs on PCP OS, with its tests. | not determined |
| `pcp-os` | PCP OS at a pinned commit, as a submodule: the kernel and the guest library the program is built against. | not determined |
| `README.md` | What Tailorbird is, how to build it and what has been measured. | no |
| `tailorbird-icon.svg` | The small mark: Taylor's head and hard hat on a square tile, for sizes under 64 pixels. | no |
| `tailorbird.svg` | The mark: Taylor in her hard hat, holding her hammer. | no |
| `guest/examples` | Small programs that draw the real screens on a host, or exercise the model worker, for tests and review. | no |
| `guest/src` | The source of the program and of the optional model worker. | not determined |
| `guest/targets` | The compiler target for the SSE2 model worker. | no |
| `guest/tests` | Native tests that boot PCP OS in an emulator and read the screen, and this catalogue's check. | no |
