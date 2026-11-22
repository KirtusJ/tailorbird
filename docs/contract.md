# Tailorbird, built fresh: what the parts are and who is trusted

Status: **draft for the directors (Z3, 22 November 2026; B3's review of the same day taken in). Nothing here is built.** The owner decided to
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
   proposal, decision and result in the register, and tells Tailorbird what happened. On Deny or
   cancel, nothing happens and that is recorded too.
5. **Stop** is the kernel's key, works without Tailorbird's cooperation, and ends every grant.

### Grants (B3's conditions, adopted)

- **First release: every grant is for one proposal, once.** The decision is bound to the action, the
  target, the content's digest and size, and the state expected beforehand. If that state has changed
  when the kernel comes to act, nothing is done and a new decision is needed.
- If Stop arrives after the kernel has committed an effect, the receipt of the completed effect is
  still shown: a finished change is never hidden by a stop.
- **No "ongoing" grant in the first release.** It would need an indicator and a way to revoke that the
  kernel keeps on screen while Tailorbird draws, and a full-screen program's display cannot give that
  today.

### The consent screen (B3's conditions, adopted)

- Before the question is shown the kernel suspends the program, finishes or discards any picture it was
  presenting, takes the display and the keyboard, and draws the whole proposal from **its own copy**,
  taken when the proposal was made.
- This holds on every display path: one page scanned out directly, both pages of a display that flips,
  and the copied ("shadow") path.
- A key combination reserved to the kernel brings up or confirms the true screen, so a look-alike drawn
  by a program can be told from it.
- The answer defaults to Deny. Allow needs a fresh key press made after every key has been released. A
  held Enter, a repeating key, Stop, or loss of the display never approves.
- On return the kernel clears the question from every page the program can read and consumes the keys
  that answered it.
- To be checked on both 800 by 600 UEFI devices and on the two-page display, with a long target and
  with a cancelled question.

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
2. **Can a model run here at all, then usefully?** Measured by B3 at `070c9d5`: boot modules must end
   below 128 MiB; the kernel uses one pool of memory below 1 GiB; a 256 MiB machine had about 138 MiB
   free when Tailorbird started. The Qwen2.5-0.5B-Instruct file the host example uses is about 469 MiB.
   So **the kernel cannot hold such a model today**: it needs memory above 1 GiB, a way to load a file
   that size, and a budget that covers weights, context, working memory, the display and a reserve for
   Stop, consent and receipts. A machine with no model or too little memory must still reach the
   desktop. Not measured: the laptop's memory map, speed, vector instructions.
3. **"Take over the system when allowed":** which effects beyond files (settings, starting programs,
   devices) may ever be proposed, and which never.
4. **No pointer.** Everything above is keyboard only until the kernel has one.
5. Where the model file lives and how it gets onto the media.

## Not claimed

No part of this exists. Nothing has run on hardware.
