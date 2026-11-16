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

The REQUEST editor draws two rows of 78 columns in a logical 960×540 layout
(x=140, y=460, 10×20 cells). The guest draws directly at the inherited display
resolution and respects its row stride. At 1920×1080 the default Fit view uses
20×40 physical pixels per editor cell. Press F6 to switch to the centered
Compact view with 10×20 cells, or press it again to restore Fit. On small
screens both views shrink to keep the full layout visible. The choice lasts
until Tailorbird closes. The two rows have room for every
one of the 120 admitted printable ASCII bytes. No additional truncation
or case folding is permitted. Trailing spaces still have blank source
font glyphs, so the left margin now displays **BYTES n/120**: entering or
removing a trailing space changes a visible byte count, even when the
glyph raster is otherwise identical. This is a count cue, not a visual
space marker or general accessibility certification. The drawing method
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
ASCII keyboard codes, including pairwise pixel distinction for all 95
printable-ASCII glyphs. A framebuffer test does **not** establish that a
human can recognize every glyph or that a QEMU guest submitted the
correct bytes: PT must still inspect native screenshot pixels and
kernel-service request readback before attaching a model or tool adapter.

TASK OUTPUT draws each retained printable ASCII byte with its original case. Each
row fits the full 84-byte message, including its final cell. The reducer replaces
control and non-ASCII bytes with `?` and keeps at most 84 bytes from each
incoming event. A blank space has no visible ink. The permission inspector
and request editor have separate length indicators. The integrated desktop,
live service output and human legibility need their own checks.

## Ordered keyboard events: preserve the text actually typed

