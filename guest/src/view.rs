//! The picture Tailorbird draws from its state. It is in the library so that the same code that runs on
//! PCP OS can be drawn and checked on the machine that builds it.
use crate::frame::Frame;
use crate::palette::{stop_colors, AMBER, BAD, GOOD, GROUND, PANEL, SOFT, TEAL, TEXT};
use crate::{Model, Ui};

/// Words in the typeface's own proportions (a cell twice as tall as wide), at one of three sizes:
/// 1 for captions, 2 for what is read, 3 for a name.
fn label(f: &mut Frame<'_>, x: i32, y: i32, value: &str, size: i32, colour: u32) {
    let (w, h) = cell(size);
    if x < 0 || y < 0 {
        return;
    }
    for (i, byte) in value.bytes().enumerate() {
        f.glyph(x as usize + i * w, y as usize, w, h, byte, colour);
    }
}

fn cell(size: i32) -> (usize, usize) {
    match size {
        1 => (6, 12),
        2 => (10, 20),
        _ => (15, 30),
    }
}

/// A name the state gives in capitals ("NOT CONNECTED"), as a sentence ("Not connected").
fn sentence<'a>(value: &str, buf: &'a mut [u8; 40]) -> &'a str {
    let mut used = 0;
    for b in value.bytes().take(buf.len()) {
        buf[used] = if used == 0 { b.to_ascii_uppercase() } else { b.to_ascii_lowercase() };
        used += 1;
    }
    core::str::from_utf8(&buf[..used]).unwrap_or("")
}

/// Taylor, the tailorbird: green back, pale front, a long tail held up, and an amber crown worn like a
/// builder's hat. Sixteen by fourteen squares, each `unit` of the layout across. Facing left, toward
/// the words beside it.
// Taylor carries a little hammer (the owner's wish), held out in front of her: steel head forward,
// wooden handle in her wing.
const TAYLOR: [&[u8; 16]; 14] = [
    b"...............G",
    b"......AAAA....GG",
    b".....AAAAAA...GG",
    b"....AAAAAAAA.GG.",
    b".....GGKGGGGGGG.",
    b"...SSGGGGGGGGGG.",
    b"SS...GWWWGGGGG..",
    b"SSAAAGGWWWGGGG..",
    b"SS...WWWWWWGG...",
    b"......WWWWWG....",
    b".......WWWW.....",
    b"........S.S.....",
    b"........S.S.....",
    b".......SS.SS....",
];

fn taylor(f: &mut Frame<'_>, x: i32, y: i32, unit: i32) {
    for (row, line) in TAYLOR.iter().enumerate() {
        for (column, square) in line.iter().enumerate() {
            let colour = match square {
                b'G' => GOOD,
                b'A' => AMBER,
                b'W' => TEXT,
                b'K' => GROUND,
                b'S' => SOFT,
                _ => continue,
            };
            f.rect(x + column as i32 * unit, y + row as i32 * unit, unit, unit, colour);
        }
    }
}

/// A key's name on a small raised cap, and what it does after it. Returns where the next one starts.
fn key(f: &mut Frame<'_>, x: i32, y: i32, name: &str, does: &str) -> i32 {
    let cap = name.len() as i32 * 6 + 8;
    f.cap(x, y - 3, cap, 17, PANEL);
    label(f, x + 4, y, name, 1, TEXT);
    label(f, x + cap + 5, y, does, 1, SOFT);
    x + cap + 5 + does.len() as i32 * 6 + 14
}

// Full 64-bit decimal IDs: a service request must not be presented under a
// truncated 32-bit task/request number. The 5x7 labels can show 0..9 literally.
fn decimal_u64<'a>(mut value: u64, buf: &'a mut [u8; 20]) -> &'a str {
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 { break; }
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("")
}

