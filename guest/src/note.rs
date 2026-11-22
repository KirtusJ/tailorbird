//! The first thing Taylor can make: a note of exactly what was typed, as a new file in the current
//! project. No model is involved. This module prepares the proposal and reads the kernel's answer into
//! plain words; the two calls to the kernel between are made by the program itself.
use crate::file_job::{FileJob, Outcome, Stage};
use crate::kernel_proposal as wire;
#[cfg(test)]
use crate::project_location::DEFAULT_BUILD_FOLDER;
use crate::project_location::{BuildFolder, ProjectLocation};
use crate::Text;

/// Until projects can be chosen on the screen there is one, in the folder Taylor builds into.
pub const PROJECT: &str = "my-first-project";
pub const RECORD: usize = wire::HEADER + wire::MAX_NAME + crate::INPUT_LIMIT + 1;

pub const NOTHING_TYPED: &str = "Type something first, then press Alt+N.";
pub const NO_FOLDER: &str = "PCP OS has not said where projects go, so nothing was asked.";
pub const NO_ROOM_FOR_NAME: &str = "The projects folder name is too long for a note name. Nothing was asked.";
pub const NO_NUMBER: &str = "This project has used every note number. Choose another project.";
pub const NOT_ALLOWED: &str = "PCP OS has not let Taylor make files here.";
pub const NOT_TAKEN: &str = "PCP OS did not take the request. Nothing was made.";
pub const UNREADABLE: &str = "PCP OS answered in a way Taylor cannot read. Nothing is claimed.";

pub struct Prepared {
    pub job: FileJob,
    pub record: [u8; RECORD],
    pub length: usize,
}

/// The file's name within the project: note-1.txt, note-2.txt, ...
fn file_name(number: u32, out: &mut [u8; 24]) -> &str {
    let mut digits = [0u8; 10];
    let (mut n, mut count) = (number, 0);
    loop {
        digits[count] = b'0' + (n % 10) as u8;
        n /= 10;
        count += 1;
        if n == 0 {
            break;
        }
    }
    let mut used = 0;
    for b in b"note-".iter().copied().chain(digits[..count].iter().rev().copied()).chain(b".txt".iter().copied()) {
        out[used] = b;
        used += 1;
    }
    core::str::from_utf8(&out[..used]).unwrap_or("note.txt")
}

/// The number for the next note: one more than the highest note already in the project, by a list of
/// the build folder just read, and never less than `at_least` (what this run has already used). With no
/// readable list it is `at_least`: the kernel still refuses a name that is taken, and says so.
#[cfg(test)]
pub fn next_number(list: Option<&[u8]>, at_least: u32) -> u32 {
    next_number_in(list, &usual_project(), Some(at_least)).unwrap_or(u32::MAX)
}

/// For tests only: the project under the usual folder. The program itself never assumes a folder.
#[cfg(test)]
pub fn usual_project() -> ProjectLocation {
    default_project(BuildFolder::new(DEFAULT_BUILD_FOLDER).unwrap()).unwrap()
}

/// The first project of the build folder PCP OS named. None when that folder's name leaves no room
/// for the project's.
pub fn default_project(root: BuildFolder) -> Option<ProjectLocation> {
    ProjectLocation::new(root, PROJECT).ok()
}

/// The same for any project. A candidate only: the list may be partial or already old, and it is the
/// kernel, making the file only if the name is free, that decides.
pub fn next_number_in(list: Option<&[u8]>, into: &ProjectLocation, at_least: Option<u32>) -> Option<u32> {
    use crate::build_folder_snapshot::{EntryKind, Snapshot};
    // `at_least` is None when this run has already used the last number there is.
    let mut next = at_least?;
    let Some(Ok(snapshot)) = list.map(Snapshot::parse) else { return Some(next) };
    for entry in snapshot.entries() {
        let EntryKind::ProjectFile { project, destination } = entry.kind else { continue };
        if project != *into {
            continue;
        }
        let Some(digits) = destination.file_name().strip_prefix("note-").and_then(|n| n.strip_suffix(".txt")) else { continue };
        // Only names this program would itself write: digits with no leading zero, within range.
        if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if let Ok(number) = digits.parse::<u32>() {
            // The last number there is, already used: there is no next, and none is invented.
            next = next.max(number.checked_add(1)?);
        }
    }
    Some(next)
}

