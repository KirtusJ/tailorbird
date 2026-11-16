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
| Ui::intent(Submit{prompt,mode}) | PT authenticates actor, allocates task/sequence, validates permitted scope; TaskGate::start() if applicable | Started{task_id,seq} from service; an intent alone is not a started task |
| Ui::intent(SetMode) | PT saves the choice in the OS-controlled persistent configuration, not model context | ModeCommitted(mode), or saved_mode from a new authenticated Service status |
| Ui::intent(Stop{task_id}) | PT routes an exact task stop via TaskGate::stop(task) and enforces a destination fence | StopAccepted is not termination; Stopped only after actual endpoint/supervisor readback |
| Ui::intent(Decide{task_id,request,allow}) | PT checks pending() for the immutable action, target, actor, active task, and fresh request identity, then calls decision(req,allow) | PermissionResolved for that exact request; no automatic grant |
| BackendEvent::ToolResult | PT reads back actual tool result and renders as display data | Event with authentic task ID and increasing sequence; no implicit next tool call |
| BackendEvent::Output | Model output is displayed, never parsed as a privileged instruction by this GUI | Approval and edit actions remain separate |
| BackendEvent::Service(connected=false) | PT reconciles or fences previously running workers | UI marks active task UNCONFIRMED and disables privileged controls |

Do not let a GUI-issued boolean substitute for a verified capability. A
verified event source, request/response correlation, capability presentation
and conflict behavior are PT's service work. TaskGate belongs outside the
untrusted model process. The GUI will need an authenticated transport that
does not yet exist in the pinned guest/kernel.

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