pub fn paint_frame(f: &mut Frame<'_>, ui: &Ui) {
    let mut name = [0u8; 40];
    f.panel(0, 0, 960, 72, PANEL);
    label(f, 22, 12, "Tailorbird", 3, TEAL);
    // Where this is and where Esc leads: Programs, from which this was started (A3 pressed it and saw).
    label(f, 22, 52, "PCP OS > Programs > Tailorbird (Esc returns to Programs)", 1, SOFT);
    label(f, 202, 27, "builds and fixes on this computer", 2, SOFT);
    // What is behind it, said once, where the eye lands last: a lamp and two words.
    let talks = ui.talk.service == crate::talk::Service::Ready;
    // Notes are something Taylor can do only while PCP OS lets this launch propose files and the
    // launch has not given that up. Once given up, nothing on the screen may offer it again.
    let makes = ui.notes && !ui.gave_up;
    let (lamp, link) = if ui.connected { (GOOD, "Ready") } else if talks && makes { (GOOD, "Talk + notes") } else if talks { (GOOD, "Talk only") }
                       else if makes { (AMBER, "Notes only") } else if ui.gave_up { (AMBER, "No new files") } else { (BAD, "Not ready") };
    f.well(764, 18, 176, 36, GROUND);
    f.rect(778, 31, 10, 10, lamp);
    label(f, 796, 26, link, 2, lamp);

    // Mode and Stop are always visible even if the model is missing.
    f.panel(20, 94, 280, 352, PANEL);
    label(f, 38, 115, "TAYLOR IS", 1, TEAL);
    // With no task service, what stands here is how the model that talks stands: the two must not
    // say different things on one screen.
    let (standing, colour) = if ui.connected { (sentence(ui.model.label(), &mut name), if ui.model == Model::Ready { GOOD } else { AMBER }) } else {
        match ui.talk.service {
            crate::talk::Service::Absent => (sentence(ui.model.label(), &mut name), AMBER),
            crate::talk::Service::Loading => ("Getting ready", AMBER),
            crate::talk::Service::Ready => ("Ready to talk", GOOD),
            crate::talk::Service::Failed => ("Not started", BAD),
            crate::talk::Service::Stopped => ("Stopped", BAD),
        }
    };
    label(f, 38, 135, standing, 2, colour);
    // How far the service says its check of the model has got, in whole mebibytes, as it said it.
    // A check that is complete is still not a model that is ready: the word above says which.
    if let (false, Some((checked, of))) = (ui.connected, ui.talk.checked) {
        let mut digits = [0u8; 20];
        label(f, 38, 163, "Checked", 1, SOFT);
        let first = decimal_u64(checked >> 20, &mut digits);
        label(f, 38 + 8 * 6, 163, first, 1, TEXT);
        let x = 38 + (9 + first.len() as i32) * 6;
        label(f, x, 163, "of", 1, SOFT);
        let mut digits = [0u8; 20];
        let second = decimal_u64(of >> 20, &mut digits);
        label(f, x + 3 * 6, 163, second, 1, TEXT);
        label(f, x + (4 + second.len() as i32) * 6, 163, "MiB", 1, SOFT);
    }
    label(f, 38, 191, "MODE", 1, TEAL);
    let mode_rect = crate::pointer::MODE_RECT;
    f.well(mode_rect.x, mode_rect.y, mode_rect.width, mode_rect.height, GROUND);
    label(f, 48, 237, sentence(ui.mode.label(), &mut name), 2, TEXT);
    if ui.mode_pending.is_some() {
        label(f, 40, 287, "Change not confirmed", 1, AMBER);
    } else {
        key(f, 40, 289, "Alt", "+");
        key(f, 78, 289, "M", "changes the mode");
    }

    let stop_rect = crate::pointer::STOP_RECT;
    // Stop stops a task, or with no task the answer under way.
    let answering = !ui.controls().stop_enabled && ui.talk.answering();
    let (stop_fill, stop_heading, stop_caption) = stop_colors(ui.controls().stop_enabled || answering);
    if ui.controls().stop_enabled || answering {
        f.panel(stop_rect.x, stop_rect.y, stop_rect.width, stop_rect.height, stop_fill);
    } else {
        f.well(stop_rect.x, stop_rect.y, stop_rect.width, stop_rect.height, stop_fill);
    }
    label(f, 48, 329, "Stop", 3, stop_heading);
    label(f, 48, 365, if answering { "Taylor is answering" } else { sentence(ui.task.state.label(), &mut name) }, 1, stop_caption);
    key(f, 40, 410, "Alt", "+");
    key(f, 78, 410, "S", if answering { "stops the answer" } else { "stops the task" });

    f.panel(316, 94, 624, 352, PANEL);
    label(f, 335, 116, "Work", 2, TEAL);
    label(f, 337, 150, sentence(ui.task.state.label(), &mut name), 1, SOFT);
    f.rect(333, 173, 588, 2, GROUND);
    // These are only rows omitted by the eight-row local GUI window.
    // Service-sequence gaps and durable transcript readback remain PT-owned.
    if let Some((first_seq, last_seq)) = ui.retained_feed_bounds() {
        f.feed_window(ui.hidden_feed_rows(), first_seq, last_seq, AMBER);
    }
    // An old grant can remain stored strictly to correlate late service
    // receipts after StopAccepted. Never let that non-actionable metadata
    // repaint a live-looking permission inspector or hide Stop/ANOMALY rows.
    // The reducer's presentation gate is distinct from stored correlation.
    if ui.controls().permission_visible {
        // Consent details occupy a focused exact-text inspector instead of
        // fighting task-output rows for the same pixels. Stop and mode stay
        // visible in the separate x=20..300 sidebar.
    } else if ui.lines().is_empty() && ui.talk_shown && !ui.show_projects {
        transcript(f, &ui.talk);
    } else if ui.lines().is_empty() && ui.show_projects {
        match ui.browse_level {
            0 => projects(f, &ui.projects, ui.place, ui.active, ui.gave_up),
            1 => files(f, &ui.files, ui.file_place, ui.active, ui.gave_up),
            _ => viewed(f, &ui.viewed),
        }
    } else if ui.lines().is_empty() {
        // Nothing has happened: show what will, as three places a job passes through, none of them lit.
        for (n, (step, what)) in [("1", "You ask"), ("2", "You check"), ("3", "I build")].iter().enumerate() {
            let x = 339 + n as i32 * 196;
            f.well(x, 196, 180, 64, GROUND);
            label(f, x + 16, 212, step, 3, SOFT);
            label(f, x + 46, 218, what, 2, SOFT);
            if n < 2 {
                f.rect(x + 182, 227, 12, 2, SOFT);
            }
        }
        // Taylor says who it is once, says what is so in words anyone can read, and stops.
        taylor(f, 339, 286, 4);
        if let Some(job) = ui.jobs.last() {
            // What became of the last note, from the job the kernel's receipt was checked against: what
            // happened, where the file is (whole name, on as many lines as it needs), how long it lasts.
            let (words, good) = crate::note::headline(job);
            label(f, 420, 286, words, 2, if good { GOOD } else { AMBER });
            // Only a refusal the kernel's receipt states rules the file out. With no readable receipt the
            // file may or may not exist, and the screen says so and keeps the name asked for.
            let refused = !good && job.receipt().is_some();
            if refused {
                label(f, 420, 316, "Nothing was changed.", 2, SOFT);
            } else {
                let name = job.destination().as_str().as_bytes();
                for (row, part) in name.chunks(80).enumerate() {
                    label(f, 420, 316 + row as i32 * 16, core::str::from_utf8(part).unwrap_or("?"), 1, TEXT);
                }
                label(f, 420, 352, if good { "Kept until this computer is switched off." }
                      else { "Not confirmed: this file may or may not have been made." }, 1, AMBER);
            }
            label(f, 420, 384, if ui.gave_up { "Taylor makes no more files until started again." } else { "Type again and press Alt+N for another." }, 1, SOFT);
            let x = key(f, 339, 420, "Esc", "returns to Programs");
            if ui.notes {
                key(f, x, 420, "Alt+P", "shows your projects");
            }
            return footer(f, ui);
        }
        label(f, 420, 286, "Hi, I'm Taylor. Nothing is running.", 2, TEXT);
        if ui.connected {
            label(f, 420, 316, "Tell me in your own words what to", 2, SOFT);
            label(f, 420, 340, "build or fix. If I can, I will.", 2, SOFT);
        } else if ui.notes && ui.gave_up {
            // This launch gave up making files, on PCP OS's confirmation. Say what is so, and offer
            // only what still works: talking where a model is ready, and looking at what was made.
            if ui.talk.service == crate::talk::Service::Ready {
                label(f, 420, 316, "Type and press Enter to talk to", 2, SOFT);
                label(f, 420, 340, "me. I make no more files now.", 2, SOFT);
            } else {
                label(f, 420, 316, "You asked me to make no more", 2, SOFT);
                label(f, 420, 340, "files. PCP OS holds me to it.", 2, SOFT);
            }
            label(f, 420, 370, "Start Taylor again from Programs to allow notes again.", 1, SOFT);
            let x = key(f, 339, 420, "Esc", "returns to Programs");
            key(f, x, 420, "Alt+P", "shows your projects");
            return footer(f, ui);
        } else if ui.notes {
            if ui.talk.service == crate::talk::Service::Ready {
                // A model on this computer is ready: talking works, and so do notes.
                label(f, 420, 316, "Type and press Enter to talk to", 2, SOFT);
                label(f, 420, 340, "me. Alt+N saves it as a note.", 2, SOFT);
            } else {
                // No model, but PCP OS lets Taylor ask to make files: say the one thing that works today.
                label(f, 420, 316, "Today I can save what you type as", 2, SOFT);
                label(f, 420, 340, "a note. Type it, then press Alt+N.", 2, SOFT);
            }
            // Where a note will go, whole, before anything is asked.
            if let Some(active) = ui.active {
                label(f, 420, 370, "New notes go into the project", 1, SOFT);
                wrapped(f, 420, 384, active.as_str(), 84);
            } else if ui.projects.folder.is_empty() {
                // No folder is assumed: until PCP OS names one, a note has nowhere to go.
                label(f, 420, 370, "PCP OS has not yet said where projects go.", 1, AMBER);
            } else {
                // PCP OS has named the folder, and its name leaves no room for a project and a note.
                label(f, 420, 370, "Folder name too long to hold a note:", 1, AMBER);
                wrapped(f, 420, 384, ui.projects.folder.as_str(), 84);
            }
            let x = key(f, 339, 420, "Esc", "returns to Programs");
            key(f, x, 420, "Alt+P", "shows your projects");
            return footer(f, ui);
        } else {
            label(f, 420, 316, "I am not switched on yet, so I", 2, AMBER);
            label(f, 420, 340, "cannot build anything.", 2, AMBER);
        }
        label(f, 420, 384, "The rest of PCP OS works without me.", 2, GOOD);
        let x = key(f, 339, 420, "Esc", "returns to Programs");
        if ui.notes {
            key(f, x, 420, "Alt+P", "shows your projects");
        }
    } else {
        for (i, event) in ui.lines().iter().enumerate() {
            let y = 190 + i as i32 * 28;
            f.task_output(y, event.kind, event.message.as_str(), event.preview_lossy, AMBER, TEXT);
        }
    }
    if let Some(grant) = ui.permission.filter(|_| ui.controls().permission_visible) {
        // Permission review is a full-height 588px-wide inspector. The
        // canonical action occupies 48 native 10x20 cells; target occupies
        // at most 72 cells across two lines. Neither can be silently clipped.
        // Three native length digits preserve the distinction between
        // trailing-space paths.
        f.well(333, 182, 588, 254, GROUND);
        label(f, 340, 188, "Permission needed", 2, AMBER);
        label(f, 340, 209, "Review only: nothing has been granted", 1, SOFT);
        label(f, 340, 218, "ACTION", 1, TEAL);
        label(f, 845, 218, "A LEN", 1, TEAL);
        label(f, 340, 265, "TARGET", 1, TEAL);
        label(f, 845, 265, "T LEN", 1, TEAL);
        if crate::exact_ascii::permission_glyphs(grant.action.as_str(), grant.target.as_str(), |x, y, byte| f.glyph(x, y, 10, 20, byte, TEXT)).is_err() {
            // Not a license to approve a field that cannot be displayed.
            f.well(333, 182, 588, 254, GROUND);
            label(f, 340, 227, "Permission cannot be shown", 2, BAD);
            label(f, 340, 264, "Stop the task: the service must refuse", 1, BAD);
        } else {
            let decision_label = if ui.controls().decision_enabled {
                "Alt+A allows    Alt+D denies"
            } else if ui.decision_pending.is_some() {
                "Decision sent: waiting for the service"
            } else {
                "Input locked: no decision"
            };
            label(f, 340, 338, decision_label, 2, AMBER);
            label(f, 340, 372, "Source digest and effect binding are the service's duties", 1, SOFT);
            label(f, 340, 392, "This screen writes nothing and authorizes nothing", 1, SOFT);
            label(f, 340, 414, "TASK", 1, TEAL);
            let mut task_digits = [0u8; 20];
            label(f, 380, 414, decimal_u64(ui.task.id, &mut task_digits), 1, TEXT);
            label(f, 530, 414, "REQUEST", 1, TEAL);
            let mut req_digits = [0u8; 20];
            label(f, 590, 414, decimal_u64(grant.request, &mut req_digits), 1, TEXT);
        }
    }

    footer(f, ui);
}

