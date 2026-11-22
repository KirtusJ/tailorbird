//! What the project list shows: a summary of one kernel snapshot of the build folder, held by value so
//! the screen is drawn from it and never from memory the next read will overwrite. No I/O here: the
//! program reads the device to its end, closes it, and hands the bytes (or the fact there were none) in.
use crate::build_folder_snapshot::{EntryKind, Snapshot};
use crate::project_location::{BuildFolder, FileDestination, ProjectLocation};
use crate::Text;

/// Rows of a list the screen has room for beside the folder's whole name and what is not shown.
pub const ROWS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listing {
    /// Nothing has been read yet.
    NotAsked,
    /// PCP OS does not offer this launch a list (no grant, or an older PCP OS).
    Unavailable,
    /// Something was read and it is not a list this program can trust; nothing from it is shown.
    Unreadable,
    Listed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub project: Text<116>,
    /// The exact location the kernel's list named; what opening the row uses.
    pub location: Option<ProjectLocation>,
    pub files: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Projects {
    pub state: Listing,
    /// False when PCP OS said there is more than it listed: then nothing here is "all".
    pub complete: bool,
    pub folder: Text<116>,
    pub launch: u64,
    pub projects: usize,
    pub files: usize,
    /// Files under the folder that are not a file directly inside a project. Counted, not hidden.
    pub other: usize,
    pub rows: [Row; ROWS],
    pub shown: usize,
}

impl Projects {
    pub const fn new() -> Self {
        Self { state: Listing::NotAsked, complete: false, folder: Text::new(), launch: 0, projects: 0, files: 0, other: 0,
               rows: [Row { project: Text::new(), location: None, files: 0 }; ROWS], shown: 0 }
    }

    pub const fn unavailable() -> Self {
        let mut p = Self::new();
        p.state = Listing::Unavailable;
        p
    }

    pub const fn unreadable() -> Self {
        let mut p = Self::new();
        p.state = Listing::Unreadable;
        p
    }

    /// The whole of what one open of the list returned. `expected` is the folder and run an earlier list
    /// of this launch named: a list for another folder or run is not a newer version of the same thing.
    pub fn read(bytes: &[u8], expected: Option<(BuildFolder, u64)>) -> Self {
        let parsed = match expected {
            Some((root, launch)) => Snapshot::parse_for(bytes, root, launch),
            None => Snapshot::parse(bytes),
        };
        let Ok(snapshot) = parsed else { return Self::unreadable() };
        let mut out = Self::new();
        out.state = Listing::Listed;
        out.complete = snapshot.is_complete();
        out.folder = Text::display(snapshot.root().as_str());
        out.launch = snapshot.launch();
        // The list is in byte order, so one project's files are together.
        let mut current: Option<ProjectLocation> = None;
        for entry in snapshot.entries() {
            match entry.kind {
                EntryKind::OtherFile => out.other += 1,
                EntryKind::ProjectFile { project, .. } => {
                    out.files += 1;
                    if current != Some(project) {
                        current = Some(project);
                        out.projects += 1;
                        if out.shown < ROWS {
                            out.rows[out.shown] = Row { project: Text::display(project.project_name()), location: Some(project), files: 0 };
                            out.shown += 1;
                        }
                    }
                    if out.projects == out.shown {
                        out.rows[out.shown - 1].files += 1;
                    }
                }
            }
        }
        out
    }

    /// Only a whole list with nothing in it says there are no projects.
    pub fn known_empty(&self) -> bool {
        self.state == Listing::Listed && self.complete && self.projects == 0 && self.other == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileRow {
    pub name: Text<120>,
    pub destination: Option<FileDestination>,
    /// The size the list gave: a hint from when the list was taken, not a promise about the file now.
    pub size: u32,
}

/// The files of one project, from one snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Files {
    pub state: Listing,
    pub complete: bool,
    pub project: Text<116>,
    pub count: usize,
    pub rows: [FileRow; ROWS],
    pub shown: usize,
}

impl Files {
    pub const fn new() -> Self {
        Self { state: Listing::NotAsked, complete: false, project: Text::new(), count: 0,
               rows: [FileRow { name: Text::new(), destination: None, size: 0 }; ROWS], shown: 0 }
    }

    pub const fn failed(state: Listing) -> Self {
        let mut f = Self::new();
        f.state = state;
        f
    }

    pub fn read(bytes: &[u8], expected: Option<(BuildFolder, u64)>, project: ProjectLocation) -> Self {
        let parsed = match expected {
            Some((root, launch)) => Snapshot::parse_for(bytes, root, launch),
            None => Snapshot::parse(bytes),
        };
        let Ok(snapshot) = parsed else { return Self::failed(Listing::Unreadable) };
        let mut out = Self::new();
        out.state = Listing::Listed;
        out.complete = snapshot.is_complete();
        out.project = Text::display(project.project_name());
        for entry in snapshot.entries() {
            if let EntryKind::ProjectFile { project: p, destination } = entry.kind {
                if p == project {
                    out.count += 1;
                    if out.shown < ROWS {
                        out.rows[out.shown] = FileRow { name: Text::display(destination.file_name()), destination: Some(destination), size: entry.size_bytes };
                        out.shown += 1;
                    }
                }
            }
        }
        out
    }
}

pub const VIEW_LINES: usize = 9;
pub const VIEW_ACROSS: usize = 92;
/// The most one file of text may hold: what PCP OS lets a program propose.
pub const VIEW_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opened {
    NotAsked,
    /// The file could not be opened or read to its end. Nothing about its content is known.
    Unavailable,
    /// Larger than a text file Taylor shows.
    TooLarge,
    /// It holds something other than plain letters, digits, punctuation, spaces and new lines.
    NotText,
    Shown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checked {
    /// The bytes read are exactly those PCP OS's receipt of this run names.
    Matches,
    /// PCP OS's receipt of this run names other bytes: the file is not what was made.
    Differs,
    /// Not made in this run, so there is no receipt to compare it with.
    NotThisRun,
}

/// One file, read whole and shown read-only: its first lines, exactly, and what is known about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewed {
    pub state: Opened,
    pub destination: Option<FileDestination>,
    pub bytes: usize,
    pub lines: [Text<VIEW_ACROSS>; VIEW_LINES],
    pub shown: usize,
    /// There is more of the file than the lines shown.
    pub more: bool,
    pub checked: Checked,
}

impl Viewed {
    pub const fn new() -> Self {
        Self { state: Opened::NotAsked, destination: None, bytes: 0, lines: [Text::new(); VIEW_LINES], shown: 0, more: false, checked: Checked::NotThisRun }
    }

    pub fn failed(destination: FileDestination, state: Opened) -> Self {
        let mut v = Self::new();
        v.destination = Some(destination);
        v.state = state;
        v
    }

    /// The whole file as read to its end. Long lines continue on the next row; nothing is replaced.
    pub fn read(destination: FileDestination, content: &[u8], checked: Checked) -> Self {
        if content.len() > VIEW_BYTES {
            return Self::failed(destination, Opened::TooLarge);
        }
        if !content.iter().all(|b| (0x20..0x7f).contains(b) || *b == b'\n') {
            let mut v = Self::failed(destination, Opened::NotText);
            v.bytes = content.len();
            v.checked = checked;
            return v;
        }
        let mut v = Self::failed(destination, Opened::Shown);
        v.bytes = content.len();
        v.checked = checked;
        let text = content.strip_suffix(b"\n").unwrap_or(content);
        if content.is_empty() {
            return v;
        }
        for line in text.split(|b| *b == b'\n') {
            let mut pieces = line.chunks(VIEW_ACROSS);
            let mut piece = pieces.next().unwrap_or(&[]);
            loop {
                if v.shown == VIEW_LINES {
                    v.more = true;
                    return v;
                }
                for b in piece {
                    let _ = v.lines[v.shown].push_ascii(*b);
                }
                v.shown += 1;
                match pieces.next() {
                    Some(next) => piece = next,
                    None => break,
                }
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec;

    fn snapshot(complete: u32, launch: u64, names: &[(&str, u32)]) -> Vec<u8> {
        let mut out = Vec::new();
        for word in [1u32, names.len() as u32, complete, 6] {
            out.extend_from_slice(&word.to_le_bytes());
        }
        out.extend_from_slice(&launch.to_le_bytes());
        out.extend_from_slice(b"taylor");
        for (name, size) in names {
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
        }
        out
    }

    #[test]
    fn the_kernels_own_bytes_are_one_project_with_one_file() {
        // The list a native run of the kernel's check produced (pcp-os 9a9e372, case "allowed").
        let hex = "0100000001000000010000000600000001000000000000007461796c6f721d0000003a0000007461796c6f722f66697273742d70726f6a6563742f6e6f74652e747874";
        let bytes: Vec<u8> = (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap()).collect();
        let p = Projects::read(&bytes, None);
        assert_eq!((p.state, p.complete, p.folder.as_str(), p.launch), (Listing::Listed, true, "taylor", 1));
        assert_eq!((p.projects, p.files, p.other, p.shown), (1, 1, 0, 1));
        assert_eq!((p.rows[0].project.as_str(), p.rows[0].files), ("first-project", 1));
        assert!(!p.known_empty());
    }

    #[test]
    fn projects_are_grouped_and_other_files_counted() {
        let bytes = snapshot(1, 4, &[("taylor/a/one.txt", 1), ("taylor/a/two.txt", 2), ("taylor/b/x.txt", 3), ("taylor/loose.txt", 4)]);
        let p = Projects::read(&bytes, None);
        assert_eq!((p.projects, p.files, p.other, p.shown), (2, 3, 1, 2));
        assert_eq!((p.rows[0].project.as_str(), p.rows[0].files), ("a", 2));
        assert_eq!((p.rows[1].project.as_str(), p.rows[1].files), ("b", 1));
    }

    #[test]
    fn empty_is_said_only_of_a_whole_readable_list() {
        assert!(Projects::read(&snapshot(1, 4, &[]), None).known_empty());
        assert!(!Projects::read(&snapshot(0, 4, &[]), None).known_empty());
        assert!(!Projects::unavailable().known_empty());
        assert!(!Projects::new().known_empty());
        let mut cut = snapshot(1, 4, &[("taylor/a/one.txt", 1)]);
        cut.pop();
        assert_eq!(Projects::read(&cut, None).state, Listing::Unreadable);
        assert!(!Projects::read(&cut, None).known_empty());
    }

    #[test]
    fn more_projects_than_rows_are_counted_not_dropped() {
        assert!(ROWS < 10);
        let names: Vec<std::string::String> = (0..10).map(|i| std::format!("taylor/p{}/f.txt", i)).collect();
        let listed: Vec<(&str, u32)> = names.iter().map(|n| (n.as_str(), 1)).collect();
        let p = Projects::read(&snapshot(1, 4, &listed), None);
        assert_eq!((p.projects, p.shown, p.files), (10, ROWS, 10));
        assert!(p.rows.iter().all(|r| r.files == 1));
    }

    #[test]
    fn the_files_of_one_project_and_no_other() {
        let bytes = snapshot(1, 4, &[("taylor/a/one.txt", 1), ("taylor/a/two.txt", 2), ("taylor/ab/x.txt", 3), ("taylor/loose.txt", 4)]);
        let p = Projects::read(&bytes, None);
        let f = Files::read(&bytes, None, p.rows[0].location.unwrap());
        assert_eq!((f.state, f.project.as_str(), f.count, f.shown), (Listing::Listed, "a", 2, 2));
        assert_eq!((f.rows[0].name.as_str(), f.rows[0].size), ("one.txt", 1));
        assert_eq!(f.rows[1].destination.unwrap().as_str(), "taylor/a/two.txt");
        assert_eq!(Files::read(&bytes[..bytes.len() - 1], None, p.rows[0].location.unwrap()).state, Listing::Unreadable);
    }

    #[test]
    fn a_file_is_shown_exactly_or_not_at_all() {
        let root = BuildFolder::new("taylor").unwrap();
        let d = ProjectLocation::new(root, "a").unwrap().file("one.txt").unwrap();
        let v = Viewed::read(d, b"Hello.\n\nSecond  line \n", Checked::Matches);
        assert_eq!((v.state, v.bytes, v.shown, v.more), (Opened::Shown, 22, 3, false));
        assert_eq!([v.lines[0].as_str(), v.lines[1].as_str(), v.lines[2].as_str()], ["Hello.", "", "Second  line "]);
        // A long line continues on the next row with nothing lost; more lines than fit are said to exist.
        let long = [b'x'; VIEW_ACROSS + 3];
        let v = Viewed::read(d, &long, Checked::NotThisRun);
        assert_eq!((v.shown, v.lines[0].as_str().len(), v.lines[1].as_str()), (2, VIEW_ACROSS, "xxx"));
        let many = [b'\n'; VIEW_LINES + 5];
        assert!(Viewed::read(d, &many, Checked::NotThisRun).more);
        assert_eq!(Viewed::read(d, b"", Checked::NotThisRun).shown, 0);
        // A tab, a byte above ASCII, or too much: no line is shown.
        for bad in [&b"a\tb"[..], &[0xc3, 0xa9][..]] {
            let v = Viewed::read(d, bad, Checked::NotThisRun);
            assert_eq!((v.state, v.shown), (Opened::NotText, 0));
        }
        let big = std::vec![b'a'; VIEW_BYTES + 1];
        assert_eq!(Viewed::read(d, &big, Checked::NotThisRun).state, Opened::TooLarge);
    }

    #[test]
    fn a_list_for_another_run_or_folder_is_not_taken() {
        let first = Projects::read(&snapshot(1, 4, &[]), None);
        let root = BuildFolder::new(first.folder.as_str()).unwrap();
        assert_eq!(Projects::read(&snapshot(1, 5, &[]), Some((root, first.launch))).state, Listing::Unreadable);
        assert_eq!(Projects::read(&snapshot(1, 4, &[]), Some((root, first.launch))).state, Listing::Listed);
    }
}