/// Everything decided before the kernel is asked for a note, in one place: whether it may be asked at
/// all, into which project, and under which number. `folder` is the build folder a checked list of this
/// launch named (None: PCP OS has not said); `active` the project chosen (None: the first project of
/// that folder, if its name fits). On success: the project, the number to ask for, and the number the
/// next request in that project starts from (None when this one is the last there is).
pub fn decide(allowed: bool, full: bool, folder: Option<BuildFolder>, active: Option<ProjectLocation>, list: Option<&[u8]>,
              number: Option<u32>) -> Result<(ProjectLocation, u32, Option<u32>), &'static str> {
    if !allowed {
        return Err(NOT_ALLOWED);
    }
    if full {
        return Err(crate::jobs::FULL);
    }
    let folder = folder.ok_or(NO_FOLDER)?;
    let project = match active {
        Some(project) => project,
        None => default_project(folder).ok_or(NO_ROOM_FOR_NAME)?,
    };
    let next = next_number_in(list, &project, number).ok_or(NO_NUMBER)?;
    Ok((project, next, next.checked_add(1)))
}

/// What is typed, with a new line after it, as note `number` of the current project. The job is ready to
/// be handed to the kernel: its record is built and it knows what the receipt must say.
#[cfg(test)]
pub fn prepare(attempt: u64, number: u32, typed: &str) -> Result<Prepared, &'static str> {
    prepare_in(usual_project(), attempt, number, typed)
}

/// The same, in the project given. The name is fixed here, before the kernel shows it to the person.
pub fn prepare_in(project: ProjectLocation, attempt: u64, number: u32, typed: &str) -> Result<Prepared, &'static str> {
    if typed.is_empty() {
        return Err(NOTHING_TYPED);
    }
    let mut content = [0u8; crate::INPUT_LIMIT + 1];
    let bytes = typed.as_bytes();
    if bytes.len() > crate::INPUT_LIMIT {
        return Err(NOT_TAKEN);
    }
    content[..bytes.len()].copy_from_slice(bytes);
    content[bytes.len()] = b'\n';
    let content = &content[..bytes.len() + 1];
    let mut name = [0u8; 24];
    let mut job = FileJob::new(attempt, typed, project, file_name(number, &mut name)).map_err(|_| NOT_TAKEN)?;
    let mut record = [0u8; RECORD];
    let path = job.destination();
    let binding = wire::binding(1, path, content).map_err(|_| NOT_TAKEN)?;
    let length = wire::record(&mut record, attempt, 1, path, content).map_err(|_| NOT_TAKEN)?;
    job.prepare(binding).map_err(|_| NOT_TAKEN)?;
    Ok(Prepared { job, record, length })
}

/// The kernel has answered (or not). Says what happened in words for anyone, and only what the receipt
/// itself establishes: a receipt that does not match this job claims nothing.
pub fn settle(job: &mut FileJob, receipt: &[u8]) -> Text<84> {
    let read = match wire::receipt(receipt) {
        Ok(r) => r,
        Err(_) => return Text::display(UNREADABLE),
    };
    if job.record_receipt(read).is_err() {
        return Text::display(UNREADABLE);
    }
    let mut words = Text::<84>::new();
    let mut say = |s: &str| {
        for b in s.bytes() {
            let _ = words.push_ascii(b);
        }
    };
    match (job.stage(), read.outcome) {
        (Stage::Created, _) => {
            say("Made ");
            say(job.destination().as_str());
            say(". Kept until power-off.");
        }
        (_, Outcome::Declined) => say("Not made: you said no."),
        (_, Outcome::NameTaken) => say("Not made: that name is already taken."),
        (_, Outcome::NoSpace) => say("Not made: there is no room left."),
        (_, Outcome::CannotDisplay) => say("Not made: this display is too small to show it to you."),
        _ => say(UNREADABLE),
    }
    words
}