/// Which of the items the rows are: "Showing 7 to 12 of 20 projects. Up, Down, Page Up, Page Down move."
/// Said only when there are more than the rows show, so every item is known to be reachable.
fn range(f: &mut Frame<'_>, top: usize, shown: usize, total: usize, what: &str) {
    if total <= shown || shown == 0 {
        return;
    }
    let mut digits = [0u8; 20];
    let mut x = 339;
    for (words, number) in [("Showing", top + 1), ("to", top + shown), ("of", total)] {
        label(f, x, 392, words, 1, AMBER);
        x += words.len() as i32 * 6 + 6;
        let n = decimal_u64(number as u64, &mut digits);
        label(f, x, 392, n, 1, AMBER);
        x += n.len() as i32 * 6 + 6;
    }
    label(f, x, 392, what, 1, AMBER);
    label(f, x + what.len() as i32 * 6 + 6, 392, "(Page Up/Down)", 1, SOFT);
}

/// How a reply that kept some text ended, if it did not simply complete.
fn ended(turn: &crate::talk::Turn) -> Option<&'static str> {
    match turn.state {
        _ if turn.text.as_str().is_empty() => None,
        crate::talk::Reply::Cancelled => Some("(stopped)"),
        crate::talk::Reply::Failed => Some("(failed)"),
        _ => None,
    }
}

/// Whether those words go on a row of their own under the reply: when its last row is full, or has
/// not room after its last character for a gap and the words within the width of the text.
fn ended_below(turn: &crate::talk::Turn) -> bool {
    const ACROSS: usize = 84;
    ended(turn).is_some_and(|how| {
        let last = turn.text.as_str().len() % ACROSS;
        last == 0 || last + 2 + how.len() > ACROSS
    })
}

