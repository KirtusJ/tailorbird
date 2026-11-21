# Live work presentation

This is a reusable Rust presentation layer for Z3's Tailorbird redesign. It shows
task steps, measured progress, an evolving preview and interrupted states. PT's
connected source and transport are still pending. The normal Tailorbird binary
remains disconnected; it does not run the sample sequence.

## Try the separate native demo

Build with the same pinned tools as the regular guest:

    sh build.sh --offline --features live-work-demo --bin tailorbird-work-demo

Load `target/x86_64-unknown-none/release/tailorbird-work-demo` into the pinned
kernel. Space advances sample work. W waits, Space resumes, S requests stop,
C confirms stop, D disconnects, F fails, R replays and Esc exits. Every screen
has a permanent **UI DEMO / sample events / no files are written** banner.
The sample image develops before completion. The demo's Applied label is a
sample event; it never writes `demo/greeting.card` or executes a build.

Host renders of the same Rust UI:

    cargo run --locked --offline --example live_work_frames -- render-output

This writes twelve states at 640x480, 800x600, 1024x768 and 1920x1080. Hosted
native checks compare every pixel of all twelve states on emulated PC and Q35
at 1024x768, then require the kernel's clean exit. Smaller host renders are not
evidence that the current single-page UEFI framebuffer mapping works.

## Integration boundary

`live_work::Work` owns one task, up to six ordered steps and one 64x40 RGB
overview snapshot. `live_view::paint` draws it through the existing `Frame`;
the controller owns repaint timing and can place this surface in Z3's redesign.
`live_demo` only supplies sample events. The normal guest does not dispatch them.

`Update` is an in-process UI API, **not an agreed PT protocol**. The future
adapter authenticates the producer, correlates task identity, performs actual
work and reads back effects. UI state grants nothing. Source/log output is never
interpreted as code or permission by this layer. Keep original artifacts and
exact diffs outside the bounded overview; this bitmap is not proof of their bytes.
Opening the real result and richer previews remain adapter/redesign work.

The model rejects wrong-task, duplicate and out-of-order events. Preview
revisions increase, retain their location, and cannot regress Applied to Proposed.
Labels must fit printable ASCII exactly; invalid snapshots/labels are rejected
without advancing the accepted sequence. No percentage appears without a valid
reported denominator. Each step finishes explicitly; 100% alone is not task
completion. The denominator is fixed once known for a step.

Stop requested, stopped, failed and disconnected are distinct. Stop requests
and disconnects freeze ordinary updates and retain the last preview. Terminal
readback can settle the task; a connection return alone cannot restart it.
Reconnection requires a reconciled service snapshot, which is not implemented
here. `request_stop()` changes presentation only: the controller must separately
request the worker's stop. No repeated permission prompts are added.

Acceptance of a connected release still needs a native run with actual worker
events and intermediate artifact updates before completion, plus cancellation,
disconnect/reconciliation and effect-readback checks. The demo establishes UI
behavior only. Coordination: PCP OS #334 and #276, Director C3 with Z3.
