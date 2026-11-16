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
    cargo build --release --target x86_64-unknown-none \
      --manifest-path apps/tailorbird/guest/Cargo.toml

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
claim of actual termination. The reducer accepts a matching, strictly newer
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

The Rust unit suite now includes **22 authored cases**: offline refusal, text bounds,
one-shot permission choice and contradictory-input rejection, stop-acknowledgment
permission refusal and prompt retraction, reconnect reconciliation only
after task-matched terminal readback, immediate task-ID replay refusal,
service-grant/no-GUI-consent mismatch and a service denial after Allow,
conservative Stop
intents while the recovered status remains unknown, task stop/disconnection,
and injection-like
model output, deferred mode readback, stop request versus termination,
wrong-task and out-of-order events, explicit grant decisions, stop/permission
race, lost-service unconfirmed status, post-terminal rejection and bounded
log order. These tests were **authored, not executed** in this absorption
environment (no Rust toolchain or QEMU here). They are not proof of
kernel execution, usability, permission enforcement, real cancellation,
or a native service connection.

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
