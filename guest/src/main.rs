#![no_std]
#![no_main]

// Native graphical *offline preview*. No model, tool service or transport is linked.
// This is a separate guest process using PCP OS' existing display and keyboard grants.
use pcp_game_guest::{exit, ink::Frame, sleep_us, Display};
use tailorbird_ui::{Action, Model, Ui};

const W: usize = 960;
const H: usize = 540;
const GROUND: u32 = 0x10283f;
const PANEL: u32 = 0x18364f;
const TEXT: u32 = 0xf2f7fc;
const SOFT: u32 = 0x9fb4c8;
const TEAL: u32 = 0x41bad7;
const AMBER: u32 = 0xe3a24b;
const GOOD: u32 = 0x8fe0c2;
const BAD: u32 = 0xf08a72;

// The only unsafe code here borrows this app's own framebuffer.
// The underlying mapping and page flip are handled by pcp-game-guest.
static mut PIXELS: [u32; W * H] = [0; W * H];

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

fn paint(display: &mut Display, ui: &Ui) {
    let pixels = unsafe { &mut *(&raw mut PIXELS) };
    pixels.fill(GROUND);
    let mut f = Frame { px: pixels, pitch: W };
    f.rect(0, 0, W as i32, 72, PANEL);
    label(&mut f, 22, 16, "TAILORBIRD", 3, TEAL);
    label(&mut f, 260, 23, "LOCAL TASK SURFACE", 2, TEXT);
    label(&mut f, 737, 27, if ui.connected { "SERVICE LINK" } else { "NO SERVICE" }, 2,
          if ui.connected { GOOD } else { BAD });

    // Mode and Stop are always visible even if the model is missing.
    f.rect(20, 94, 280, 352, PANEL);
    label(&mut f, 38, 115, "CONNECTION", 2, TEAL);
    label(&mut f, 38, 147, ui.model.label(), 2,
          if ui.model == Model::Ready { GOOD } else { AMBER });
    label(&mut f, 38, 187, "MODE", 2, TEAL);
    f.rect(34, 218, 254, 58, GROUND);
    label(&mut f, 48, 232, ui.mode.label(), 2, TEXT);
    if ui.mode_pending.is_some() {
        label(&mut f, 40, 287, "CHANGE NOT CONFIRMED", 1, AMBER);
    } else {
        label(&mut f, 40, 287, "ALT M CHANGE MODE", 1, SOFT);
    }

    f.rect(34, 318, 254, 72, if ui.controls().stop_enabled { BAD } else { GROUND });
    label(&mut f, 48, 331, "STOP", 3, TEXT);
    label(&mut f, 48, 361, ui.task.state.label(), 1, SOFT);
    label(&mut f, 38, 408, "ALT S STOP TASK", 1, SOFT);

    f.rect(316, 94, 624, 352, PANEL);
    label(&mut f, 335, 116, "TASK OUTPUT", 2, TEAL);
    label(&mut f, 337, 150, ui.task.state.label(), 1, SOFT);
    f.rect(333, 173, 588, 2, GROUND);
    if ui.lines().is_empty() {
        label(&mut f, 339, 205, "NO TASK EVENTS RECEIVED", 2, SOFT);
        label(&mut f, 339, 246, "MANUAL OS USE REMAINS AVAILABLE", 1, GOOD);
        label(&mut f, 339, 270, "ESC RETURNS TO THE PCP OS LAUNCHER", 1, SOFT);
    } else {
        for (i, event) in ui.lines().iter().enumerate() {
            let y = 190 + i as i32 * 28;
            label(&mut f, 336, y, event.kind, 1, AMBER);
            label(&mut f, 438, y, event.message.as_str(), 1, TEXT);
        }
    }
    if let Some(grant) = ui.permission {
        f.rect(335, 385, 585, 52, GROUND);
        // Put the exact action and target on separate rows. The underlying
        // 5x7 font advances 6 px/character at scale 1. 48 action bytes
        // from x=390 end at 678; 72 target bytes end at x=822, both
        // within the x=335..920 permission panel with no overlap.
        label(&mut f, 342, 392, "PERMISSION REQUIRED", 1, AMBER);
        label(&mut f, 342, 406, "ACTION", 1, TEAL);
        label(&mut f, 390, 406, grant.action.as_str(), 1, TEXT);
        label(&mut f, 342, 420, "TARGET", 1, TEAL);
        label(&mut f, 390, 420, grant.target.as_str(), 1, TEXT);
        let decision_label = if ui.controls().decision_enabled {
            "ALT A / ALT D"
        } else if ui.decision_pending.is_some() {
            "DECISION SENT"
        } else {
            "INPUT LOCKED"
        };
        label(&mut f, 760, 392, decision_label, 1, SOFT);
    }

    f.rect(20, 458, 920, 48, PANEL);
    label(&mut f, 32, 469, "REQUEST", 1, TEAL);
    label(&mut f, 140, 469, ui.input.as_str(), 1, TEXT);
    label(&mut f, 22, 513, "ENTER SUBMIT   ALT M MODE   ALT S STOP   ESC EXIT", 1, SOFT);
    label(&mut f, 535, 513, ui.notice.as_str(), 1, AMBER);
    display.show(pixels);
}

// PC set-1 codes as exposed by the existing PCP OS keyboard device.
// This minimal font is ASCII-only. Extended text, IME, pointer and accessibility
// APIs are future daily-driver requirements, not demonstrated by this preview.
fn ascii(code: usize, shift: bool) -> Option<u8> {
    let lower = match code {
        2..=10 => b'1' + (code as u8 - 2),
        11 => b'0',
        12 => b'-',
        16 => b'q', 17 => b'w', 18 => b'e', 19 => b'r', 20 => b't',
        21 => b'y', 22 => b'u', 23 => b'i', 24 => b'o', 25 => b'p',
        30 => b'a', 31 => b's', 32 => b'd', 33 => b'f', 34 => b'g',
        35 => b'h', 36 => b'j', 37 => b'k', 38 => b'l',
        44 => b'z', 45 => b'x', 46 => b'c', 47 => b'v', 48 => b'b',
        49 => b'n', 50 => b'm',
        53 => b'/', 57 => b' ',
        _ => return None,
    };
    Some(if shift { lower.to_ascii_uppercase() } else { lower })
}

pcp_game_guest::entry!(tailorbird_main);
#[no_mangle]
pub extern "C" fn tailorbird_main() -> ! {
    let mut display = Display::new(W, H);
    let mut ui = Ui::new();
    paint(&mut display, &ui);
    loop {
        display.poll();
        let mut dirty = false;
        if display.pressed[1] { exit(0); }
        let alt = display.held[56];
        if alt {
            if display.pressed[50] { let _ = ui.intent(Action::ToggleMode); dirty = true; }
            if display.pressed[31] { let _ = ui.intent(Action::Stop); dirty = true; }
            if display.pressed[30] { let _ = ui.intent(Action::Approve); dirty = true; }
            if display.pressed[32] { let _ = ui.intent(Action::Deny); dirty = true; }
        } else {
            if display.pressed[28] { let _ = ui.intent(Action::Submit); dirty = true; }
            if display.pressed[14] { dirty |= ui.input.pop(); }
            let shift = display.held[42] || display.held[54];
            for code in 0..128 {
                if display.pressed[code] {
                    if let Some(b) = ascii(code, shift) { dirty |= ui.input.push_ascii(b); }
                }
            }
        }
        if dirty { paint(&mut display, &ui); }
        sleep_us(12000);
    }
}
