#![no_std]
#![no_main]

mod frame;
use frame::Frame;

// Native graphical *offline preview*. No model, tool service or transport is linked.
// This is a separate guest process using PCP OS' existing display and keyboard grants.
use pcp_game_guest::{call, exit, log, open, sleep_us, Display};
use tailorbird_ui::{ordered_keys::{KeyOutcome, OrderedKeyboard}, Model, Ui};

use tailorbird_ui::palette::{stop_colors, AMBER, BAD, GOOD, GROUND, PANEL, SOFT, TEAL, TEXT};

fn label(f: &mut Frame<'_>, x: i32, y: i32, value: &str, scale: i32, colour: u32) {
    let mut upper = [0u8; 92];
    let mut used = 0;
    for b in value.bytes() {
        if used == upper.len() { break; }
        upper[used] = b.to_ascii_uppercase();
        used += 1;
    }
    let s = core::str::from_utf8(&upper[..used]).unwrap_or("");
    f.text(x, y, s, scale, colour);
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

fn paint(display: &mut Display, ui: &Ui) {
    display.draw(|pixels, width, height, pitch| {
        let mut f = Frame::new(pixels, width, height, pitch, GROUND, ui.compact_view);
        paint_frame(&mut f, ui);
    });
}

fn paint_frame(f: &mut Frame<'_>, ui: &Ui) {
    f.rect(0, 0, 960, 72, PANEL);
    label(f, 22, 16, "TAILORBIRD", 3, TEAL);
    label(f, 260, 23, "LOCAL TASK SURFACE", 2, TEXT);
    label(f, 737, 27, if ui.connected { "SERVICE LINK" } else { "NO SERVICE" }, 2,
          if ui.connected { GOOD } else { BAD });

    // Mode and Stop are always visible even if the model is missing.
    f.rect(20, 94, 280, 352, PANEL);
    label(f, 38, 115, "CONNECTION", 2, TEAL);
    label(f, 38, 147, ui.model.label(), 2,
          if ui.model == Model::Ready { GOOD } else { AMBER });
    label(f, 38, 187, "MODE", 2, TEAL);
    let mode_rect = tailorbird_ui::pointer::MODE_RECT;
    f.rect(mode_rect.x, mode_rect.y, mode_rect.width, mode_rect.height, GROUND);
    label(f, 48, 232, ui.mode.label(), 2, TEXT);
    if ui.mode_pending.is_some() {
        label(f, 40, 287, "CHANGE NOT CONFIRMED", 1, AMBER);
    } else {
        label(f, 40, 287, "ALT M CHANGE MODE", 1, SOFT);
    }

    let stop_rect = tailorbird_ui::pointer::STOP_RECT;
    let (stop_fill, stop_heading, stop_caption) = stop_colors(ui.controls().stop_enabled);
    f.rect(stop_rect.x, stop_rect.y, stop_rect.width, stop_rect.height, stop_fill);
    label(f, 48, 331, "STOP", 3, stop_heading);
    label(f, 48, 361, ui.task.state.label(), 1, stop_caption);
    label(f, 38, 408, "ALT S STOP TASK", 1, SOFT);

    f.rect(316, 94, 624, 352, PANEL);
    label(f, 335, 116, "TASK OUTPUT", 2, TEAL);
    label(f, 337, 150, ui.task.state.label(), 1, SOFT);
    f.rect(333, 173, 588, 2, GROUND);
    // An old grant can remain stored strictly to correlate late service
    // receipts after StopAccepted. Never let that non-actionable metadata
    // repaint a live-looking permission inspector or hide Stop/ANOMALY rows.
    // The reducer's presentation gate is distinct from stored correlation.
    if ui.controls().permission_visible {
        // Consent details occupy a focused exact-text inspector instead of
        // fighting task-output rows for the same pixels. Stop and mode stay
        // visible in the separate x=20..300 sidebar.
    } else if ui.lines().is_empty() {
        label(f, 339, 205, "NO TASK EVENTS RECEIVED", 2, SOFT);
        label(f, 339, 246, "MANUAL OS USE REMAINS AVAILABLE", 1, GOOD);
        label(f, 339, 270, "ESC RETURNS TO THE PCP OS LAUNCHER", 1, SOFT);
    } else {
        for (i, event) in ui.lines().iter().enumerate() {
            let y = 190 + i as i32 * 28;
            f.task_output(y, event.kind, event.message.as_str(), event.preview_lossy, AMBER, TEXT);
        }
    }
    if let Some(grant) = ui.permission {
        // Permission review is a full-height 588px-wide inspector. The
        // canonical action occupies 48 native 10x20 cells; target occupies
        // at most 72 cells across two lines. Neither can be silently clipped
        // by Frame::text's uppercase-only 5x7 alphabet. Three native length
        // digits preserve the distinction between trailing-space paths.
        f.rect(333, 182, 588, 254, GROUND);
        label(f, 340, 188, "PERMISSION REQUIRED", 2, AMBER);
        label(f, 340, 209, "GUI REVIEW ONLY - NO EFFECT GRANTED", 1, SOFT);
        label(f, 340, 218, "ACTION", 1, TEAL);
        label(f, 845, 218, "A LEN", 1, TEAL);
        label(f, 340, 265, "TARGET", 1, TEAL);
        label(f, 845, 265, "T LEN", 1, TEAL);
        if tailorbird_ui::exact_ascii::permission_glyphs(grant.action.as_str(), grant.target.as_str(), |x, y, byte| f.glyph(x, y, 10, 20, byte, TEXT)).is_err() {
            // Not a license to approve a field that cannot be displayed.
            f.rect(333, 182, 588, 254, GROUND);
            label(f, 340, 227, "PERMISSION NOT DISPLAYABLE", 2, BAD);
            label(f, 340, 264, "STOP TASK - SERVICE MUST REFUSE", 1, BAD);
        } else {
            let decision_label = if ui.controls().decision_enabled {
                "ALT A ALLOW - ALT D DENY"
            } else if ui.decision_pending.is_some() {
                "DECISION SENT - AWAIT SERVICE"
            } else {
                "INPUT LOCKED - NO DECISION"
            };
            label(f, 340, 338, decision_label, 2, AMBER);
            label(f, 340, 372, "SOURCE DIGEST AND EFFECT BINDING ARE SERVICE DUTIES", 1, SOFT);
            label(f, 340, 392, "NO WRITE OR AUTHORIZATION BY THIS UI", 1, SOFT);
            label(f, 340, 414, "TASK", 1, TEAL);
            let mut task_digits = [0u8; 20];
            label(f, 380, 414, decimal_u64(ui.task.id, &mut task_digits), 1, TEXT);
            label(f, 530, 414, "REQUEST", 1, TEAL);
            let mut req_digits = [0u8; 20];
            label(f, 590, 414, decimal_u64(grant.request, &mut req_digits), 1, TEXT);
        }
    }

    f.rect(20, 458, 920, 48, PANEL);
    label(f, 32, 469, "REQUEST", 1, TEAL);
    // Trailing ASCII spaces have blank font cells. Display the exact byte
    // count alongside the editor so foo and foo-space are distinguishable.
    // These count glyphs are in the x<140 margin, outside the text raster.
    label(f, 32, 489, "BYTES", 1, TEAL);
    let mut count_buf = [0u8; 10];
    label(f, 70, 489,
          pcp_game_guest::digits(ui.input.as_str().len() as u32, &mut count_buf),
          1, TEXT);
    label(f, 102, 489, "/120", 1, SOFT);
    // The text submitted to the service is the *exact* text painted here,
    // not the uppercased 5x7 label. Two lines at 78 cells expose all 120
    // stored bytes, including case-sensitive paths and source punctuation.
    // A drawing failure is visible, and the reducer independently refuses
    // submission when the editor cannot represent the request.
    if tailorbird_ui::exact_ascii::request_glyphs(ui.input.as_str(), |x, y, byte| f.glyph(x, y, 10, 20, byte, TEXT)).is_err() {
        f.rect(140, 460, 780, 42, GROUND);
        label(f, 146, 470, "INPUT NOT DISPLAYABLE", 1, BAD);
    }
    label(f, 22, 513, "ENTER SUBMIT   ALT R RETRY   ALT M MODE   ALT S STOP   ESC EXIT", 1, SOFT);
    label(f, 418, 513, if ui.compact_view { "F6: COMPACT" } else { "F6: FIT" }, 1, TEAL);
    label(f, 535, 513, ui.notice.as_str(), 1, AMBER);
}

// The kernel's /dev/kbd is an ordered two-byte event FIFO. The shared
// game Display::poll reduces it to two bitmaps and loses both ordering and
// repeated keys during a single poll. Tailorbird opens the *same* existing
// keyboard device as a separate guest capability and consumes that FIFO
// directly. No kernel, firmware, shared guest or device ABI change is made.
// This is still a US-PC scancode map, not a general IME/layout service.

pcp_game_guest::entry!(tailorbird_main);
#[no_mangle]
pub extern "C" fn tailorbird_main() -> ! {
    let mut display = Display::current();
    let kb = open(b"/dev/kbd\0", 0x800);
    if kb < 0 {
        log(b"tailorbird: ordered keyboard unavailable\n");
        exit(2);
    }
    let mut ordered = OrderedKeyboard::new();
    let mut ui = Ui::new();
    paint(&mut display, &ui);
    log(b"Tailorbird first frame presented\n");
    loop {
        let mut dirty = false;
        loop {
            let mut event = [0u8; 2];
            if call(0, kb as usize, event.as_mut_ptr() as usize, 2, 0, 0, 0) != 2 {
                break; // no more event records in nonblocking FIFO
            }
            let code = (event[0] & 127) as usize;
            let down = event[1] & 1 == 0;
            let outcome = ordered.event(code, down, &mut ui);
            match outcome {
                KeyOutcome::Exit => exit(0),
                KeyOutcome::Redraw => dirty = true,
                KeyOutcome::Unchanged => {}
            }
        }
        if dirty { paint(&mut display, &ui); }
        sleep_us(12000);
    }
}
