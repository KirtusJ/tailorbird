# Tailorbird native PCP-OS GUI: bounded first draft

Owner: fresh Session A in PCP OS issue #132. Source pin:
67e57f924ce0e2c86f9250c567796b745ed633c8.
Director PT owns the model, service, authorization and cancellation semantics.
Session B owns firmware, loader, installation and recovery. This draft changes none of them.

## Status and scope

This directory contains a no-std Rust user-mode graphical preview and a typed,
bounded UI state model with Rust unit tests. The guest uses the existing
pcp-game-guest framebuffer and keyboard interface, so it remains a user process,
not an AI or service inside the kernel. The kernel launcher discovers the ELF
only when a loader actually hands it over. There is no installation, desktop
window manager, LLM, broker, IPC transport, or permission enforcement here.

The preview is intentionally **disconnected**. It reports NO SERVICE and
cannot start work, approve a grant, switch the saved mode or stop a task.
This is a safety property of the preview, not a claim that Tailorbird works
with Qwen yet. Esc returns to the ordinary launcher. Ctrl+Alt+Delete is the
kernel-owned emergency stop for a running guest. Other OS programs work without
a model, so Tailorbird must remain optional.

The native display is a full-screen app surface, not a movable desktop window:
PCP OS currently grants the framebuffer to one active guest at a time. A real
windowing system, input method, persistence and accessible controls are open.

## Build and proposed verification

With Rust 1.91+, the bare-metal target, compatible linker and the PCP OS
kernel prerequisites installed, run from the repository root:

    cargo test --manifest-path apps/tailorbird/guest/Cargo.toml --lib
    rustup target add x86_64-unknown-none
    (cd apps/tailorbird/guest && \
      CARGO_PROFILE_RELEASE_LTO=off \
      cargo build --release --target x86_64-unknown-none)

The native build intentionally changes into the guest directory: Cargo
finds its freestanding linker settings in the local .cargo/config.toml
relative to its working directory, not --manifest-path. Building from the
repository root previously produced a different low-entry ELF64 PIE;
the successful static high-address ELF was verified in GitHub Actions
run 43802969930 at commit 18710a6d.

Then separately build the pinned kernel using its documented run.sh path.
Pass the actual compiled guest ELF as a Multiboot initrd, for example:

    qemu-system-x86_64 -m 256M \
      -kernel PCP_OS_Rust/crates/pcpos-kernel/target/pcpos-kernel.elf32 \
      -initrd "apps/tailorbird/guest/target/x86_64-unknown-none/release/tailorbird Tailorbird"

Check the native menu includes Tailorbird, press Enter, inspect the two
always-visible controls, type an ASCII request and press Enter; confirm
that the UI displays NO MODEL SERVICE and no task or grant is recorded.
Press Alt+M and Alt+S to check their truthful unavailable-state notices.
Press Esc and confirm the original launcher returns. Separately test
Ctrl+Alt+Delete as a kernel-level stop and review the task register.
No replay or hardware boot was performed by this authoring session.

## Exact request editor, source and limits

The original standalone 5x7 renderer could hide input after byte 92 and
fold lowercase into uppercase even though the reducer retained and submitted
the full 120-byte request. This was a source-level counterexample to
human-visible input fidelity, not a demonstrated service exploit.

The updated native guest now uses the *existing PCP kernel's generated*
DejaVu Sans Mono printable-ASCII coverage table, linked from
PCP_OS_Rust/crates/pcpos-kernel/src/font.rs. That table is 10x20 pixels
per character and preserves case and ASCII punctuation. Its provenance,
generator and license are maintained with the kernel source. This avoids
silently copying a font into a new asset. This source reference must be
rechecked if the kernel font is revised or relocated.

The REQUEST editor draws two rows of 78 columns at fixed geometry
(960x540 surface, x=140, y=460, 10x20 glyph cells), enough to show every
one of the 120 admitted printable ASCII bytes. No additional truncation,
case folding or invisible suffix is permitted. The drawing method
preflights renderability, pixel-buffer extent and footprint before
modifying pixels; the UI's Submit-enable condition also checks that the
request can be shown. The US-PC Set-1 decoder now maps the normal shifted
and unshifted printable key set, including underscores, braces, backslash
and quotation marks. This remains a fixed US keyboard layout, not
automatic locale detection, text composition, clipboard, Unicode or IME.

New Rust tests compare *actual raster pixels* for case-sensitive paths
and punctuation, assert visible last-byte glyphs at lengths 92, 93 and
120, compare the stored text and the exact Submit intent, reject
unrenderable strings without framebuffer writes, and cover all printable
ASCII keyboard codes. A framebuffer test does **not** establish that a
human can recognize every glyph or that a QEMU guest submitted the
correct bytes: PT must still inspect native screenshot pixels and
kernel-service request readback before attaching a model or tool adapter.

