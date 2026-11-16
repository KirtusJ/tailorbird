# Tailorbird UI to Director PT service interface: proposed adapter boundary

**Status:** source-grounded integration note, not a live adapter or accepted wire protocol.
Director PT's comment on PCP OS coordination issue #132, comment 6905656189,
states the service/core interface as:

    pcpos_core::tailorbird::TaskGate
    Mode::{Ongoing,Single}
    Request {task,sequence}
    Edit::{Create,Replace}

with start(), pending(), decision(req,allow), continue_after_reply(task),
and stop(task). The Replace edit carries the expected existing-source
SHA-256. The gate is for a **trusted UI/controller**; a model reply is
untrusted proposal data.

As inspected on pinned master
67e57f924ce0e2c86f9250c567796b745ed633c8, the
PCP_OS_Rust/crates/pcpos-core/src/tailorbird.rs module was **not found**.
Thus, these names are PT's published interface statement, **not compiled
source inspected in that revision**. Do not import it or claim a live gate
until PT supplies a source pin and executable test receipt.

## Contract mapping, not a direct type cast

| Guest UI | Trusted adapter/PT service | Required readback |
| --- | --- | --- |
| Ui::intent(Submit{idempotency_key,prompt,mode}) | PT binds actor, trusted session epoch, exact immutable payload/mode and single-use admission key; atomically deduplicates before TaskGate::start() | Matching SubmissionStarted{idempotency_key,task_id,seq} or SubmissionRejected{idempotency_key}; an intent alone is not a started task |
| Ui::intent(SetMode) | PT saves the choice in the OS-controlled persistent configuration, not model context | ModeCommitted(mode), or saved_mode from a new authenticated Service status |
| Ui::intent(Stop{task_id}) | PT routes an exact task stop via TaskGate::stop(task) and enforces a destination fence | StopAccepted is not termination; Stopped only after actual endpoint/supervisor readback |
| Ui::intent(Decide{task_id,request,allow}) | PT checks pending() for the immutable action, target, actor, active task, and fresh request identity, then calls decision(req,allow) | PermissionResolved for that exact request; no automatic grant |
| BackendEvent::ToolResult | PT reads back actual tool result and renders as display data | Event with authentic task ID and increasing sequence; no implicit next tool call |
| BackendEvent::Output | Model output is displayed, never parsed as a privileged instruction by this GUI | Approval and edit actions remain separate |
| BackendEvent::Service(connected=false) | PT reconciles or fences previously running workers | UI marks active task UNCONFIRMED and disables dispatch while disconnected; reconnection does not certify stop |

Do not let a GUI-issued boolean substitute for a verified capability. A
verified event source, request/response correlation, capability presentation
and conflict behavior are PT's service work. TaskGate belongs outside the
untrusted model process. The GUI will need an authenticated transport that
does not yet exist in the pinned guest/kernel.

## Submit admission and retransmission boundary

The public GUI now rejects immediate duplicate Submit intents while a
submission remains unconfirmed. Its `idempotency_key` is a monotonic
**in-process** counter, NOT an authenticated globally unique request ID.
Director PT's trusted controller must bind this key with a persistent
session/actor epoch and an immutable payload digest, guarantee atomic
deduplication and retention across disconnect/restart, and report either
`SubmissionStarted` for that key and one actual task ID, or
`SubmissionRejected` for that key. Uncorrelated `Started` remains a
legacy/status-only event and cannot clear a pending submission.

On temporary loss, the GUI retains the same prompt/mode/key, disables new
submission and shows an unresolved status. Explicit Alt+R retry emits
the *same* key, not a fresh request; this is a proposed transport operation,
not proof that the prior submission did not run. PT must return the same
task identity if the first transmission was already accepted. Timeouts,
disconnects, and an unverified service return are not safe-to-repeat
signals; no automatic retry or new key is permitted until an authorized
status/denial receipt. Ordinary task output and tool effects remain
distinct from task admission. The native UI locks input editing during
pending submission to preserve the visual request snapshot.

