# Tailorbird, built fresh: what the parts are and who is trusted

Status: **draft for the directors (Z3, 22 November 2026; B3's and C3's reviews of the same day taken in). Nothing here is built.** The owner decided to
stop waiting for the earlier private implementation and start again. This page is the agreement to reach
before any code that can change files or the system is written.

## What the owner asked for

Visual in what it does; clear in what it did; able to act on the whole system when explicitly allowed;
"can we build it, can we fix it: if we can, yes".

And on 22 November, as the rebuild began: "taylor the tailorbird should be helpful, and interactive, but
not overbearing". Read here as rules of manner: it answers and offers, and does not interrupt, nag,
take the screen unasked, or ask again for what was refused. The kernel's question appears only when
the user's own request has led to a proposal. Also that day: "Taylor should be user friendly, not expecting a professional. Taylor should expect,
and adapt, to all users, carefully and responsibly", and, as the archetype, a cheerful builder who
says yes when the job can be done. Read here as: plain words on every screen, no term a newcomer
must look up; a request in the user's own words is enough; when something cannot be done Taylor says
so kindly and says what can; the kernel's consent screen is written for the same reader, since a
question the user cannot understand is not consent.

The mascot is named **Taylor** (owner, 22 November): the program is Tailorbird, the character in it is Taylor.

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

### Proposals and receipts (C3's conditions, adopted)

- **What the model writes is never a receipt.** Tokens, plans and half-made things can only put the
  screen into a *proposed* state. Only the kernel's receipt, matching the proposal, shows something as
  applied. A generated sentence that says "done" is nothing.
- A proposal and its receipt are bound to the task attempt, a proposal number, the exact target, the
  state expected beforehand, the bytes and their digest, and the result. Asking twice does not do the
  thing twice. The kernel keeps the receipt and can show it after Stop has ended Tailorbird.
- **The first thing that can be made is one new file of text or data** (create only, UTF-8, within
  agreed limits of size and path) that a program already on the system can open. One decision covers
  the whole file. The kernel's screen shows the exact content, with control and undrawable characters
  marked unmistakably, and refuses what it cannot show whole and never clips it. Replacing a file,
  several effects in one plan, and anything that runs come later, each with its own agreement.
- **Kept until when, said plainly.** The ordinary kernel's files last until power-off; a kept file is a
  different, acknowledged commit. A receipt says which; "created" never silently means "will be there
  after a restart". How the user gets from Tailorbird to the new file and back, on a system that shows
  one program at a time, must be a designed path.

### Where Taylor's work goes (owner, 22 November)

"Ensure tailorbird has a clean, and customizable, build to directory, with projects placed in that
directory."