**Separate unresolved content:** ordinary TASK OUTPUT and notices still
use the original fixed-width five-by-seven status-label renderer, which
folds case, clips at 92 bytes and can omit unsupported glyphs. They are
not a fidelity-preserving source-file or audit viewer. The permission
ACTION/TARGET panel deliberately keeps the previously fail-closed
5x7 policy and remains restrictive for lowercase, underscore and
Unicode paths. Do not generalize the new request-editor guarantee to
output, full-source editing, immutable-grant disclosure, mouse input
or the distinct integrated desktop renderer reported by PT.

## The proposed service-facing contract

The library's Action produces only an Intent. A submit, per-task Stop,
mode request and grant decision require a real delivery/authorization channel.
The UI is **not** an authorization oracle. A mode is displayed as saved only
after an authenticated ModeCommitted or Service event from PT's service.
StopAccepted means merely that the service has taken the request;
Stopped means the service reported termination, which itself still requires
PT's supervisor-side readback to substantiate. Disconnect leaves an active
task UNCONFIRMED. The UI never converts model-generated text into an action.

**One-shot permission intent hardening (Session A revision):** once the person
presses Allow or Deny for an identified pending request, the reducer records
`decision_pending` and refuses repeat or contradictory button presses for
that request until a matching, newer `PermissionResolved` event is received
from PT's trusted service. The visible panel changes from `ALT A / ALT D` to
`DECISION SENT` while awaiting acknowledgment. Stop/terminal/disconnect
paths disable or discard pending choices, without implying those choices were
committed, canceled or enforced by the service. This avoids duplicate GUI
dispatches but **does not provide idempotency or authorization** by itself.
PT must still bind user, task, immutable action, destination and request to
one service-side decision, including replay/late-message rejection.

**Stop/permission ordering correction (PT-requested review):** a trusted
`StopAccepted` event is an acknowledgment of a stop request, *not* worker
termination. Once received, the UI now hides any preexisting permission
proposal, clears the GUI's pending choice and rejects later
`PermissionNeeded` events for that task even if the incoming event has a
higher sequence number. A delayed `PermissionResolved` likewise cannot
reopen a permission panel. The screen remains `STOP REQUESTED` and makes
no new grant available. This is a GUI presentation invariant, **not**
evidence that the trusted service prevented or rolled back a filesystem
write. An effect that was approved and executed before the stop fence
may already have occurred. The service must record its actual write/stop
ordering and report the resulting artifact independently.

**Non-ambiguous permission target display (Session A review):** the
existing shared 5x7 text renderer displays uppercase Latin letters,
digits, slash, dash, dot and plus; it uppercases incoming labels and
silently draws unsupported glyphs as blank. Originally the action and
target also shared one row, so a long visible action overlapped a
target and the decision hint. This could show two *different* effect
destinations identically, or conceal a suffix, precisely when a person
is deciding to Allow a write. The reducer now refuses to expose either
Allow **or** Deny for a metadata-bearing permission event unless the
entire action (1..48 bytes) and target (1..72 bytes) consist of exactly
renderable characters. Only action labels may contain internal spaces;
targets disallow spaces and lowercase because case is semantically
meaningful for filesystem paths. A rejected proposal leaves no
permission button, logs a bounded `ANOMALY`, marks the task
`STATUS UNKNOWN` and keeps task-scoped Stop available. The renderer
now gives ACTION and TARGET distinct rows and fits all 48/72
permitted glyphs within the panel at the built-in 6-pixel advance.
Three new adverse/boundary tests were authored. This conservative
prototype behavior **blocks common real paths** such as
`/home/name/file.txt`, paths containing underscores, Windows drive
syntax and Unicode. It must not be turned into an operative
daily-driver approval surface until a full exact target viewer and
trusted confirmation flow exist. The service must independently
default-deny undecidable effect requests; a GUI refusal alone does
not prevent an actual write.

**Service grant / human-choice discrepancy:** a pending `PermissionNeeded`
event is a request for explicit consent. The UI now stores whether the person
chose Allow or Deny for that exact request. If the service reports
`PermissionResolved(allowed=true)` after a local Deny **or no local
choice**, the reducer displays `SERVICE GRANT MISMATCH - EFFECT UNKNOWN`,
records a bounded `ANOMALY` line and moves the active task to
`STATUS UNKNOWN`. Tool output and further approvals remain blocked;
a task-scoped Stop intent stays available. A later trusted terminal
readback can resolve the uncertain task. This detects a conflicting service
**report**, not an actual write or malicious action. PT's native service
must authenticate events and preserve the exact task, request, target,
decision and effect readback in its own durable record. An automatic denial
without UI approval, or a service denial despite a local Allow, remains a
valid refused/denied outcome. The UI does not override PT's effect policy.