PT should run lost-ack, duplicate-in-flight, identical-key/different-
payload, replay-across-epochs, service-restart and stop-during-admission
negative controls before connecting this UI to Qwen. The attached
guest-only tests establish **presentation state**, not reliable delivery,
trusted deduplication, task causality or permission enforcement.

## Exact prompt rendering versus permission disclosure

The original standalone guest request field clipped the 120-byte stored
printable ASCII input at 92 bytes and uppercased all visible characters.
The updated request editor uses the kernel's generated case-sensitive
printable-ASCII 10x20 font with two 78-character rows. Host-side pixel
tests must check that the last bytes and case of the submitted prompt
match the entire `Intent::Submit` payload, including 92/93/120
boundaries. Physical keyboard/locale and actual QEMU frame readback
remain unverified. A fixed US-PC Set-1 keyboard map is not a general
input method.

This is **not** the approval viewer. The old permission panel still
has narrow renderability rules, so PT must deny effect requests it
cannot fully disclose there. The standalone UI output feed likewise
still uses a lossy five-by-seven status-label renderer. Neither should
be used as evidence that a model-generated program, its source digest,
or immutable destination was displayed exactly. PT's separate native
integrated desktop uses its own renderer and must be assessed against
its actual source, pixels and actor-bound effect records.

## Exact visual disclosure before any Allow

This UI's native 5x7 renderer does **not** preserve lowercase case,
underscore, backslash, colon, non-ASCII and control characters.
A truncated or case-folded pathname must not be presented as the exact
destination for a potentially privileged write. The reducer therefore
fails closed on `PermissionNeeded` if the full action/target cannot
be represented literally and within the fixed text limits; an
ambiguous request cannot emit `Intent::Decide(allow=true)` or
`Intent::Decide(allow=false)`. The state becomes `Unconfirmed`,
with an anomaly in the UI and Stop still offered. The guest now draws
ACTION and TARGET on different rows so even maximum-length allowed
strings stay visible. This is intentionally restrictive: lowercase
Unix paths, underscores, Unicode and Windows path separators are not
yet eligible for confirmation. PT must still default-deny undecidable
requests, apply its independent stop fence and provide an eventual
readback. A future full text viewer must demonstrate exact case,
controls, Unicode, overflow, action identity and actual immutable
source-hash disclosure in the approval workflow. Never silently
normalize a display string and treat it as informed consent.

## Service-reported grant contradicting UI consent

A `PermissionNeeded` event means the UI is awaiting a human decision for
a specific immutable proposal. The reducer now tracks the first GUI
`Decide{request,allow}` intent. If the trusted service later reports
`PermissionResolved(allowed=true)` despite an explicit GUI Deny or without
any UI Allow for that request, Tailorbird does **not** render it as ordinary
user authorization: it records an `ANOMALY`, marks the task status
`Unconfirmed` and prevents further UI grants/tool event consumption while
offering an exact-task Stop intent. This is a comparison of two *reported*
pieces of evidence, not proof that the service wrote bytes.

PT must bind service decision/outcome to verified actor identity, task, request,
immutable edit destination and expected source hash. A durable service log
must retain the discrepancy and the actual before/after effect even if the
GUI cannot display the full event history. If an automatic policy granted
something without asking for consent, it must not be encoded as a
`PermissionNeeded` request requiring human Allow; use an explicit,
separately reviewed policy/report type. An automatic refusal is compatible
with UI security and does not require a prior GUI Deny. UI-only state
checks never substitute for TaskGate enforcement or filesystem readback.

## Mode persistence and readback

A user command `SetMode(Ongoing|Single)` does not prove the saved
configuration changed. For the existing GUI to call a transition
**confirmed**, the trusted adapter must return a `ModeCommitted`
matching the outstanding requested value. A different returned value
must be displayed as **not applied**, not as success. An unsolicited
service mode update can be displayed as its authoritative *reported*
value, but must not be credited as the person's new request. The same
comparison applies when `Service{saved_mode}` is received before
the pending command's receipt. PT owns durable mode storage, receipt
correlation, caller identity and actual worker mode selection; the GUI
has no direct authority over them.