- Everything Taylor makes goes under **one folder the user chooses** (`taylor` unless changed in
  Tailorbird's settings), and nowhere else.
- **Each project has its own folder inside it** (`taylor/birthday-card/…`), named by the user or
  suggested by Taylor and shown before anything is made. A project's files stay in its folder.
- The kernel's question names folder and file apart, and refuses a name that reaches outside (a part
  beginning with a dot, more than three folders deep, or the kernel's own `dev`).
- Clean means: no scratch files or leftovers in the build folder; what is there is what was allowed.
- To be built: the setting itself, a list of projects in Tailorbird, and (with Files) a way to open
  the folder from the desktop. The kernel side of the naming rule exists in pcp-os pull request 344.

### Taylor knows what it is doing, from start to finish (owner, 22 November)

"Crucial: ensure tailorbird actually knows what it's doing, from start to finish, and isn't confused
about its directive/role."

A small model forgets, drifts and can be talked out of its job by the text it reads. So the knowing is
not left to the model:

- **The role is fixed by the program, not by the conversation.** Who Taylor is, what it may and may not
  do, and how it must answer are supplied by Tailorbird on every step and cannot be changed by what
  the user types, by a file's contents or by the model's own earlier output.
- **The job is held by the program.** For each task Tailorbird keeps, outside the model: the user's
  request in their own words, the project and place, the plan as numbered steps, which step is being
  done, and what has been made (by the kernel's receipts, not by the model's say-so). The model is
  given that state each time; it is never asked to remember it.
- **Text is material, never orders.** Words inside a file, a result or a preview are data. An
  instruction found there is not followed; if it matters, Taylor shows it to the user.
- **Every step is checked against the job.** What the model proposes must fit the current step and the
  request (right project, right kind of thing, nothing asked for that was not requested). What does
  not fit is discarded and the step retried or the user asked; it is never shown as progress.
- **When it does not know, it says so.** Unclear request, a step that keeps failing, an answer that does
  not parse: Taylor stops, says in plain words what it could not do and what the user can do next. It
  does not guess, loop or pretend.
- **The user can always see what Taylor thinks it is doing:** the request, the step, what is done and
  what is left, on the screen throughout. At the end Taylor compares what was made with what was
  asked and says which parts were and were not done.
- To be measured before any claim: on a fixed set of ordinary requests, how often the whole job is
  done as asked, how often Taylor stops honestly, and how often it does the wrong thing believing it
  right. The last number is the one that must be near nothing. C3's first host trials (1 exact result
  in 5, text altered in others) are why this section exists.

### The screen stays alive (C3's conditions, adopted)

- The model's work is cut into bounded pieces, in loading, in reading the prompt and inside the layers,
  not only between words produced; between pieces the program reads keys, redraws and honours a
  request to stop. The kernel's Stop works even if it does not.
- Recorded, not estimated: the longest wait from a key to a picture and from a stop to stopped, while
  loading, reading and producing; and that memory is all returned after the kernel's Stop. Counts of
  work done are shown as counts, never as a percentage that was not measured.

What this buys: no trusted service to write and authenticate, no transport to secure, and a model that
is wrong or manipulated can only ever *ask*.

## What the present screen already models

`guest/src/lib.rs` has the intents (Submit, SetMode, Stop, Decide) and events (Started, Output,
ToolResult, PermissionNeeded/Resolved, StopAccepted, Stopped, Completed, Failed) with task and sequence
numbers and "effect unknown" handling. They stay; "the service" behind them becomes the model loop in
the same program plus the kernel's answers to proposals. `PermissionNeeded` becomes a preview only: the
real question is asked by the kernel.

## Open questions, in the order they block us

1. **What can a "built" thing be, beyond the first file?** PCP OS has no compiler on board. The first
   slice makes documents and data (above). For "build it" to mean more, the candidate is a small
   sandboxed script or bytecode that a PCP OS runtime draws with. The owner has been asked.
2. **Can a model run here at all, then usefully?** Measured by B3 at `070c9d5` on emulated UEFI: free
   memory when Tailorbird starts was about 138, 394 and 906 MiB on machines of 256, 512 and 1,024 MiB;
   the kernel uses one pool of memory below 1 GiB; boot modules must end below 128 MiB. The
   Qwen2.5-0.5B-Instruct file the host example uses is about 469 MiB. So on 1 GiB there is room for
   the raw weights, but **no way to load a file that size** (the 128 MiB window), and the full budget
   (the Coder weights, context cache, working memory, the display, and a reserve for Stop, consent and
   receipts) has not been measured, so whether memory above 1 GiB is needed is not yet known. At
   2,048 MiB the kernel failed a start-up check before any program ran (pcp-os pull request 342). A
   machine with no model or too little memory must still reach the desktop. Not measured: the laptop's
   memory map, speed, vector instructions.
3. **"Take over the system when allowed":** which effects beyond files (settings, starting programs,
   devices) may ever be proposed, and which never.
4. **No pointer.** Everything above is keyboard only until the kernel has one.
5. Where the model file lives and how it gets onto the media.

## Not claimed

No part of this exists. Nothing has run on hardware.