**Persistent operating-mode readback:** `SetMode` is a user request,
not confirmation. Tailorbird now compares `ModeCommitted(mode)` to the
actual outstanding GUI choice. A mismatched response displays
`MODE NOT APPLIED - SERVICE REPLIED DIFFERENTLY`, while a matching one
displays `MODE CONFIRMED BY SERVICE`. A mode update with no outstanding
GUI request is shown as `SERVICE MODE UPDATE - NO UI REQUEST`. Likewise,
a `Service{saved_mode}` snapshot after a pending change distinguishes
`MODE MATCHES SERVICE STATUS` from `MODE NOT APPLIED - SERVICE STATUS`.
All such reports still require an authenticated PT service adapter and
durable storage before the selector can claim persistence across reboot.
Three new reducer tests cover those distinctions. This is not an OS
configuration save, nor evidence that a model task actually used the mode.

**Completed-task ID replay hardening:** a service event that attempts to
start the **immediately preceding task ID** again is rejected, even with a
larger sequence number. A matching terminal state and displayed final event
are not erased. Two new tests cover completed/stopped/failed tasks and a
fresh, distinct next task. This is only an adjacent-ID replay defense in
the presentation layer. It does **not** enforce global task-ID uniqueness,
detect reuse of older task IDs from earlier history, or authenticate events.
PT's trusted service must issue fresh IDs within a durable epoch and
reject replay before any system effect or terminal claim.

**Unresolved neighboring risk:** repeated Submit or Stop intents are still
possible before the service supplies its first event receipt. The trusted
adapter needs idempotency keys, reliable correlation/reconciliation and
cancellation fences; GUI state alone cannot safely solve transport retries.
On reconnect an active task remains UNCONFIRMED. A task-scoped Stop
**intent** becomes available again to seek cancellation, but it is not a
claim of actual termination. The reducer also accepts a matching, strictly
newer `StopAccepted` acknowledgment after reconnect and changes the UI from
`STATUS UNKNOWN` to `STOP REQUESTED`; this acknowledges delivery only,
not the effect of stopping the worker. An additional negative test checks
wrong/stale stop receipts, accepted stop and eventual terminal readback. The reducer accepts a matching, strictly newer
`Stopped`, `Completed`, or `Failed` event only after reconnection to
settle the unknown task; it still rejects new output, tools and grants for
that uncertain task. This prevents a GUI-only deadlock while keeping service
origin authentication, durable replay, worker termination verification and
the full reconnect/resync design under PT's control. A forged or prematurely
reported terminal message must be refused by the trusted adapter. No live
recovery or termination is claimed.

BackendEvent carries monotonically increasing per-task sequence numbers.
Stale, wrong-task and post-terminal events are refused by the UI reducer.
Task text is bounded and normalized to printable ASCII for this font.
The display keeps only the newest eight lines, rather than claiming to
be a durable audit log. Grants show the specific action and target; a visible
Allow/Deny intent remains pending until service readback. Actual identity
verification, permission evaluation, persistence, tool execution, stop
propagation, cancellation fences, and task histories belong to Director PT.

No transport binding is supplied by this draft. A mock or a programmatic
BackendEvent may demonstrate the view reducer, not a live model or an
authenticated host receipt. Director PT should approve the exact service API,
source of task IDs and sequences, disconnection recovery, and the storage
location for the ongoing/single-request setting before any adapter is linked.

## Test coverage and open defects

The Rust reducer originally had **29 passing cases at commit 18710a6d**, and the new exact-input renderer adds **7 additional authored cases** (36 total): offline refusal, text bounds,
one-shot permission choice and contradictory-input rejection, stop-acknowledgment
permission refusal and prompt retraction, reconnect reconciliation only
after task-matched terminal readback, immediate task-ID replay refusal,
service-grant/no-GUI-consent mismatch and a service denial after Allow,
post-reconnect stop acknowledgment without terminal overclaim,
conservative Stop
intents while the recovered status remains unknown, task stop/disconnection,
and injection-like
model output, exact and unrenderable permission metadata including target overflow and glyph aliases, matching/mismatched/unsolicited mode readback, stop request versus termination,
wrong-task and out-of-order events, explicit grant decisions, stop/permission
race, lost-service unconfirmed status, post-terminal rejection and bounded
log order. The original 29 reducer cases passed on exact-commit
GitHub Actions run 43802969930 in debug and release with a static
native ELF build. The 7 new editor/keyboard cases are authored and
**not yet confirmed by the executor at this revision**. No kernel/QEMU
boot, service integration, human usability or effective permission
enforcement is established by passing host tests.

Remaining daily-driver prerequisites include persistent storage, concurrent
GUI processes/window management, text input and accessibility support, keyboard
and pointer hardware diversity, signed installation and updates, recovery,
network and offline operation, service isolation, effective revocation,
trusted user confirmation, and supervised end-to-end tests.

Design references (not conformance certifications):
- W3C WCAG 2.2, keyboard access and pause/stop/status principles:
  https://www.w3.org/TR/WCAG22/
- MCP authorization specification; HTTP scope minimization does not
  automatically apply to a local device capability service:
  https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization
- PCP OS existing native guest and launcher:
  store/native/guest/ and PCP_OS_Rust/crates/pcpos-kernel/src/session.rs