## Edits must remain explicitly scoped

For Edit::Create, the service needs an authorized destination and clear
collision rule. For Edit::Replace, PT requires the immutable expected
source SHA-256 and must reject a stale/wrong source before writing. The GUI
shows that exact target and effective edit to the person before sending a
grant decision; it does not synthesize permission from a generated rationale.
One grant should bind exactly one immutable edit request, task and target.
A new edit, changed hash or changed target requires reauthorization.

## Stop, mode and offline invariants

- Keep Stop and the ongoing/single-request control visible at all times.
- After trusted `StopAccepted`, the UI rejects new permission proposals
  and retracts any earlier visible grant request. It permits no
  `Intent::Decide` while `StopRequested`. A late service grant receipt
  does not establish a new UI permission. If an edit was authorized or
  executed *before* the service enforced its stop fence, only PT's trusted
  tool readback can establish the time, bytes and effect; the GUI cannot
  roll back or erase that event.
- After a service reconnects with an unresolved task, the GUI offers an
  exact task-scoped Stop **intent** without enabling tool or permission
  output. It accepts a newer, task-matched StopAccepted acknowledgment and
  then displays STOP REQUESTED, **not** STOPPED. Only a newer matching
  Stopped, Completed or Failed event from PT's trusted adapter can resolve
  the terminal state. The adapter must authenticate the event, prohibit
  task-ID reuse and perform any required supervisor readback; a GUI state
  transition alone proves no effect.
- Reconnecting is not a new task. Incoming output, tool results and
  permission requests while UNCONFIRMED are rejected by the reducer.
  The service must supply a separate, explicit reconciliation protocol
  if it intends to resume or adopt a running task instead of terminating it.
- Stop must interrupt active work through PT's real execution fence. A UI
  click, intent, exit from the GUI, or disappearance of output is not proof
  that the worker stopped. Use actual terminal readback and disclose residual
  effects and timeouts.
- Mode choice is durable only on service acknowledgment. No persisted
  configuration exists in the current kernel, so the preview displays a
  confirmed default only, not a saved user preference.
- When Qwen is uninstalled, loading, fails or runs fully offline, ordinary
  OS programs still launch; Tailorbird reports unavailability without
  blocking the system or attempting network access.
- The app currently renders a single, full-screen guest. A real desktop
  window, general IPC, durable model files and accessible pointer/text input
  require separate OS functionality.

## Test handoff to PT

1. Pin and inspect the actual Rust TaskGate implementation and its tests.
2. Run cargo test on this guest's UI reducer, with ordinary and optimized
   builds. Failures should use test assertions processed in both builds.
3. Build the x86_64 user ELF and feed it to the Multiboot launcher in QEMU;
   inspect the guest image, navigation, visible Stop/mode and Esc return.
4. Simulate stale or duplicated task IDs, mismatched permission request,
   wrong expected source SHA, Stop races and lost-service reconnect. The
   adapter must not call privileged functions in a denied case.
5. With an *authorized* real PT worker, compare stop request receipt,
   destination fence, worker exit and supervisor readback separately. Do
   not mark physical termination merely because the UI shows a label.
6. Repeat with no Qwen installed and no network capability. The OS and
   non-Tailorbird programs must remain operational.

## External reference points

- W3C WCAG 2.2: keyboard access, consistently named controls, pause/stop,
  and status messages are design guidance; this guest is **not certified**
  conformant: https://www.w3.org/TR/WCAG22/
- MCP's HTTP authorization/scope minimization is a **transport reference**,
  not an authorization scheme for PCP OS's local capabilities:
  https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization
- The kernel's actual guest and launcher interfaces are in
  store/native/guest/src/ and PCP_OS_Rust/crates/pcpos-kernel/src/session.rs.