/// The same answer for the screen, read from the kept job each time it is drawn: a headline and whether
/// it is good news. Where the file is and how long it lasts are drawn separately, from the job.
pub fn headline(job: &FileJob) -> (&'static str, bool) {
    match (job.stage(), job.receipt().map(|r| r.outcome)) {
        (Stage::Created | Stage::Verified, _) => ("I made your note.", true),
        (_, Some(Outcome::Declined)) => ("Not made: you said no.", false),
        (_, Some(Outcome::NameTaken)) => ("Not made: that name is taken.", false),
        (_, Some(Outcome::NoSpace)) => ("Not made: no room is left.", false),
        (_, Some(Outcome::CannotDisplay)) => ("Not made: display too small.", false),
        _ => ("I cannot tell what happened.", false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha256::sha256;

    fn kernel_receipt(prepared: &Prepared, outcome: u32, launch: u64) -> [u8; wire::RECEIPT] {
        let name_len = u32::from_le_bytes(prepared.record[24..28].try_into().unwrap()) as usize;
        let (name, content) = prepared.record[wire::HEADER..prepared.length].split_at(name_len);
        let mut r = [0u8; wire::RECEIPT];
        r[0..4].copy_from_slice(&1u32.to_le_bytes());
        r[4..8].copy_from_slice(&outcome.to_le_bytes());
        r[8..16].copy_from_slice(&launch.to_le_bytes());
        r[16..24].copy_from_slice(&prepared.record[8..16]);
        r[24..32].copy_from_slice(&prepared.record[16..24]);
        r[32..64].copy_from_slice(&wire::request_digest(core::str::from_utf8(name).unwrap(), content));
        r[64..96].copy_from_slice(&sha256(&[content]));
        r
    }

    #[test]
    fn a_note_is_exactly_what_was_typed_in_the_projects_folder() {
        let p = prepare(3, 12, "Buy milk; call Sam @ 5").unwrap();
        let name_len = u32::from_le_bytes(p.record[24..28].try_into().unwrap()) as usize;
        let (name, content) = p.record[wire::HEADER..p.length].split_at(name_len);
        assert_eq!(name, b"taylor/my-first-project/note-12.txt");
        assert_eq!(content, b"Buy milk; call Sam @ 5\n");
        assert_eq!(p.job.destination().as_str(), "taylor/my-first-project/note-12.txt");
        assert_eq!(p.job.stage(), Stage::Proposed);
        assert_eq!(prepare(1, 1, "").err(), Some(NOTHING_TYPED));
    }

    #[test]
    fn the_next_note_follows_the_highest_in_the_project() {
        extern crate std;
        let list = |names: &[&str]| {
            let mut out = std::vec::Vec::new();
            for word in [1u32, names.len() as u32, 1, 6] {
                out.extend_from_slice(&word.to_le_bytes());
            }
            out.extend_from_slice(&1u64.to_le_bytes());
            out.extend_from_slice(b"taylor");
            for name in names {
                out.extend_from_slice(&(name.len() as u16).to_le_bytes());
                out.extend_from_slice(&[0, 0, 1, 0, 0, 0]);
                out.extend_from_slice(name.as_bytes());
            }
            out
        };
        assert_eq!(next_number(None, 1), 1);
        assert_eq!(next_number(Some(&list(&[])), 1), 1);
        assert_eq!(next_number(Some(&list(&["taylor/my-first-project/note-1.txt", "taylor/my-first-project/note-12.txt", "taylor/my-first-project/note-3.txt"])), 1), 13);
        // Another project's notes, look-alike names and a list that does not check change nothing.
        assert_eq!(next_number(Some(&list(&["taylor/my-first-project/note-007.txt", "taylor/my-first-project/note-x.txt", "taylor/my-first-project/notes-9.txt", "taylor/other/note-40.txt"])), 2), 2);
        let mut cut = list(&["taylor/my-first-project/note-5.txt"]);
        cut.pop();
        assert_eq!(next_number(Some(&cut), 4), 4);
        // Never below what this run has used, and the largest number does not wrap.
        assert_eq!(next_number(Some(&list(&["taylor/my-first-project/note-2.txt"])), 9), 9);
        // The last number already used, in the list or by this run: there is no next.
        assert_eq!(next_number_in(Some(&list(&["taylor/my-first-project/note-4294967295.txt"])), &usual_project(), Some(1)), None);
        assert_eq!(next_number_in(Some(&list(&["taylor/my-first-project/note-4294967294.txt"])), &usual_project(), Some(1)), Some(u32::MAX));
        assert_eq!(next_number_in(None, &usual_project(), None), None);
        // In another project only that project's notes count, and the note is named inside it.
        let other = ProjectLocation::new(BuildFolder::new("taylor").unwrap(), "other").unwrap();
        assert_eq!(next_number_in(Some(&list(&["taylor/my-first-project/note-9.txt", "taylor/other/note-40.txt"])), &other, Some(1)), Some(41));
        // A folder name so long that the first project's name does not fit: no project, and no other folder.
        let long = BuildFolder::new(core::str::from_utf8(&[b'a'; 116]).unwrap()).unwrap();
        assert!(default_project(long).is_none());
        assert_eq!(prepare_in(other, 1, 41, "x").unwrap().job.destination().as_str(), "taylor/other/note-41.txt");
    }

    #[test]
    fn nothing_is_asked_without_a_folder_a_name_that_fits_and_a_number() {
        let root = BuildFolder::new("taylor").unwrap();
        let long = BuildFolder::new(core::str::from_utf8(&[b'a'; 116]).unwrap()).unwrap();
        // Not allowed, the request limit, and no folder each stop it, in that order, and name no folder.
        assert_eq!(decide(false, true, None, None, None, Some(1)), Err(NOT_ALLOWED));
        assert_eq!(decide(true, true, None, None, None, Some(1)), Err(crate::jobs::FULL));
        assert_eq!(decide(true, false, None, None, None, Some(1)), Err(NO_FOLDER));
        // The list failed at start and is read on the next press: the same call then succeeds.
        let (project, number, next) = decide(true, false, Some(root), None, None, Some(1)).unwrap();
        assert_eq!((project.as_str(), number, next), ("taylor/my-first-project", 1, Some(2)));
        // A folder whose name leaves no room: said, and no other folder stands in.
        assert_eq!(decide(true, false, Some(long), None, None, Some(1)), Err(NO_ROOM_FOR_NAME));
        // The last number there is can be asked for once; after it there is none, and nothing wraps.
        assert_eq!(decide(true, false, Some(root), Some(project), None, Some(u32::MAX)).map(|d| (d.1, d.2)), Ok((u32::MAX, None)));
        assert_eq!(decide(true, false, Some(root), Some(project), None, None), Err(NO_NUMBER));
    }

    #[test]
    fn only_a_matching_receipt_says_made() {
        let mut p = prepare(3, 1, "hello").unwrap();
        p.job.submitted().unwrap();
        let receipt = kernel_receipt(&p, 1, 9);
        let made = settle(&mut p.job, &receipt);
        assert_eq!(made.as_str(), "Made taylor/my-first-project/note-1.txt. Kept until power-off.");
        assert!(made.as_str().len() <= 70);
        assert_eq!(p.job.open_target().map(|d| d.as_str()), Some("taylor/my-first-project/note-1.txt"));
        assert_eq!(headline(&p.job), ("I made your note.", true));
        assert_eq!(headline(&prepare(3, 1, "hello").unwrap().job), ("I cannot tell what happened.", false));

        for (outcome, words) in [(2, "Not made: you said no."), (3, "Not made: that name is already taken."),
                                 (5, "Not made: there is no room left."), (6, "Not made: this display is too small to show it to you.")] {
            let mut p = prepare(3, 1, "hello").unwrap();
            p.job.submitted().unwrap();
            let receipt = kernel_receipt(&p, outcome, 9);
            assert_eq!(settle(&mut p.job, &receipt).as_str(), words);
            assert!(!headline(&p.job).1 && headline(&p.job).0.starts_with("Not made: "));
            assert!(p.job.open_target().is_none());
        }
    }

    #[test]
    fn a_receipt_for_something_else_claims_nothing() {
        let other = prepare(3, 1, "something else").unwrap();
        let mut p = prepare(3, 1, "hello").unwrap();
        p.job.submitted().unwrap();
        // A created receipt for other content, for another attempt, with no run number, cut short, or
        // before the job was handed over: none of them makes this job say "Made".
        assert_eq!(settle(&mut p.job, &kernel_receipt(&other, 1, 9)).as_str(), UNREADABLE);
        let mut wrong_attempt = kernel_receipt(&p, 1, 9);
        wrong_attempt[16] ^= 1;
        assert_eq!(settle(&mut p.job, &wrong_attempt).as_str(), UNREADABLE);
        let (no_run, whole) = (kernel_receipt(&p, 1, 0), kernel_receipt(&p, 1, 9));
        assert_eq!(settle(&mut p.job, &no_run).as_str(), UNREADABLE);
        assert_eq!(settle(&mut p.job, &whole[..60]).as_str(), UNREADABLE);
        assert!(p.job.open_target().is_none());
        let mut early = prepare(3, 1, "hello").unwrap();
        let receipt = kernel_receipt(&early, 1, 9);
        assert_eq!(settle(&mut early.job, &receipt).as_str(), UNREADABLE);
    }
}
