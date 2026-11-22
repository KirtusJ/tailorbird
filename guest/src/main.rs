#![no_std]
#![no_main]

use tailorbird_ui::frame::Frame;
use tailorbird_ui::view::paint_frame;

// Native graphical *offline preview*. No model, tool service or transport is linked.
// This is a separate guest process using PCP OS' existing display and keyboard grants.
use pcp_game_guest::{call, exit, log, open, sleep_us, Display};
use tailorbird_ui::{note, ordered_keys::{KeyOutcome, OrderedKeyboard}, Text, Ui};
use tailorbird_ui::build_folder_snapshot::MAX_SNAPSHOT_BYTES;
use tailorbird_ui::project_location::BuildFolder;
use tailorbird_ui::projects::{Checked, Files, Listing, Opened, Projects, Viewed, VIEW_BYTES};
use tailorbird_ui::project_location::FileDestination;
use tailorbird_ui::sha256::sha256;

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

/// One fresh list of the build folder: opened, read to its end, closed on every path, then checked whole.
/// A read that fills the buffer is more than a list may be, and is not shown.
/// Everything a descriptor holds, to its end, and the descriptor closed whatever happens. `None` when a
/// read failed or the end was not reached with room to spare: then nothing read is used.
fn read_whole(fd: isize, into: &mut [u8]) -> Option<usize> {
    let mut have = 0usize;
    let ended = loop {
        if have == into.len() {
            break false;
        }
        let got = call(0, fd as usize, into[have..].as_mut_ptr() as usize, into.len() - have, 0, 0, 0);
        if got == 0 {
            break true;
        }
        if got < 0 {
            break false;
        }
        have += got as usize;
    };
    call(3, fd as usize, 0, 0, 0, 0, 0);
    if ended { Some(have) } else { None }
}

/// One fresh list of the build folder, as its bytes; the state says why when there are none.
fn read_list() -> Result<&'static [u8], Listing> {
    static mut LIST: [u8; MAX_SNAPSHOT_BYTES + 1] = [0; MAX_SNAPSHOT_BYTES + 1];
    // Safety: this program has one thread, and each caller is done with the bytes before the next call.
    let list = unsafe { &mut *core::ptr::addr_of_mut!(LIST) };
    let fd = open(b"/dev/build-folder\0", 0);
    if fd < 0 {
        return Err(Listing::Unavailable);
    }
    match read_whole(fd, list) {
        Some(have) => Ok(&list[..have]),
        None => Err(Listing::Unreadable),
    }
}

fn read_projects(expected: Option<(BuildFolder, u64)>) -> Projects {
    match read_list() {
        Ok(bytes) => Projects::read(bytes, expected),
        Err(Listing::Unavailable) => Projects::unavailable(),
        Err(_) => Projects::unreadable(),
    }
}

/// One file under the build folder, read whole and to its end. If this run's last note is this very
/// file, what was read is compared with the kernel's receipt for it, and the job remembers the answer.
fn view_file(destination: FileDestination, last_note: &mut Option<tailorbird_ui::file_job::FileJob>) -> Viewed {
    static mut FILE: [u8; VIEW_BYTES + 1] = [0; VIEW_BYTES + 1];
    // Safety: one thread; used only inside this function.
    let file = unsafe { &mut *core::ptr::addr_of_mut!(FILE) };
    let mut path = [0u8; 256];
    let name = destination.as_str().as_bytes();
    path[..name.len()].copy_from_slice(name);
    let job = last_note.as_mut().filter(|job| job.open_target().is_some_and(|d| d.as_str() == destination.as_str())
        || job.readback() == tailorbird_ui::file_job::Readback::Changed && job.destination().as_str() == destination.as_str());
    let fd = open(&path[..name.len() + 1], 0);
    if fd < 0 {
        if let Some(job) = job {
            let _ = job.open_unavailable();
        }
        return Viewed::failed(destination, Opened::Unavailable);
    }
    // Probe how the read ended before deciding it was too large: a full buffer may be a larger file.
    let Some(have) = read_whole(fd, file) else {
        // Either a failed read or more than a text file holds. With a receipt to compare, more bytes
        // than were made is a different file; otherwise only "too large" or "unreadable" is known.
        if let Some(job) = job {
            let _ = job.verify_readback(file.len() as u64, [0; 32], false);
            let mut v = Viewed::failed(destination, Opened::TooLarge);
            v.checked = Checked::Differs;
            return v;
        }
        return Viewed::failed(destination, Opened::TooLarge);
    };
    let content = &file[..have];
    let checked = match job {
        Some(job) => if job.verify_readback(have as u64, sha256(&[content]), true).is_ok() { Checked::Matches } else { Checked::Differs },
        None => Checked::NotThisRun,
    };
    Viewed::read(destination, content, checked)
}

/// The folder and run the first good list of this launch named; later lists must name the same.
fn same_launch(projects: &Projects) -> Option<(BuildFolder, u64)> {
    if projects.state != Listing::Listed {
        return None;
    }
    BuildFolder::new(projects.folder.as_str()).ok().map(|root| (root, projects.launch))
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
    let mut launch: Option<(BuildFolder, u64)> = None;
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
                KeyOutcome::Projects => {
                    if ui.show_projects {
                        match ui.browse_level {
                            0 => {
                                let fresh = read_projects(launch);
                                if launch.is_none() {
                                    launch = same_launch(&fresh);
                                }
                                ui.projects = fresh;
                                ui.place = ui.place.min(ui.projects.shown.saturating_sub(1));
                            }
                            1 => {
                                ui.files = match (read_list(), ui.projects.rows[ui.place].location) {
                                    (Ok(bytes), Some(project)) => Files::read(bytes, launch, project),
                                    (Err(state), _) => Files::failed(state),
                                    (_, None) => Files::failed(Listing::Unreadable),
                                };
                                ui.file_place = ui.file_place.min(ui.files.shown.saturating_sub(1));
                            }
                            _ => {
                                ui.viewed = match ui.files.rows[ui.file_place].destination {
                                    Some(destination) => view_file(destination, &mut ui.last_note),
                                    None => Viewed::new(),
                                };
                            }
                        }
                    }
                    dirty = true;
                }
                KeyOutcome::Unchanged => {}
            }
        }
        if dirty { paint(&mut display, &ui); }
        sleep_us(12000);
    }
}
