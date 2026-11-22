#![no_std]
#![no_main]

use tailorbird_ui::frame::Frame;
use tailorbird_ui::view::paint_frame;

// Native graphical *offline preview*. No model, tool service or transport is linked.
// This is a separate guest process using PCP OS' existing display and keyboard grants.
use pcp_game_guest::{call, exit, log, open, sleep_us, Display};
use tailorbird_ui::talk::{Reply, Service, Who};
use tailorbird_ui::{link::Link, note, ordered_keys::{KeyOutcome, OrderedKeyboard}, Text, Ui};
use tailorbird_ui::build_folder_snapshot::MAX_SNAPSHOT_BYTES;
use tailorbird_ui::project_location::BuildFolder;
use tailorbird_ui::projects::{read_whole, view, Files, Listing, Projects, Viewed, Whole, VIEW_BYTES};
use tailorbird_ui::project_location::FileDestination;

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
fn make_note(proposals: isize, project: tailorbird_ui::project_location::ProjectLocation, attempt: u64, number: u32, typed: &str) -> (Text<84>, Option<tailorbird_ui::file_job::FileJob>) {
    if proposals < 0 {
        return (Text::display(note::NOT_ALLOWED), None);
    }
    let mut prepared = match note::prepare_in(project, attempt, number, typed) {
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
fn reader(fd: isize) -> impl FnMut(&mut [u8]) -> isize {
    move |into: &mut [u8]| call(0, fd as usize, into.as_mut_ptr() as usize, into.len(), 0, 0, 0)
}

/// One fresh list of the build folder, as its bytes, the descriptor closed whatever happened. Only a
/// refusal of the device itself is "not offered"; any other failure is "could not be read".
fn read_list() -> Result<&'static [u8], Listing> {
    static mut LIST: [u8; MAX_SNAPSHOT_BYTES + 1] = [0; MAX_SNAPSHOT_BYTES + 1];
    // Safety: this program has one thread, and each caller is done with the bytes before the next call.
    let list = unsafe { &mut *core::ptr::addr_of_mut!(LIST) };
    let fd = open(b"/dev/build-folder\0", 0);
    if fd == -13 || fd == -2 {
        return Err(Listing::Unavailable);
    }
    if fd < 0 {
        return Err(Listing::Unreadable);
    }
    let whole = read_whole(reader(fd), list);
    call(3, fd as usize, 0, 0, 0, 0, 0);
    match whole {
        Whole::Ended(have) => Ok(&list[..have]),
        _ => Err(Listing::Unreadable),
    }
}

/// A list that failed keeps the folder an earlier list of this launch named, so the screen can still
/// say where the projects are.
fn read_projects(expected: Option<(BuildFolder, u64)>) -> Projects {
    read_projects_from(expected, 0)
}

fn read_projects_from(expected: Option<(BuildFolder, u64)>, top: usize) -> Projects {
    let mut projects = match read_list() {
        Ok(bytes) => Projects::read_from(bytes, expected, top),
        Err(Listing::Unavailable) => Projects::unavailable(),
        Err(_) => Projects::unreadable(),
    };
    if projects.state == Listing::Unreadable {
        if let Some((root, _)) = expected {
            projects.folder = Text::display(root.as_str());
        }
    }
    projects
}

/// One file under the build folder. If a kept job is for this very file, what is read is compared with
/// the kernel's receipt for it and the job remembers the answer.
fn view_file(destination: FileDestination, jobs: &mut tailorbird_ui::jobs::Jobs) -> Viewed {
    static mut FILE: [u8; VIEW_BYTES + 1] = [0; VIEW_BYTES + 1];
    // Safety: one thread; used only inside this function.
    let file = unsafe { &mut *core::ptr::addr_of_mut!(FILE) };
    let mut path = [0u8; 256];
    let name = destination.as_str().as_bytes();
    path[..name.len()].copy_from_slice(name);
    let job = jobs.made(&destination);
    let fd = open(&path[..name.len() + 1], 0);
    let viewed = view(destination, if fd >= 0 { Some(reader(fd)) } else { None }, file, job);
    if fd >= 0 {
        call(3, fd as usize, 0, 0, 0, 0, 0);
    }
    viewed
}

/// What the console was last told about the conversation. The console is told how the model service
/// stands and how each reply ended, with a count of bytes; never what was said.
struct Said {
    service: Service,
    reply: u64,
    state: Reply,
}

impl Said {
    fn changes(&mut self, ui: &Ui) {
        if ui.talk.service != self.service {
            self.service = ui.talk.service;
            log(match self.service {
                Service::Absent => b"tailorbird: model service: none\n",
                Service::Loading => b"tailorbird: model service: loading\n",
                Service::Ready => b"tailorbird: model service: ready\n",
                Service::Failed => b"tailorbird: model service: did not start\n",
                Service::Stopped => b"tailorbird: model service: stopped\n",
            });
        }
        let Some(turn) = ui.talk.turns().iter().rev().find(|t| t.who == Who::Taylor) else { return };
        if (turn.reply, turn.state) == (self.reply, self.state) {
            return;
        }
        (self.reply, self.state) = (turn.reply, turn.state);
        let words = match turn.state {
            Reply::Streaming => " is being answered\n",
            Reply::Complete => " complete\n",
            Reply::Cancelled => " stopped by the person\n",
            Reply::Failed => " failed\n",
            Reply::None => return,
        };
        // One line in one write: another program writes to the same console.
        let mut line = ConsoleLine { bytes: [0; 96], length: 0 };
        push(&mut line, "tailorbird: reply ");
        number(&mut line, turn.reply);
        if turn.state != Reply::Streaming {
            push(&mut line, " kept ");
            number(&mut line, turn.text.as_str().len() as u64);
            push(&mut line, " bytes and is");
        }
        push(&mut line, words);
        log(&line.bytes[..line.length]);
    }
}

struct ConsoleLine {
    bytes: [u8; 96],
    length: usize,
}

fn push(line: &mut ConsoleLine, words: &str) {
    for b in words.bytes() {
        if line.length < line.bytes.len() {
            line.bytes[line.length] = b;
            line.length += 1;
        }
    }
}

fn number(line: &mut ConsoleLine, mut value: u64) {
    let mut digits = [0u8; 20];
    let mut count = 0;
    loop {
        digits[count] = b'0' + (value % 10) as u8;
        value /= 10;
        count += 1;
        if value == 0 {
            break;
        }
    }
    for i in (0..count).rev() {
        push(line, core::str::from_utf8(&digits[i..i + 1]).unwrap_or("?"));
    }
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
    let (mut attempt, mut number) = (1u64, Some(1u32));
    let mut launch: Option<(BuildFolder, u64)> = None;
    let mut ordered = OrderedKeyboard::new();
    let mut ui = Ui::new();
    ui.notes = proposals >= 0;
    // Learn from PCP OS, not from a constant, which folder this launch builds into.
    if ui.notes {
        let first = read_projects(None);
        launch = same_launch(&first);
        ui.active = launch.and_then(|(root, _)| note::default_project(root));
        ui.projects = first;
    }
    // The channel to a local model service, if PCP OS started one beside this program. Opened not
    // to wait: the person's keys are read whether or not a model has anything to say.
    let talk = open(b"/dev/talk\0", 2 | 0x800);
    let mut link = Link::new(talk >= 0, &mut ui);
    log(if talk >= 0 { b"tailorbird: a model service channel is open\n" } else { b"tailorbird: no model service channel\n" });
    let mut said = Said { service: ui.talk.service, reply: 0, state: Reply::None };
    paint(&mut display, &ui);
    log(b"Tailorbird first frame presented\n");
    loop {
        let mut dirty = link.turn(&mut ui,
            |into| call(0, talk as usize, into.as_mut_ptr() as usize, into.len(), 0, 0, 0),
            |from| call(1, talk as usize, from.as_ptr() as usize, from.len(), 0, 0, 0));
        said.changes(&ui);
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
                KeyOutcome::GiveUp => {
                    // The person has said, twice, that Taylor is to make no more files. The program
                    // tells PCP OS it gives the right up, and says so on the screen only if PCP OS's
                    // own receipt says so: from then on PCP OS refuses without asking, whatever this
                    // program or a model later does.
                    let record = tailorbird_ui::kernel_proposal::give_up_record(attempt);
                    let taken = call(1, proposals as usize, record.as_ptr() as usize, record.len(), 0, 0, 0);
                    let mut receipt = [0u8; 100];
                    let got = if taken == record.len() as isize { call(0, proposals as usize, receipt.as_mut_ptr() as usize, receipt.len(), 0, 0, 0) } else { -1 };
                    ui.gave_up = got == 100 && tailorbird_ui::kernel_proposal::gave_up(&receipt, attempt);
                    ui.notice = Text::display(if ui.gave_up { note::GAVE_UP } else { note::GIVE_UP_UNCONFIRMED });
                    log(b"tailorbird: ");
                    log(ui.notice.as_str().as_bytes());
                    log(b"\n");
                    attempt += 1;
                    dirty = true;
                }
                KeyOutcome::MakeNote if ui.gave_up => {
                    // Nothing is asked of PCP OS: it would refuse, and the person is told why here.
                    ui.talk_shown = false;
                    ui.show_projects = false;
                    ui.browse_level = 0;
                    ui.notice = Text::display(note::GAVE_UP);
                    dirty = true;
                }
                KeyOutcome::MakeNote => {
                    // Where the note goes is fixed before anything is asked: the active project, and a
                    // number a fresh list suggests. The kernel still makes it only if that name is free.
                    // The folder comes only from PCP OS's list: if it has not been read yet it is read
                    // now, and if it still is not known nothing is asked and no folder is assumed.
                    if launch.is_none() {
                        let fresh = read_projects(None);
                        launch = same_launch(&fresh);
                        ui.projects = fresh;
                    }
                    if ui.active.is_none() {
                        ui.active = launch.and_then(|(root, _)| note::default_project(root));
                    }
                    let list = read_list().ok();
                    let (words, job) = match note::decide(proposals >= 0, ui.jobs.is_full(), launch.map(|(root, _)| root), ui.active, list, number) {
                        Err(words) => (Text::display(words), None),
                        Ok((project, this, next)) => {
                            number = next;
                            make_note(proposals, project, attempt, this, ui.input.as_str())
                        }
                    };
                    // What became of it is shown at once, not hidden behind a list read before it.
                    ui.talk_shown = false;
                    ui.show_projects = false;
                    ui.browse_level = 0;
                    ui.notice = words;
                    if let Some(job) = job {
                        // Room was checked before the kernel was asked, so this cannot fail.
                        let _ = ui.jobs.keep(job);
                    }
                    attempt += 1;
                    // The kernel had the display while it asked: everything is drawn again.
                    dirty = true;
                }
                KeyOutcome::Projects => {
                    if ui.show_projects {
                        match ui.browse_level {
                            0 => {
                                let fresh = read_projects_from(launch, ui.top);
                                if launch.is_none() {
                                    launch = same_launch(&fresh);
                                }
                                // The first list to succeed, here as at start or on Alt+N, also says
                                // where a new note goes, unless a project was already chosen.
                                if ui.active.is_none() {
                                    ui.active = launch.and_then(|(root, _)| note::default_project(root));
                                }
                                ui.projects = fresh;
                                ui.top = ui.projects.top;
                                ui.place = ui.place.min(ui.projects.shown.saturating_sub(1));
                            }
                            1 => {
                                ui.files = match (read_list(), ui.projects.rows[ui.place].location) {
                                    (Ok(bytes), Some(project)) => {
                                        // Opening a project chooses it: new notes go there, and both
                                        // lists and the first screen say so.
                                        ui.active = Some(project);
                                        number = Some(1);
                                        Files::read_from(bytes, launch, project, ui.file_top)
                                    }
                                    (Err(state), _) => Files::failed(state),
                                    (_, None) => Files::failed(Listing::Unreadable),
                                };
                                ui.file_top = ui.files.top;
                                ui.file_place = ui.file_place.min(ui.files.shown.saturating_sub(1));
                            }
                            _ => {
                                ui.viewed = match ui.files.rows[ui.file_place].destination {
                                    Some(destination) => view_file(destination, &mut ui.jobs),
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