/// The conversation, in the Work panel: the latest turns that fit, each with who really said it. Taylor's
/// name stands only on turns a model service sent; what this program says about the state of things is
/// marked as PCP OS's. Nothing here is a task: no stages, no Stop.
fn transcript(f: &mut Frame<'_>, talk: &crate::talk::Talk) {
    use crate::talk::{Reply, Who};
    const ACROSS: usize = 84;
    const ROWS: i32 = 16;
    let mut digits = [0u8; 20];
    label(f, 339, 190, "Talking with Taylor", 2, TEXT);
    // Whether anything can answer, said where the conversation is, in the same words every time.
    let (model, colour) = match talk.service {
        crate::talk::Service::Absent => ("Model: none", SOFT),
        crate::talk::Service::Loading => ("Model: loading", AMBER),
        crate::talk::Service::Ready => ("Model: ready", GOOD),
        crate::talk::Service::Failed => ("Model: not started", BAD),
        crate::talk::Service::Stopped => ("Model: stopped", BAD),
    };
    label(f, 790, 196, model, 1, colour);
    // Rows each turn needs: its text wrapped, at least one, and a gap after it.
    let rows = |t: &crate::talk::Turn| (t.text.as_str().len().div_ceil(ACROSS).max(1) + t.cut as usize + ended_below(t) as usize) as i32 + 1;
    let turns = talk.turns();
    let mut first = turns.len();
    let mut used = 0;
    while first > 0 && used + rows(&turns[first - 1]) <= ROWS {
        used += rows(&turns[first - 1]);
        first -= 1;
    }
    let above = talk.dropped + first;
    if above > 0 {
        label(f, 600, 196, "earlier turns not shown:", 1, AMBER);
        label(f, 600 + 25 * 6, 196, decimal_u64(above as u64, &mut digits), 1, AMBER);
    }
    let mut y = 218;
    for turn in &turns[first..] {
        let (name, colour) = match turn.who {
            Who::Person => ("You", TEAL),
            Who::Taylor => ("Taylor", GOOD),
            Who::System => ("PCP OS", AMBER),
        };
        label(f, 339, y, name, 1, colour);
        let text = turn.text.as_str().as_bytes();
        for part in text.chunks(ACROSS) {
            label(f, 393, y, core::str::from_utf8(part).unwrap_or("?"), 1, if turn.who == Who::System { SOFT } else { TEXT });
            y += 12;
        }
        if text.is_empty() && turn.state == Reply::Streaming && talk.reading.is_some() {
            // What the service said of its reading, and nothing worked out from it.
            let (read, of) = talk.reading.unwrap_or((0, 1));
            let mut line = crate::Text::<84>::new();
            // "Preparing the reply", not "reading your message": the count is of everything the model
            // reads before it can answer, which includes the conversation so far (C3).
            for part in ["(preparing the reply: ", decimal_u64(read, &mut digits), " of "] {
                for b in part.bytes() { line.push_ascii(b); }
            }
            let mut more = [0u8; 20];
            for b in decimal_u64(of, &mut more).bytes().chain(b" tokens read)".iter().copied()) { line.push_ascii(b); }
            label(f, 393, y, line.as_str(), 1, SOFT);
            y += 12;
        } else if text.is_empty() {
            let waiting = match turn.state {
                Reply::Streaming => "(answering...)",
                Reply::Cancelled => "(stopped before anything was said)",
                Reply::Failed => "(no answer: it failed)",
                _ => "",
            };
            label(f, 393, y, waiting, 1, SOFT);
            y += 12;
        } else if let Some(how) = ended(turn) {
            // After the last character where the row has room for it; otherwise on a row of its own.
            // It is never drawn over the reply: a reply that fills its last row exactly used to have
            // it painted across that row's beginning (Director 2 counted the pixels lost).
            if ended_below(turn) {
                label(f, 393, y, how, 1, AMBER);
                y += 12;
            } else {
                label(f, 393 + (text.len() % ACROSS) as i32 * 6 + 8, y - 12, how, 1, AMBER);
            }
        }
        if turn.cut {
            label(f, 393, y, "(the rest is not kept)", 1, AMBER);
            y += 12;
        }
        y += 12;
    }
    let x = key(f, 339, 420, "Enter", "sends");
    let x = key(f, x, 420, "Alt+C", "new conversation");
    let x = key(f, x, 420, "Alt+P", "projects");
    key(f, x, 420, "Esc", "returns to Programs");
}

/// A name whole, on as many rows of `across` characters as it needs (12 apart).
fn wrapped(f: &mut Frame<'_>, x: i32, y: i32, value: &str, across: usize) {
    for (row, part) in value.as_bytes().chunks(across).enumerate() {
        label(f, x, y + row as i32 * 12, core::str::from_utf8(part).unwrap_or("?"), 1, TEXT);
    }
}

/// The project list, in the Work panel: where the projects are, each with how many files, and in words
/// what is not known. "No projects yet" is said only of a whole list that was read and held nothing.
fn projects(f: &mut Frame<'_>, list: &crate::projects::Projects, place: usize, active: Option<crate::project_location::ProjectLocation>, gave_up: bool) {
    use crate::projects::Listing;
    let mut digits = [0u8; 20];
    label(f, 339, 190, "Your projects", 2, TEXT);
    // Where new notes would go is said only while new notes can be made (Director B3 read the
    // legend and the marker off the screen beside "No new files").
    if active.is_some() && !gave_up {
        label(f, 600, 196, "\"notes\" marks where new notes go", 1, SOFT);
    }
    match list.state {
        Listing::NotAsked => label(f, 339, 226, "Not read yet.", 2, SOFT),
        Listing::Unavailable => {
            label(f, 339, 226, "PCP OS has not let Taylor see a", 2, AMBER);
            label(f, 339, 250, "projects folder in this launch.", 2, AMBER);
        }
        Listing::Unreadable => {
            // Why is not known here (no room, a fault, bytes that do not check): no cause is named.
            label(f, 339, 226, "The projects folder could not be", 2, AMBER);
            label(f, 339, 250, "read, so nothing is shown.", 2, AMBER);
            label(f, 339, 280, "Alt+P twice tries again.", 1, SOFT);
            if !list.folder.is_empty() {
                label(f, 339, 304, "It was last read as the folder", 1, SOFT);
                wrapped(f, 339, 318, list.folder.as_str(), 94);
            }
        }
        Listing::Listed => {
            label(f, 339, 214, "in the folder", 1, SOFT);
            wrapped(f, 339 + 14 * 6, 214, list.folder.as_str(), 80);
            if list.known_empty() {
                label(f, 339, 256, "No projects yet.", 2, SOFT);
                label(f, 339, 286, if gave_up { "Taylor makes no more files until started again." } else { "Type something and press Alt+N to make a note." }, 1, SOFT);
            }
            for (row, entry) in list.rows[..list.shown].iter().enumerate() {
                let y = 244 + row as i32 * 20;
                f.well(339, y - 3, 570, 18, GROUND);
                if row == place {
                    f.rect(339, y - 3, 4, 18, TEAL);
                }
                let name = entry.project.as_str();
                // A name too long for the row is shown cut, and marked as cut.
                let (fits, cut) = if name.len() > 72 { (&name[..69], true) } else { (name, false) };
                label(f, 347, y, fits, 1, TEXT);
                if cut {
                    label(f, 347 + 69 * 6, y, "...", 1, AMBER);
                }
                // Its own column, right of the count, clear of the longest name a row shows.
                if active.is_some() && entry.location == active && !gave_up {
                    label(f, 872, y, "notes", 1, GOOD);
                }
                label(f, 800, y, decimal_u64(entry.files as u64, &mut digits), 1, TEXT);
                label(f, 830, y, if entry.files == 1 { "file" } else { "files" }, 1, SOFT);
            }
            // Fixed places below the rows, clear of the keys: the chosen project's whole location (a cut
            // row cannot tell two long names apart), then what is not shown, then whether the list is whole.
            if let Some(location) = list.rows[..list.shown].get(place).and_then(|r| r.location) {
                label(f, 339, 366, "Chosen:", 1, SOFT);
                wrapped(f, 339 + 8 * 6, 366, location.as_str(), 86);
            }
            range(f, list.top, list.shown, list.projects, "projects");
            if list.other > 0 {
                label(f, 339 + 50 * 6, 392, "other files in the folder:", 1, SOFT);
                label(f, 339 + 77 * 6, 392, decimal_u64(list.other as u64, &mut digits), 1, SOFT);
            }
            if !list.complete {
                label(f, 339, 404, "PCP OS listed only part of the folder. There is more.", 1, AMBER);
            }
        }
    }
    let mut x = key(f, 339, 420, "Alt+P", "goes back");
    if list.shown > 0 {
        x = key(f, x, 420, "Up", "/");
        x = key(f, x, 420, "Down", "choose");
        x = key(f, x, 420, "Enter", "opens");
    }
    key(f, x, 420, "Esc", "returns to Programs");
}

