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
const TAYLOR: [&[u8; 16]; 14] = [
    b"............G...",
    b"...AAAA....GG...",
    b"..AAAAAA...GG...",
    b".AAAAAAAA.GG....",
    b"..GGKGGGGGGG....",
    b"SSGGGGGGGGGG....",
    b"..GWWWGGGGG.....",
    b"..WWWWWGGGG.....",
    b"..WWWWWWGG......",
    b"...WWWWWG.......",
    b"....WWWW........",
    b".....S.S........",
    b".....S.S........",
    b"....SS.SS.......",
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
    let (lamp, link) = if ui.connected { (GOOD, "Ready") } else if ui.notes { (AMBER, "Notes only") } else { (BAD, "Not ready") };
    f.well(764, 18, 176, 36, GROUND);
    f.rect(778, 31, 10, 10, lamp);
    label(f, 796, 26, link, 2, lamp);

    // Mode and Stop are always visible even if the model is missing.
    f.panel(20, 94, 280, 352, PANEL);
    label(f, 38, 115, "TAYLOR IS", 1, TEAL);
    label(f, 38, 135, sentence(ui.model.label(), &mut name), 2,
          if ui.model == Model::Ready { GOOD } else { AMBER });
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
    let (stop_fill, stop_heading, stop_caption) = stop_colors(ui.controls().stop_enabled);
    if ui.controls().stop_enabled {
        f.panel(stop_rect.x, stop_rect.y, stop_rect.width, stop_rect.height, stop_fill);
    } else {
        f.well(stop_rect.x, stop_rect.y, stop_rect.width, stop_rect.height, stop_fill);
    }
    label(f, 48, 329, "Stop", 3, stop_heading);
    label(f, 48, 365, sentence(ui.task.state.label(), &mut name), 1, stop_caption);
    key(f, 40, 410, "Alt", "+");
    key(f, 78, 410, "S", "stops the task");

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
    } else if ui.lines().is_empty() && ui.show_projects {
        projects(f, &ui.projects);
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
        if let Some(job) = &ui.last_note {
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
            label(f, 420, 384, "Type again and press Alt+N for another.", 1, SOFT);
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
        } else if ui.notes {
            // No model, but PCP OS lets Taylor ask to make files: say the one thing that works today.
            label(f, 420, 316, "Today I can save what you type as", 2, SOFT);
            label(f, 420, 340, "a note. Type it, then press Alt+N.", 2, SOFT);
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

/// The project list, in the Work panel: where the projects are, each with how many files, and in words
/// what is not known. "No projects yet" is said only of a whole list that was read and held nothing.
fn projects(f: &mut Frame<'_>, list: &crate::projects::Projects) {
    use crate::projects::Listing;
    let mut digits = [0u8; 20];
    label(f, 339, 190, "Your projects", 2, TEXT);
    match list.state {
        Listing::NotAsked => label(f, 339, 226, "Not read yet.", 2, SOFT),
        Listing::Unavailable => {
            label(f, 339, 226, "PCP OS has not let Taylor see a", 2, AMBER);
            label(f, 339, 250, "projects folder in this launch.", 2, AMBER);
        }
        Listing::Unreadable => {
            label(f, 339, 226, "The list could not be read, so", 2, AMBER);
            label(f, 339, 250, "nothing is shown. Alt+P twice retries.", 2, AMBER);
        }
        Listing::Listed => {
            label(f, 339, 218, "in the folder", 1, SOFT);
            label(f, 339 + 14 * 6, 218, list.folder.as_str(), 1, TEXT);
            if list.known_empty() {
                label(f, 339, 250, "No projects yet.", 2, SOFT);
                label(f, 339, 280, "Type something and press Alt+N to make a note.", 1, SOFT);
            }
            for (row, entry) in list.rows[..list.shown].iter().enumerate() {
                let y = 240 + row as i32 * 22;
                f.well(339, y - 3, 570, 20, GROUND);
                let name = entry.project.as_str();
                // A name too long for the row is shown cut, and marked as cut.
                let (fits, cut) = if name.len() > 72 { (&name[..69], true) } else { (name, false) };
                label(f, 347, y, fits, 1, TEXT);
                if cut {
                    label(f, 347 + 69 * 6, y, "...", 1, AMBER);
                }
                label(f, 800, y, decimal_u64(entry.files as u64, &mut digits), 1, TEXT);
                label(f, 830, y, if entry.files == 1 { "file" } else { "files" }, 1, SOFT);
            }
            let mut y = 240 + list.shown as i32 * 22 + 4;
            if list.projects > list.shown {
                label(f, 339, y, "more projects not shown:", 1, AMBER);
                label(f, 339 + 25 * 6, y, decimal_u64((list.projects - list.shown) as u64, &mut digits), 1, AMBER);
                y += 14;
            }
            if list.other > 0 {
                label(f, 339, y, "other files in the folder:", 1, SOFT);
                label(f, 339 + 27 * 6, y, decimal_u64(list.other as u64, &mut digits), 1, SOFT);
                y += 14;
            }
            if !list.complete {
                label(f, 339, y, "PCP OS listed only part of the folder. There is more.", 1, AMBER);
            }
        }
    }
    let x = key(f, 339, 420, "Alt+P", "goes back");
    key(f, x, 420, "Esc", "returns to Programs");
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
        ui.last_note = Some(p.job);
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
        refused.last_note = Some(declined.job);
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
        ui.last_note = Some(p.job);
        let made = shot(&ui);
        assert_ne!(unknown, made);
        // Everything outside the Work panel's lower part is the same in all four.
        for (i, (a, b)) in plain.iter().zip(made.iter()).enumerate() {
            let (x, y) = (i % 960, i / 960);
            assert!(a == b || (x >= 316 && x < 940 && y >= 280 && y < 446) || (x >= 764 && y >= 18 && y < 54), "differs at {x},{y}");
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
