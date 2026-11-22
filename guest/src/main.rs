#![no_std]
#![no_main]

use tailorbird_ui::frame::Frame;
use tailorbird_ui::view::paint_frame;

// Native graphical *offline preview*. No model, tool service or transport is linked.
// This is a separate guest process using PCP OS' existing display and keyboard grants.
use pcp_game_guest::{call, exit, log, open, sleep_us, Display};
use tailorbird_ui::{note, ordered_keys::{KeyOutcome, OrderedKeyboard}, Text, Ui};

use tailorbird_ui::palette::GROUND;

fn paint(display: &mut Display, ui: &Ui) {
    display.draw(|pixels, width, height, pitch| {
        let mut f = Frame::new(pixels, width, height, pitch, GROUND, ui.compact_view);
        paint_frame(&mut f, ui);
    });
}

// The kernel's /dev/kbd is an ordered two-byte event FIFO. The shared
// game Display::poll reduces it to two bitmaps and loses both ordering and
// repeated keys during a single poll. Tailorbird opens the *same* existing
// keyboard device as a separate guest capability and consumes that FIFO
// directly. No kernel, firmware, shared guest or device ABI change is made.
// This is still a US-PC scancode map, not a general IME/layout service.

/// Ask PCP OS to make a note of what is typed. The person is asked by the kernel, on the kernel's own
/// screen, while this program is stopped inside the `write`; what comes back is the kernel's receipt, and
/// only a receipt that matches the job is reported as a file made.
fn make_note(proposals: isize, attempt: u64, number: u32, typed: &str) -> (Text<84>, Option<tailorbird_ui::file_job::FileJob>) {
    if proposals < 0 {
        return (Text::display(note::NOT_ALLOWED), None);
    }
    let mut prepared = match note::prepare(attempt, number, typed) {
        Ok(p) => p,
        Err(words) => return (Text::display(words), None),
    };
    let taken = call(1, proposals as usize, prepared.record.as_ptr() as usize, prepared.length, 0, 0, 0);
    if taken != prepared.length as isize || prepared.job.submitted().is_err() {
        return (Text::display(note::NOT_TAKEN), None);
    }
    let mut receipt = [0u8; 100];
    let got = call(0, proposals as usize, receipt.as_mut_ptr() as usize, receipt.len(), 0, 0, 0);
    let words = note::settle(&mut prepared.job, if got == 100 { &receipt[..] } else { &[] });
    log(b"tailorbird: ");
    log(words.as_str().as_bytes());
    log(b"\n");
    // The words go to the log; the screen reads the job itself, so nothing is cut to fit a line.
    (Text::new(), Some(prepared.job))
}

pcp_game_guest::entry!(tailorbird_main);
#[no_mangle]
pub extern "C" fn tailorbird_main() -> ! {
    let mut display = Display::current();
    let kb = open(b"/dev/kbd\0", 0x800);
    if kb < 0 {
        log(b"tailorbird: ordered keyboard unavailable\n");
        exit(2);
    }
    // Whether PCP OS lets this program propose files to it (it then lets it make none by itself). If
    // not, Alt+N says so and nothing else changes.
    let proposals = open(b"/dev/proposal\0", 2);
    let (mut attempt, mut number) = (1u64, 1u32);
    let mut ordered = OrderedKeyboard::new();
    let mut ui = Ui::new();
    ui.notes = proposals >= 0;
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
                KeyOutcome::MakeNote => {
                    let (words, job) = make_note(proposals, attempt, number, ui.input.as_str());
                    ui.notice = words;
                    if job.is_some() {
                        ui.last_note = job;
                    }
                    attempt += 1;
                    number += 1;
                    // The kernel had the display while it asked: everything is drawn again.
                    dirty = true;
                }
                KeyOutcome::Unchanged => {}
            }
        }
        if dirty { paint(&mut display, &ui); }
        sleep_us(12000);
    }
}
