# Tailorbird, built fresh: what the parts are and who is trusted

Status: **draft for the directors (Z3, 22 November 2026). Nothing here is built.** The owner decided to
stop waiting for the earlier private implementation and start again. This page is the agreement to reach
before any code that can change files or the system is written.

## What the owner asked for

Visual in what it does; clear in what it did; able to act on the whole system when explicitly allowed;
"can we build it, can we fix it: if we can, yes".

## What PCP OS gives us today (read from the kernel at `070c9d5`)

- One program at a time has the display and the keyboard. There is no second running program to talk
  to: no pipes, sockets or process creation.
- A program holds only the capabilities its launch gave it (display, keys, files of its own, ...). The
  kernel checks them on every request. Ctrl+Alt+Delete stops a program and belongs to the kernel.
- A register that records what was started, by what, and how it ended.

## The design proposed

**The kernel is the gate. Tailorbird is one ordinary program and is not trusted, model included.**

1. **Tailorbird (one program):** the screen, the conversation, and the model running inside it. It can
   compute and draw as much as it likes. By itself it can change nothing outside its own scratch files.
2. **Proposals:** to change anything, Tailorbird hands the kernel a *proposal*: an exact, complete
   description of one effect (create this file with these bytes; replace that file, whose current
   digest is X, with these bytes; start this program with these capabilities).
3. **The question is the kernel's.** The kernel takes the display away from Tailorbird, draws the
   proposal itself in full (what, where, how big, what it replaces), and takes Allow or Deny from the
   keyboard. Tailorbird cannot draw on, read or answer that screen. This is the same idea as a secure
   attention screen or a permission portal on other systems.
4. **The effect is the kernel's too.** On Allow the kernel performs exactly the bytes it showed, records
   proposal, decision and result in the register, and tells Tailorbird what happened. On Deny, nothing.
   A grant is for that one proposal; "ongoing" mode may allow a *named folder* until the task ends,
   shown on screen the whole time and ended by Stop.
5. **Stop** is the kernel's key, works without Tailorbird's cooperation, and ends every grant.

What this buys: no trusted service to write and authenticate, no transport to secure, and a model that
is wrong or manipulated can only ever *ask*.

## What the present screen already models

`guest/src/lib.rs` has the intents (Submit, SetMode, Stop, Decide) and events (Started, Output,
ToolResult, PermissionNeeded/Resolved, StopAccepted, Stopped, Completed, Failed) with task and sequence
numbers and "effect unknown" handling. They stay; "the service" behind them becomes the model loop in
the same program plus the kernel's answers to proposals. `PermissionNeeded` becomes a preview only: the
real question is asked by the kernel.

## Open questions, in the order they block us

1. **What can a "built" thing be?** PCP OS has no compiler on board. Candidates: (a) a small sandboxed
   script or bytecode that a PCP OS runtime draws with; (b) documents and data only at first. Without
   an answer "build it" means "write text files".
2. **Can a model run here usefully?** Unknown: memory for the weights, speed without verified vector
   instructions or threads, and whether a 0.5B-parameter model can plan anything worth approving.
3. **"Take over the system when allowed":** which effects beyond files (settings, starting programs,
   devices) may ever be proposed, and which never.
4. **No pointer.** Everything above is keyboard only until the kernel has one.
5. Where the model file lives and how it gets onto the media.

## Not claimed

No part of this exists. Nothing has run on hardware.