Director PT independently reproduced an adverse QMP event batch at
[PCP-OS #132 comment 6907744157](https://github.com/KirtusJ/pcp-os/issues/132#issuecomment-6907744157):
the intended `z,x,z,Shift+a,a` became `azx` in the public guest, even
though the 10x20 font could render the intended source exactly. The shared
`pcp_game_guest::Display::poll()` converts the kernel's ordered keyboard
records to `pressed[128]`/final `held[128]` bitmaps. That loses repeated
keys, their arrival order, and Shift changes between presses. No UI code
can reconstruct the original sequence from only those two bitmaps.

The Tailorbird native guest now consumes the **existing** `/dev/kbd`
two-byte FIFO records in order using its own nonblocking read capability.
The A-owned `ordered_keys.rs` reduces key-down/up events sequentially,
including Shift/Alt state at the instant of each event. It does not
change the kernel, shared guest library, physical controller driver or
security grants. A second guest keyboard fd is used because the
`Display` helper's fd is private. The app does not invoke
`Display::poll()`, which would drain the stream before this reducer
could inspect it.

Five additional host tests cover PT's exact `zxzAa` counterexample,
numeric-key ordering, held Shift and repeat events, duplicate Enter
while admission remains pending, and shortcuts. The QEMU smoke script
also injects PT's same-batch QMP event stream and compares the **actual
emulator scanout** with the expected glyph pixels computed directly from
the pinned 95-character source font, as well as against the previous wrong
`azx` sequence. An earlier test incorrectly treated a *paced* capture
containing `zxzA` as the `zxzAa` oracle and falsely reported failure.
That incorrect reference and the failed CI runs remain preserved. The
corrected source-font comparison passed on both PC and Q35 in
[run 43810191329](https://github.com/KirtusJ/pcp-os/actions/runs/43810191329).
The synthetic key-tracing feature was later removed; the **normal
non-logging guest** reproduced the pass at exact source pin
`3e39c1e1` in [run 43810706604](https://github.com/KirtusJ/pcp-os/actions/runs/43810706604).

This is a source- and emulator-scoped keyboard improvement, not proof
of locale-independent input, Unicode, international keyboard layouts,
IME, clipboard or physical keyboard hardware compatibility. The
kernel's 256-entry FIFO may still drop events under overload; the
application does not yet receive an authenticated per-process dropped-
event notification. PT must resolve that boundary for consequential
live input and approve any new native device input contract.

## Trailing whitespace in the request editor

PT noted that even an exact-case ASCII font draws a space as a blank cell.
The native REQUEST panel now has a visible `BYTES n/120` readback in the
left margin. [Exact-source QEMU run 43811187850](https://github.com/KirtusJ/pcp-os/actions/runs/43811187850)
checked the actual screen on **PC and Q35**: entering a trailing space
preserved the text raster but changed the displayed count; Backspace
restored its earlier count and pixels. The full-burst `zxzAa` source-font
oracle, offline refusal and clean exit also passed. This only establishes
a visible byte-length cue for printable ASCII under these modes. A
cursor, per-whitespace markers, multi-line editor, Unicode, arbitrary
accessibility tooling and PT's service-accepted exact request bytes
remain separate qualification requirements.

## Future mouse/pointer UI adapter (not a mouse driver)

The principal's [explicit daily-driver hardware directive](https://github.com/KirtusJ/pcp-os/issues/132#issuecomment-6908486443)
requires a **keyboard, mouse/peripherals, disks, encrypted drives, Wi-Fi
and wired Ethernet**. On the public master source pin
`67e57f924ce0e2c86f9250c567796b745ed633c8`, the kernel has a
PS/2 keyboard FIFO and framebuffer, **but no user-mode pointer device**
or general USB/HID interface. Do not confuse a static hit-test with an
implemented mouse, pointer driver or physically tested peripheral.

Session A now includes `src/pointer.rs`, a pure `no_std` UI adapter
that can *later* receive OS-authenticated pointer coordinates and
button-down/up events. Its Mode and Stop rectangles are reused by
the native renderer, so the test targets and pixels agree. A primary
press and release must occur inside the same still-enabled control;
leaving the region, an explicit unplug/focus-loss Cancel event,
a foreign task, or a changed mode disables a stale click. An offline
state at Release also refuses the action. **The future trusted adapter
must send Cancel on every disconnect/reconnect, focus change and display
transition**; this pure module cannot detect an unseen connection
transition if no input event is delivered during it. Unconnected and
pending-Submit states never issue a new action from a click. Secondary,
middle or cross-button gestures cannot activate an action.

**Critically, no pointer gesture can Approve/Deny an effect or Submit
a prompt** in this version. Such actions require an exact immutable
proposal, confirmed actor and PT's trusted service, not a pointer click
surface. The normal guest does **not** call this module: it cannot
receive mouse events until PT and the firmware/input lane provide a
capability-gated pointer device and real adapter with bounded ranges,
timestamps/order, device identity and loss/unplug recovery. No
native pointer cursor, hover, dragging, wheel, USB HID, touch or
wireless mouse is claimed.

Eight new adversarial host cases validate the conservative adapter's
geometry, double-click, disabled control, wrong task, out-of-bounds,
event cancellation and denial of privileged-click behavior. This is
pre-integration source/testing work; it does not qualify a daily-driver
mouse. A future actual pointer device must be tested in software QEMU
and on physical hardware, with and without Qwen, before release.

## Pending submission admission and stable retry identity

A source-first adversarial review by Director PT at [PCP-OS issue #132 comment
6907654034](https://github.com/KirtusJ/pcp-os/issues/132#issuecomment-6907654034)
found that two rapid Enter presses could emit two Submit intents before any
Started reply. This is a reducer-level defect, not evidence of two actual
backend tasks in the deliberately disconnected preview.

The UI now assigns a monotonic **in-process idempotency_key** for each
submitted request, snapshots the exact original prompt and mode, and sets
`pending_submit` BEFORE returning its first `Intent::Submit`. Further
Enter presses and mode changes are refused pending a matching service
readback. While pending, native text editing is frozen so the visible
request remains the same as the retry payload. A **manual Alt+R** retry
can emit only the *same* key, prompt and mode, never a new request. The
UI never automatically retries after a timeout or reconnect.

The proposed trusted adapter must emit
`SubmissionStarted{idempotency_key,task_id,seq}` or
`SubmissionRejected{idempotency_key}`. An unrelated `Started`, a
wrong key, or an out-of-order reply cannot settle pending admission.
Connection loss and restoration preserve the pending submission, with
status unconfirmed until a matched service response. An explicitly
rejected request can be submitted anew under a **new** local key.

**This is not service-side idempotency or authentication.** A local
counter alone is not globally unique: PT must scope it to a durable
authenticated actor/session epoch, reject replay atomically and return
the existing task identity on an acknowledged retry. The on-disk
TaskGate must decide whether an earlier attempt started or effected
anything, including when its acknowledgment was lost. No GUI timer,
visible state, or repeated click makes an uncertain operation safe.
A matching `SubmissionStarted` is a reported task admission, **not**
evidence of an authorized filesystem write or completion.

Four new unit controls assert double Enter refusal, delayed/wrong
admission, disconnect-and-reconnect with unchanged retry payload/key,
and local-key exhaustion without wrap. At this revision the total
source suite is **41 authored tests** (37 previously verified + 4
new), with no claim that the new source passed until the pinned CI
run has actually executed.

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

The Rust reducer originally had **29 passing cases at commit 18710a6d**, and the exact-input renderer added **8 cases**, the pending-submission race **4**, and ordered keyboard handling **5** (46 previously tested plus 8 new pointer cases = 54 authored): offline refusal, text bounds,
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
log order. The prior 46 source tests **passed in debug and release**
in [run 43811187850](https://github.com/KirtusJ/pcp-os/actions/runs/43811187850),
before the eight pointer cases were added. Those eight are authored but
**must be executed at the exact new source head** for any pass claim.
which compiled the high-address native guest. Its separate QEMU job
actually booted the *public legacy kernel* with the normal, model-free
Tailorbird guest and passed both **PC and Q35** raster/input/exit smoke
checks, including PT's original `zxzAa` key burst and the trailing-space
count. These are emulator results, **not** the private integrated
Qwen/TaskGate workflow, human usability testing, physical hardware
operation or host-enforced permission and calibration.

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