/// One project's files. The size is what the list said when it was read.
fn files(f: &mut Frame<'_>, list: &crate::projects::Files, place: usize, active: Option<crate::project_location::ProjectLocation>, gave_up: bool) {
    use crate::projects::Listing;
    let mut digits = [0u8; 20];
    label(f, 339, 190, "Project", 2, TEXT);
    match list.state {
        Listing::Listed => {
            wrapped(f, 339, 214, list.project.as_str(), 94);
            if active.is_some() && list.location == active {
                label(f, 440, 196, if gave_up { "Taylor makes no more files until started again." } else { "New notes (Alt+N) go into this project." }, 1, if gave_up { SOFT } else { GOOD });
            }
            if list.count == 0 {
                label(f, 339, 256, if list.complete { "This project has no files now." } else { "No files of it are in the part PCP OS listed." }, 1, SOFT);
            }
            for (row, entry) in list.rows[..list.shown].iter().enumerate() {
                let y = 244 + row as i32 * 20;
                f.well(339, y - 3, 570, 18, GROUND);
                if row == place {
                    f.rect(339, y - 3, 4, 18, TEAL);
                }
                let name = entry.name.as_str();
                let (fits, cut) = if name.len() > 72 { (&name[..69], true) } else { (name, false) };
                label(f, 347, y, fits, 1, TEXT);
                if cut {
                    label(f, 347 + 69 * 6, y, "...", 1, AMBER);
                }
                label(f, 800, y, decimal_u64(entry.size as u64, &mut digits), 1, TEXT);
                // u32 sizes need up to ten 6-pixel digits (x=800..860). Keep a full cell gap.
                label(f, 866, y, "bytes", 1, SOFT);
            }
            if let Some(destination) = list.rows[..list.shown].get(place).and_then(|r| r.destination) {
                label(f, 339, 366, "Chosen:", 1, SOFT);
                wrapped(f, 339 + 8 * 6, 366, destination.as_str(), 86);
            }
            range(f, list.top, list.shown, list.count, "files");
            if !list.complete {
                label(f, 339, 404, "PCP OS listed only part of the folder. There is more.", 1, AMBER);
            }
        }
        _ => {
            label(f, 339, 226, "The list could not be read, so", 2, AMBER);
            label(f, 339, 250, "nothing is shown.", 2, AMBER);
        }
    }
    let mut x = key(f, 339, 420, "Left", "goes back");
    if list.shown > 0 {
        x = key(f, x, 420, "Up", "/");
        x = key(f, x, 420, "Down", "choose");
        x = key(f, x, 420, "Enter", "shows the file");
    }
    key(f, x, 420, "Alt+P", "closes");
}

/// One file, read whole, shown and not changeable here.
fn viewed(f: &mut Frame<'_>, file: &crate::projects::Viewed) {
    use crate::projects::{Checked, Opened};
    let mut digits = [0u8; 20];
    if let Some(destination) = &file.destination {
        for (row, part) in destination.as_str().as_bytes().chunks(96).enumerate() {
            label(f, 339, 184 + row as i32 * 13, core::str::from_utf8(part).unwrap_or("?"), 1, TEXT);
        }
    }
    match file.state {
        Opened::NotAsked | Opened::Unavailable => label(f, 339, 250, "This file could not be read. Nothing is known about it.", 1, AMBER),
        Opened::TooLarge => label(f, 339, 250, "This file is larger than a text file Taylor shows.", 1, AMBER),
        Opened::NotText => label(f, 339, 250, "This file is not plain text, so Taylor does not show it.", 1, AMBER),
        Opened::Shown => {
            f.well(335, 226, 590, 14 * crate::projects::VIEW_LINES as i32 + 8, GROUND);
            if file.bytes == 0 {
                label(f, 343, 232, "(the file is empty)", 1, SOFT);
            }
            for (row, line) in file.lines[..file.shown].iter().enumerate() {
                label(f, 343, 231 + row as i32 * 14, line.as_str(), 1, TEXT);
            }
        }
    }
    if matches!(file.state, Opened::Shown | Opened::NotText) {
        let y = 226 + 14 * crate::projects::VIEW_LINES as i32 + 14;
        let x = 339 + 6 * (decimal_u64(file.bytes as u64, &mut digits).len() as i32 + 1);
        label(f, 339, y, decimal_u64(file.bytes as u64, &mut digits), 1, TEXT);
        label(f, x, y, if file.more { "characters. Only the first lines are shown." } else { "characters. Read only." }, 1, if file.more { AMBER } else { SOFT });
        let (words, colour) = match file.checked {
            Checked::Matches => ("Checked: exactly what PCP OS made when you allowed it.", GOOD),
            Checked::Differs => ("Warning: this is NOT what PCP OS made when you allowed it.", BAD),
            Checked::NoReceipt => ("No kept receipt for this file, so it is not checked.", SOFT),
        };
        label(f, 339, y + 14, words, 1, colour);
    }
    let x = key(f, 339, 420, "Left", "goes back");
    key(f, x, 420, "Alt+P", "closes");
}

fn footer(f: &mut Frame<'_>, ui: &Ui) {
    f.panel(20, 458, 920, 48, PANEL);
    label(f, 32, 472, "ASK", 1, TEAL);
    // Trailing ASCII spaces have blank font cells. Display the exact byte
    // count alongside the editor so foo and foo-space are distinguishable.
    // These count glyphs are in the x<140 margin, outside the text raster.
    label(f, 32, 489, "USED", 1, TEAL);
    let mut count_buf = [0u8; 20];
    label(f, 70, 489, decimal_u64(ui.input.as_str().len() as u64, &mut count_buf), 1, TEXT);
    label(f, 102, 489, "/120", 1, SOFT);
    // The text submitted to the service is the *exact* text painted here.
    // Two lines at 78 cells expose all 120 stored bytes, including
    // case-sensitive paths and source punctuation. A drawing failure is
    // visible, and the reducer independently refuses submission when the
    // editor cannot represent the request.
    // The field is one flat colour under every letter, so that what is typed can be checked pixel for
    // pixel against the typeface; its edge is drawn outside the letters' cells.
    f.rect(136, 459, 788, 44, GROUND);
    f.rect(138, 460, 784, 42, PANEL);
    if crate::exact_ascii::request_glyphs(ui.input.as_str(), |x, y, byte| f.glyph(x, y, 10, 20, byte, TEXT)).is_err() {
        f.rect(138, 460, 784, 42, GROUND);
        label(f, 146, 470, "Input cannot be shown", 1, BAD);
    }
    let mut x = 22;
    for (name, does) in [("Enter", "sends"), ("Alt+R", "retries"), ("Esc", "leaves")] {
        x = key(f, x, 516, name, does);
    }
    key(f, x, 516, "F6", if ui.compact_view { "compact" } else { "fit" });
    // The notice keeps the place and the size the emulator checks read it at.
    f.text(535, 513, ui.notice.as_str(), 1, AMBER);
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::{format, vec, vec::Vec};

    fn shot(ui: &Ui) -> Vec<u32> {
        let mut pixels = vec![0u32; 960 * 540];
        let mut f = Frame::new(&mut pixels, 960, 540, 960, GROUND, false);
        paint_frame(&mut f, ui);
        pixels
    }

    /// The queued notice is a real raster assertion, not just a string in a reducer test.
    /// It may not claim model delivery before any channel write or service acknowledgment.
    #[test]
    fn queued_talk_notice_renders_exactly_and_not_as_sent() {
        let mut ui = Ui::new();
        ui.talk.service_is(crate::talk::Service::Ready);
        for b in b"hello" { assert!(ui.input.push_ascii(*b)); }
        let _ = ui.intent(crate::Action::Submit);
        let queued = "QUEUED LOCALLY - DELIVERY UNCONFIRMED";
        assert_eq!(ui.notice.as_str(), queued);
        assert!(ui.for_service().is_some(), "the frame has not been carried");
        let shown = shot(&ui);

        // Keep everything else in the frame identical and draw only the notice from the
        // actual source font at its production coordinates.
        ui.notice = crate::Text::new();
        let blank = shot(&ui);
        let mut exact = blank.clone();
        let mut frame = Frame::new(&mut exact, 960, 540, 960, GROUND, false);
        frame.text(535, 513, queued, 1, AMBER);
        assert_ne!(blank, exact, "the queued label must have visible pixels");
        assert_eq!(shown, exact, "the native bitmap must show the entire exact queued notice");

        let mut misleading = blank.clone();
        let mut frame = Frame::new(&mut misleading, 960, 540, 960, GROUND, false);
        frame.text(535, 513, "SENT TO TAYLOR'S MODEL ON THIS COMPUTER", 1, AMBER);
        assert_ne!(shown, misleading, "an unconfirmed queued message must not look delivered");
    }

    /// A reply that was stopped or failed says so without one pixel of the reply being painted over,
    /// whatever its length: after its last character where there is room, on a row of its own where
    /// there is not. Director 2 found the words drawn across the start of a reply that filled its
    /// last row exactly (84, 168, ... characters).
    /// While the service reads the message, the reply's place says how far it has got, in the
    /// service's own two numbers, and nothing else on the screen changes.
    #[test]
    fn reading_progress_is_drawn_where_the_answer_will_be_and_nowhere_else() {
        use crate::talk::{Incoming, Service};
        let mut ui = Ui::new();
        ui.talk.service_is(Service::Ready);
        for b in b"go" { assert!(ui.input.push_ascii(*b)); }
        let _ = ui.intent(crate::Action::Submit);
        let waiting = shot(&ui);
        assert_eq!(ui.talk.receive(Incoming::Reading { conversation: 1, reply: 1, read: 37, of: 212 }), crate::talk::Taken::Shown);
        let reading = shot(&ui);
        // Drawn independently: the row emptied to the colour of one of its blank cells, then the words.
        let width = 960;
        let mut expected = waiting.clone();
        let blank = waiting[242 * width + 393 + 6 * 80];
        for y in 242..254 {
            for x in 393..393 + 6 * 84 {
                expected[y * width + x] = blank;
            }
        }
        // (A new frame starts from its ground everywhere, so the words are drawn on one of their
        // own and that row copied over.)
        let mut alone = vec![0u32; expected.len()];
        label(&mut Frame::new(&mut alone, width, expected.len() / width, width, blank, false), 393, 242, "(preparing the reply: 37 of 212 tokens read)", 1, SOFT);
        for y in 242..254 {
            for x in 393..393 + 6 * 84 {
                expected[y * width + x] = alone[y * width + x];
            }
        }
        let differ: std::vec::Vec<(usize, usize)> = (0..expected.len()).filter(|i| reading[*i] != expected[*i]).map(|i| (i % width, i / width)).collect();
        assert!(differ.is_empty(), "the screen while reading is not the waiting screen with that one row changed: {} pixels, x {:?}..{:?}, y {:?}..{:?}", differ.len(),
                differ.iter().map(|d| d.0).min(), differ.iter().map(|d| d.0).max(), differ.iter().map(|d| d.1).min(), differ.iter().map(|d| d.1).max());
        assert!(reading != waiting);
        // Another count is another picture; the first of the answer removes the line.
        ui.talk.receive(Incoming::Reading { conversation: 1, reply: 1, read: 38, of: 212 });
        assert!(shot(&ui) != reading);
        ui.talk.receive(Incoming::Chunk { conversation: 1, reply: 1, text: "Hi" });
        let answered = shot(&ui);
        ui.talk.reading = Some((38, 212));
        assert!(shot(&ui) == answered, "reading is never drawn once the answer has begun");
    }

    #[test]
    fn how_a_reply_ended_is_never_drawn_over_the_reply() {
        use crate::talk::{Incoming, Service};
        let with = |length: usize, end: u8| {
            let mut ui = Ui::new();
            ui.talk.service_is(Service::Ready);
            for b in b"go" { assert!(ui.input.push_ascii(*b)); }
            let _ = ui.intent(crate::Action::Submit);
            let words: std::string::String = (0..length).map(|i| (b'A' + (i % 26) as u8) as char).collect();
            ui.talk.receive(Incoming::Chunk { conversation: 1, reply: 1, text: &words });
            match end {
                0 => { ui.talk.receive(Incoming::Done { conversation: 1, reply: 1 }); }
                1 => { let _ = ui.talk.cancel(); }
                _ => { ui.talk.receive(Incoming::Failed { conversation: 1, reply: 1 }); }
            }
            shot(&ui)
        };
        // The person's turn is one row at y 218 and a gap; the reply's text begins at y 242, x 393.
        for length in [1usize, 60, 72, 73, 74, 75, 83, 84, 85, 167, 168, 169, 251, 252, 253, 336] {
            let complete = with(length, 0);
            for (end, words) in [(1u8, "(stopped)"), (2, "(failed)")] {
                let ended = with(length, end);
                let rows = length.div_ceil(84);
                let last = if length % 84 == 0 { 84 } else { length % 84 };
                // No pixel of the reply's own cells differs from the same reply drawn complete.
                for row in 0..rows {
                    let across = if row + 1 == rows { last } else { 84 };
                    for y in 242 + 12 * row..242 + 12 * row + 12 {
                        for x in 393..393 + 6 * across {
                            assert_eq!(ended[y * 960 + x], complete[y * 960 + x], "{length} characters {words}: the reply's pixel at {x},{y} was painted over");
                        }
                    }
                }
                // And the words are there: drawn alone at the place they belong, they are what differs.
                let below = length % 84 == 0 || length % 84 + 2 + words.len() > 84;
                let (x, y) = if below { (393, 242 + 12 * rows as i32) } else { (393 + 6 * last as i32 + 8, 242 + 12 * (rows as i32 - 1)) };
                let mut alone = vec![PANEL; 960 * 540];
                label(&mut Frame::new(&mut alone, 960, 540, 960, PANEL, false), x, y, words, 1, AMBER);
                let mut inked = 0;
                for at in 0..960 * 540 {
                    if alone[at] != PANEL {
                        inked += 1;
                        assert_eq!(ended[at], alone[at], "{length} characters {words}: not drawn whole at {x},{y}");
                    }
                }
                assert!(inked > 50, "{words} drew nothing");
                // The words stay within the width of the text.
                assert!(x as usize + 6 * words.len() <= 393 + 6 * 84 + 8, "{length} characters {words}: runs past the text");
            }
        }
    }

    /// On the project list, where new notes go is marked only while new notes can be made: after
    /// giving up, the list with a project chosen is drawn exactly as the list with none chosen.
    #[test]
    fn after_giving_up_the_project_list_marks_nowhere_for_new_notes() {
        let location = crate::note::default_project(crate::project_location::BuildFolder::default()).unwrap();
        let listed = |active: bool, gave_up: bool, rows: usize| {
            let mut ui = Ui::new();
            ui.notes = true;
            ui.gave_up = gave_up;
            ui.show_projects = true;
            ui.projects.state = crate::projects::Listing::Listed;
            ui.projects.complete = true;
            ui.projects.folder = crate::Text::display("taylor");
            ui.projects.projects = rows;
            ui.projects.shown = rows;
            if rows == 1 {
                ui.projects.rows[0] = crate::projects::Row { project: crate::Text::display(location.as_str()), location: Some(location), files: 2 };
            }
            ui.active = active.then_some(location);
            shot(&ui)
        };
        for rows in [0, 1] {
            // Before giving up, a chosen project is marked (the legend, and on its row the word).
            assert_ne!(listed(true, false, rows), listed(false, false, rows), "{rows} row(s)");
            // After, nothing marks it: the same picture as with no project chosen.
            assert_eq!(listed(true, true, rows), listed(false, true, rows), "{rows} row(s)");
        }
    }

    /// Once this launch has given up making files, no screen offers Alt+N or says notes can be saved:
    /// the greeting with and without a model, and the panel after an earlier note.
    #[test]
    fn after_giving_up_nothing_on_the_screen_offers_a_note() {
        let row = |pixels: &Vec<u32>, y: usize, words: &str, size: i32| {
            let mut alone = vec![PANEL; 960 * 540];
            let mut f = Frame::new(&mut alone, 960, 540, 960, PANEL, false);
            label(&mut f, 420, y as i32, words, size, SOFT);
            (y..y + 20).all(|line| (420..900).all(|x| alone[line * 960 + x] == pixels[line * 960 + x]))
        };
        let mut ui = Ui::new();
        ui.notes = true;
        let before = shot(&ui);
        assert!(row(&before, 340, "a note. Type it, then press Alt+N.", 2));
        ui.gave_up = true;
        let after = shot(&ui);
        assert!(!row(&after, 340, "a note. Type it, then press Alt+N.", 2));
        assert!(row(&after, 316, "You asked me to make no more", 2) && row(&after, 340, "files. PCP OS holds me to it.", 2));
        // With a model ready, talking is still offered and notes are not.
        ui.talk.service = crate::talk::Service::Ready;
        let talking = shot(&ui);
        assert!(!row(&talking, 340, "me. Alt+N saves it as a note.", 2) && row(&talking, 340, "me. I make no more files now.", 2));
        // And after a note made earlier in the launch, the panel no longer invites another.
        let mut ui = Ui::new();
        ui.notes = true;
        let p = crate::note::prepare(3, 1, "hello").unwrap();
        assert!(ui.jobs.keep(p.job).is_ok());
        let invited = shot(&ui);
        ui.gave_up = true;
        assert_ne!(invited, shot(&ui));
        // Where PCP OS never allowed notes, giving up cannot have happened and changes nothing drawn.
        let mut never = Ui::new();
        let plain = shot(&never);
        never.gave_up = true;
        assert_ne!(plain, shot(&never)); // only the lamp's words differ: "No new files" for "Not ready"
    }

    /// Without a model the screen says what Taylor can do today only when PCP OS lets it, and after a
    /// note it shows that note's result in the Work panel, not the greeting.
    #[test]
    fn the_work_panel_follows_what_taylor_can_do_and_did() {
        let plain = shot(&Ui::new());
        let mut ui = Ui::new();
        ui.notes = true;
        let able = shot(&ui);
        assert_ne!(plain, able);
        let mut p = crate::note::prepare(3, 1, "hello").unwrap();
        assert!(ui.jobs.keep(p.job).is_ok());
        let unknown = shot(&ui);
        assert_ne!(able, unknown);
        // A job with no receipt never draws the words that rule the file out; a stated refusal does.
        let says = |pixels: &Vec<u32>, words: &str, size: i32, colour: u32| {
            // The row the words stand on, against the same words drawn alone on the panel's colour.
            let mut alone = vec![PANEL; 960 * 540];
            let mut f = Frame::new(&mut alone, 960, 540, 960, PANEL, false);
            label(&mut f, 420, 316, words, size, colour);
            (316..336).all(|y| (420..900).all(|x| alone[y * 960 + x] == pixels[y * 960 + x]))
        };
        assert!(!says(&unknown, "Nothing was changed.", 2, SOFT));
        let mut declined = crate::note::prepare(3, 1, "hello").unwrap();
        declined.job.submitted().unwrap();
        let mut no = [0u8; 100];
        no[0..4].copy_from_slice(&1u32.to_le_bytes());
        no[4..8].copy_from_slice(&2u32.to_le_bytes());
        no[8..16].copy_from_slice(&9u64.to_le_bytes());
        no[16..24].copy_from_slice(&3u64.to_le_bytes());
        no[24..32].copy_from_slice(&1u64.to_le_bytes());
        no[32..64].copy_from_slice(&crate::kernel_proposal::request_digest("taylor/my-first-project/note-1.txt", b"hello\n"));
        no[64..96].copy_from_slice(&crate::sha256::sha256(&[b"hello\n"]));
        assert_eq!(crate::note::settle(&mut declined.job, &no).as_str(), "Not made: you said no.");
        let mut refused = Ui::new();
        refused.notes = true;
        assert!(refused.jobs.keep(declined.job).is_ok());
        assert!(says(&shot(&refused), "Nothing was changed.", 2, SOFT));
        p = crate::note::prepare(3, 1, "hello").unwrap();
        p.job.submitted().unwrap();
        let (name, content) = ("taylor/my-first-project/note-1.txt", b"hello\n");
        let mut r = [0u8; 100];
        r[0..4].copy_from_slice(&1u32.to_le_bytes());
        r[4..8].copy_from_slice(&1u32.to_le_bytes());
        r[8..16].copy_from_slice(&9u64.to_le_bytes());
        r[16..24].copy_from_slice(&3u64.to_le_bytes());
        r[24..32].copy_from_slice(&1u64.to_le_bytes());
        r[32..64].copy_from_slice(&crate::kernel_proposal::request_digest(name, content));
        r[64..96].copy_from_slice(&crate::sha256::sha256(&[content]));
        crate::note::settle(&mut p.job, &r);
        assert_eq!(crate::note::headline(&p.job), ("I made your note.", true));
        assert!(ui.jobs.keep(p.job).is_ok());
        let made = shot(&ui);
        assert_ne!(unknown, made);
        // Everything outside the Work panel's lower part is the same in all four.
        for (i, (a, b)) in plain.iter().zip(made.iter()).enumerate() {
            let (x, y) = (i % 960, i / 960);
            assert!(a == b || (x >= 316 && x < 940 && y >= 280 && y < 446) || (x >= 764 && y >= 18 && y < 54), "differs at {x},{y}");
        }
    }

    /// The fullest list there can be (longest folder, long names, more projects than rows, other files,
    /// an incomplete list) draws nothing outside the Work panel, and nothing on the row of keys but the keys.
    #[test]
    fn the_fullest_project_list_stays_in_its_panel() {
        use crate::projects::Projects;
        let folder = std::format!("{}/{}", "a".repeat(57), "b".repeat(58));
        let mut bytes: Vec<u8> = Vec::new();
        let names: Vec<std::string::String> = (0..9).map(|i| std::format!("{}/{}/f", folder, i)).chain([std::format!("{}/zz", folder)]).collect();
        for word in [1u32, names.len() as u32, 0, folder.len() as u32] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes.extend_from_slice(&7u64.to_le_bytes());
        bytes.extend_from_slice(folder.as_bytes());
        for name in &names {
            bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
            bytes.extend_from_slice(&[0, 0, 1, 0, 0, 0]);
            bytes.extend_from_slice(name.as_bytes());
        }
        let list = Projects::read(&bytes, None);
        assert_eq!((list.projects, list.other, list.complete, list.folder.as_str().len()), (9, 1, false, 116));
        let mut full = Ui::new();
        full.notes = true;
        full.show_projects = true;
        full.projects = list;
        let mut empty = Ui::new();
        empty.notes = true;
        empty.show_projects = true;
        empty.projects = Projects::unavailable();
        let (a, b) = (shot(&full), shot(&empty));
        let mut lowest = 0;
        for (i, (p, q)) in a.iter().zip(b.iter()).enumerate() {
            let (x, y) = (i % 960, i / 960);
            if p != q {
                assert!(x >= 333 && x < 925 && y >= 176 && y < 446, "outside the panel at {x},{y}");
                if y < 416 { lowest = lowest.max(y); }
            }
        }
        assert!(lowest >= 404 && lowest < 416, "the last line of words ends at {lowest}");
    }

    /// The complete listed u32 size and the unit word occupy disjoint glyph cells.
    /// Independently draw both on the same sunken-row surface, then compare every pixel.
    /// The old x=842 unit location must fail this oracle for long sizes.
    #[test]
    fn project_file_size_and_units_do_not_overpaint_at_u32_boundaries() {
        use crate::projects::{Files, Listing};
        for (width, height) in [(960usize, 540usize), (1920, 1080)] {
            let scale = width / 960;
            for size in [0u32, 9_999_999, 10_000_000, u32::MAX] {
                let mut list = Files::new();
                list.state = Listing::Listed;
                list.complete = true;
                list.count = 1;
                list.shown = 1;
                list.rows[0].size = size;

                let mut actual = vec![PANEL; width * height];
                files(&mut Frame::new(&mut actual, width, height, width, PANEL, false),
                      &list, 0, None, false);

                let mut expected = vec![PANEL; width * height];
                let mut digits = [0u8; 20];
                let count = decimal_u64(size as u64, &mut digits);
                {
                    let mut f = Frame::new(&mut expected, width, height, width, PANEL, false);
                    f.well(339, 241, 570, 18, GROUND);
                    label(&mut f, 800, 244, count, 1, TEXT);
                    label(&mut f, 866, 244, "bytes", 1, SOFT);
                }
                assert!(800 + 6 * count.len() + 6 <= 866, "no gap for {size}");
                for y in 244 * scale..256 * scale {
                    for x in 800 * scale..896 * scale {
                        assert_eq!(actual[y * width + x], expected[y * width + x],
                                   "size {size} painted over or misplaced at {x},{y}");
                    }
                }

                if count.len() >= 8 {
                    let mut collided = vec![PANEL; width * height];
                    let mut f = Frame::new(&mut collided, width, height, width, PANEL, false);
                    f.well(339, 241, 570, 18, GROUND);
                    label(&mut f, 800, 244, count, 1, TEXT);
                    label(&mut f, 842, 244, "bytes", 1, SOFT);
                    assert!((244 * scale..256 * scale).any(|y|
                        (800 * scale..896 * scale).any(|x|
                            collided[y * width + x] != expected[y * width + x])),
                        "legacy overlapping units were not rejected for {size}");
                }
            }
        }
    }

    /// With TAILORBIRD_SHOT=<folder> set, write the first screen at the common sizes as P6 files.
    #[test]
    fn first_screen_can_be_drawn_at_common_sizes() {
        for (width, height) in [(1280usize, 720usize), (1920, 1080), (2560, 1440), (800, 600)] {
            let mut pixels = vec![0u32; width * height];
            let ui = Ui::new();
            let mut f = Frame::new(&mut pixels, width, height, width, GROUND, false);
            paint_frame(&mut f, &ui);
            assert!(pixels.iter().any(|&p| p != GROUND));
            if let Ok(folder) = std::env::var("TAILORBIRD_SHOT") {
                let mut out: Vec<u8> = format!("P6\n{} {}\n255\n", width, height).into_bytes();
                for p in &pixels {
                    out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
                }
                std::fs::write(format!("{}/first-{}x{}.ppm", folder, width, height), out).unwrap();
            }
        }
    }
}
