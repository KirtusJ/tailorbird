//! Validated, borrowed version-1 listing of this launch's build folder.
//!
//! The controller reads the actual kernel device through EOF, with a 64 KiB
//! bound, then calls `Snapshot::parse`. Parsing a prefix is not proof of EOF.
//! This module performs no I/O and authenticates no transport. List sizes are
//! hints: Open must verify actual bytes, length and EOF against a result receipt.
use crate::project_location::{BuildFolder, FileDestination, ProjectLocation};

pub const MAX_SNAPSHOT_BYTES: usize = 64 * 1024;
pub const MAX_ENTRIES: usize = 256;
pub const HEADER_BYTES: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotError {
    TooLarge, Truncated, UnsupportedVersion, InvalidCount, InvalidCompleteness,
    InvalidRoot, InvalidLaunch, InvalidName, ReservedField, OutsideBuildFolder,
    UnsortedOrDuplicate, TrailingData, WrongContext,
}

/// The full snapshot is validated before any name is exposed to a consumer.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot<'a> {
    root: BuildFolder,
    launch: u64,
    count: usize,
    complete: bool,
    records: &'a [u8],
}

/// Unsupported names/layouts are counted as other files, not silently hidden.
/// Only `ProjectFile` supplies a typed destination for the project's Open action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    ProjectFile { project: ProjectLocation, destination: FileDestination },
    OtherFile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    /// Raw exact name; other-file names may not be safe to draw as plain labels.
    pub name: &'a str,
    pub size_bytes: u32,
    pub kind: EntryKind,
}

impl<'a> Snapshot<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, SnapshotError> {
        if bytes.len() > MAX_SNAPSHOT_BYTES { return Err(SnapshotError::TooLarge); }
        let mut cursor = Cursor::new(bytes);
        let header = cursor.take(HEADER_BYTES)?;
        let word = |at: usize| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
        if word(0) != 1 { return Err(SnapshotError::UnsupportedVersion); }
        let count = word(4) as usize;
        if count > MAX_ENTRIES { return Err(SnapshotError::InvalidCount); }
        let complete = match word(8) {
            0 => false, 1 => true, _ => return Err(SnapshotError::InvalidCompleteness),
        };
        let launch = u64::from_le_bytes(header[16..24].try_into().unwrap());
        if launch == 0 { return Err(SnapshotError::InvalidLaunch); }
        let root_text = core::str::from_utf8(cursor.take(word(12) as usize)?)
            .map_err(|_| SnapshotError::InvalidRoot)?;
        let root = BuildFolder::new(root_text).map_err(|_| SnapshotError::InvalidRoot)?;
        let records = cursor.remaining();
        let mut previous: Option<&[u8]> = None;
        for _ in 0..count {
            let (name, _) = read_entry(&mut cursor)?;
            // A raw prefix is insufficient: taylor-other must not enter taylor.
            if !name.strip_prefix(root.as_str()).is_some_and(|tail| tail.starts_with('/')) {
                return Err(SnapshotError::OutsideBuildFolder);
            }
            if previous.is_some_and(|old| old >= name.as_bytes()) {
                return Err(SnapshotError::UnsortedOrDuplicate);
            }
            previous = Some(name.as_bytes());
        }
        if !cursor.remaining().is_empty() { return Err(SnapshotError::TrailingData); }
        Ok(Self { root, launch, count, complete, records })
    }

    /// After the first snapshot, a refresh must retain the current launch/root.
    /// A next-launch setting cannot silently retarget the current browser.
    pub fn parse_for(bytes: &'a [u8], root: BuildFolder, launch: u64) -> Result<Self, SnapshotError> {
        let snapshot = Self::parse(bytes)?;
        if snapshot.root != root || snapshot.launch != launch { return Err(SnapshotError::WrongContext); }
        Ok(snapshot)
    }

    pub fn root(&self) -> BuildFolder { self.root }
    pub fn launch(&self) -> u64 { self.launch }
    pub fn entry_count(&self) -> usize { self.count }
    pub fn is_complete(&self) -> bool { self.complete }
    pub fn is_known_empty(&self) -> bool { self.complete && self.count == 0 }
    pub fn entries(&self) -> Entries<'a> {
        Entries { root: self.root, cursor: Cursor::new(self.records), remaining: self.count }
    }

    /// Counts files represented by project locations separately from other files.
    pub fn file_counts(&self) -> (usize, usize) {
        let mut project_files = 0;
        for entry in self.entries() {
            if matches!(entry.kind, EntryKind::ProjectFile { .. }) { project_files += 1; }
        }
        (project_files, self.count - project_files)
    }
}

pub struct Entries<'a> {
    root: BuildFolder,
    cursor: Cursor<'a>,
    remaining: usize,
}

impl<'a> Iterator for Entries<'a> {
    type Item = Entry<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 { return None; }
        // Snapshot validated all records. The borrowed buffer cannot be changed
        // while these references exist, so iteration cannot lose a later error.
        let (name, size_bytes) = read_entry(&mut self.cursor).expect("validated snapshot record");
        self.remaining -= 1;
        let kind = match self.root.parse_project_file(name) {
            Ok((project, destination)) => EntryKind::ProjectFile { project, destination },
            Err(_) => EntryKind::OtherFile,
        };
        Some(Entry { name, size_bytes, kind })
    }
    fn size_hint(&self) -> (usize, Option<usize>) { (self.remaining, Some(self.remaining)) }
}
impl ExactSizeIterator for Entries<'_> {}

struct Cursor<'a> { bytes: &'a [u8], at: usize }
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self { Self { bytes, at: 0 } }
    fn remaining(&self) -> &'a [u8] { &self.bytes[self.at..] }
    fn take(&mut self, count: usize) -> Result<&'a [u8], SnapshotError> {
        if count > self.bytes.len() - self.at { return Err(SnapshotError::Truncated); }
        let start = self.at;
        self.at += count;
        Ok(&self.bytes[start..self.at])
    }
}

fn read_entry<'a>(cursor: &mut Cursor<'a>) -> Result<(&'a str, u32), SnapshotError> {
    let header = cursor.take(8)?;
    let length = u16::from_le_bytes([header[0], header[1]]) as usize;
    if length == 0 { return Err(SnapshotError::InvalidName); }
    if header[2..4] != [0, 0] { return Err(SnapshotError::ReservedField); }
    let size = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let name = core::str::from_utf8(cursor.take(length)?).map_err(|_| SnapshotError::InvalidName)?;
    Ok((name, size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{format, vec, vec::Vec};

    fn wire(root: &str, launch: u64, complete: u32, entries: &[(&str, u32)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for word in [1, entries.len() as u32, complete, root.len() as u32] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes.extend_from_slice(&launch.to_le_bytes());
        bytes.extend_from_slice(root.as_bytes());
        for (name, size) in entries {
            bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(&size.to_le_bytes());
            bytes.extend_from_slice(name.as_bytes());
        }
        bytes
    }

    #[test]
    fn current_context_and_exact_project_destinations_survive_decoding() {
        for root in ["taylor", "work/taylor"] {
            let first = format!("{root}/garden/a.txt");
            let second = format!("{root}/garden/b.txt");
            let bytes = wire(root, u64::MAX, 1, &[(&first, 0), (&second, 16384)]);
            let snapshot = Snapshot::parse(&bytes).unwrap();
            assert_eq!(snapshot.root().as_str(), root);
            assert_eq!(snapshot.launch(), u64::MAX);
            assert!(snapshot.is_complete() && !snapshot.is_known_empty());
            assert_eq!(snapshot.entry_count(), 2);
            assert_eq!(snapshot.file_counts(), (2, 0));
            let entries: Vec<_> = snapshot.entries().collect();
            assert_eq!((entries[0].name, entries[0].size_bytes), (first.as_str(), 0));
            assert_eq!((entries[1].name, entries[1].size_bytes), (second.as_str(), 16384));
            match entries[1].kind {
                EntryKind::ProjectFile { project, destination } => {
                    assert_eq!(project.build_folder(), root);
                    assert_eq!(project.project_name(), "garden");
                    assert_eq!(destination.as_str(), second);
                }
                _ => panic!("supported project file lost"),
            }
            assert!(Snapshot::parse_for(&bytes, snapshot.root(), u64::MAX).is_ok());
            assert_eq!(Snapshot::parse_for(&bytes, BuildFolder::new("other").unwrap(), u64::MAX).err(), Some(SnapshotError::WrongContext));
            assert_eq!(Snapshot::parse_for(&bytes, snapshot.root(), 1).err(), Some(SnapshotError::WrongContext));
        }
    }

    #[test]
    fn empty_incomplete_and_other_files_are_distinct() {
        let empty = wire("taylor", 1, 1, &[]);
        assert!(Snapshot::parse(&empty).unwrap().is_known_empty());
        let partial = wire("taylor", 1, 0, &[]);
        let partial = Snapshot::parse(&partial).unwrap();
        assert!(!partial.is_complete() && !partial.is_known_empty());
        assert_eq!(partial.root().as_str(), "taylor");
        let other = wire("taylor", 1, 1, &[("taylor/loose.txt", 1), ("taylor/p/deeper/file.txt", 2), ("taylor/p/\u{e9}.txt", 3)]);
        let other = Snapshot::parse(&other).unwrap();
        assert_eq!(other.file_counts(), (0, 3));
        assert!(!other.is_known_empty());
        assert!(other.entries().all(|entry| entry.kind == EntryKind::OtherFile));
    }

    #[test]
    fn no_truncated_or_extra_record_can_produce_a_partial_success() {
        let bytes = wire("taylor", 1, 1, &[("taylor/p/a", 1), ("taylor/p/b", 2)]);
        for end in 0..bytes.len() { assert!(Snapshot::parse(&bytes[..end]).is_err(), "prefix {end}"); }
        let mut extra = bytes.clone(); extra.push(0);
        assert_eq!(Snapshot::parse(&extra).err(), Some(SnapshotError::TrailingData));
        let mut hidden = bytes.clone(); hidden[4..8].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(Snapshot::parse(&hidden).err(), Some(SnapshotError::TrailingData));
    }

    #[test]
    fn invalid_headers_are_refused_including_empty_bad_contexts() {
        let bytes = wire("taylor", 1, 1, &[]);
        for (offset, value, error) in [(0, 2, SnapshotError::UnsupportedVersion),
            (4, 257, SnapshotError::InvalidCount), (8, 2, SnapshotError::InvalidCompleteness),
            (12, u32::MAX, SnapshotError::Truncated)] {
            let mut bad = bytes.clone(); bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert_eq!(Snapshot::parse(&bad).err(), Some(error));
        }
        assert_eq!(Snapshot::parse(&wire("taylor", 0, 1, &[])).err(), Some(SnapshotError::InvalidLaunch));
        for root in ["", "dev", "taylor/", "taylor/../other", "a/b/c", "Taylor"] {
            assert_eq!(Snapshot::parse(&wire(root, 1, 1, &[])).err(), Some(SnapshotError::InvalidRoot));
        }
    }

    #[test]
    fn malformed_entries_cannot_be_treated_as_other_files() {
        let bytes = wire("taylor", 1, 1, &[("taylor/p/a", 1)]);
        let at = HEADER_BYTES + "taylor".len();
        let mut reserved = bytes.clone(); reserved[at + 2] = 1;
        assert_eq!(Snapshot::parse(&reserved).err(), Some(SnapshotError::ReservedField));
        let mut bad_utf8 = bytes.clone(); *bad_utf8.last_mut().unwrap() = 0xff;
        assert_eq!(Snapshot::parse(&bad_utf8).err(), Some(SnapshotError::InvalidName));
        assert_eq!(Snapshot::parse(&wire("taylor", 1, 1, &[("", 0)])).err(), Some(SnapshotError::InvalidName));
        for name in ["taylor", "taylor2/p/a", "taylor-old/p/a", "/taylor/p/a", "other/p/a"] {
            assert_eq!(Snapshot::parse(&wire("taylor", 1, 1, &[(name, 1)])).err(), Some(SnapshotError::OutsideBuildFolder));
        }
    }

    #[test]
    fn duplicates_or_unsorted_names_are_refused_before_iteration() {
        for names in [[("taylor/p/a", 1), ("taylor/p/a", 2)], [("taylor/p/b", 1), ("taylor/p/a", 2)]] {
            assert_eq!(Snapshot::parse(&wire("taylor", 1, 1, &names)).err(), Some(SnapshotError::UnsortedOrDuplicate));
        }
    }

    #[test]
    fn entry_and_byte_caps_are_independent_and_never_truncate() {
        let names: Vec<_> = (0..256).map(|n| format!("taylor/p/n{n:03}")).collect();
        let entries: Vec<_> = names.iter().map(|n| (n.as_str(), 1)).collect();
        let bytes = wire("taylor", 1, 0, &entries);
        let snapshot = Snapshot::parse(&bytes).unwrap();
        assert_eq!(snapshot.entries().count(), 256);
        assert!(!snapshot.is_complete());
        let size = MAX_SNAPSHOT_BYTES - HEADER_BYTES - "taylor".len() - 8;
        let name = format!("taylor/{}", "x".repeat(size - 7));
        let maximum = wire("taylor", 1, 0, &[(&name, u32::MAX)]);
        assert_eq!(maximum.len(), MAX_SNAPSHOT_BYTES);
        assert_eq!(Snapshot::parse(&maximum).unwrap().file_counts(), (0, 1));
        let too_large = vec![0; MAX_SNAPSHOT_BYTES + 1];
        assert_eq!(Snapshot::parse(&too_large).err(), Some(SnapshotError::TooLarge));
    }

    #[test]
    fn a_maximal_destination_stays_exact_in_the_listing() {
        let root = "a".repeat(116);
        let name = format!("{root}/p/z");
        let bytes = wire(&root, 1, 1, &[(&name, 3)]);
        let entry = Snapshot::parse(&bytes).unwrap().entries().next().unwrap();
        match entry.kind {
            EntryKind::ProjectFile { destination, .. } => assert_eq!(destination.as_str(), name),
            _ => panic!("120-byte destination lost"),
        }
    }

    #[test]
    fn single_byte_corruption_never_panics_or_exposes_an_unchecked_suffix() {
        let bytes = wire("taylor", 9, 1, &[("taylor/p/a", 3), ("taylor/p/b", 4)]);
        for index in 0..bytes.len() {
            for value in [0, 0xff] {
                let mut changed = bytes.clone(); changed[index] = value;
                if let Ok(snapshot) = Snapshot::parse(&changed) {
                    assert_eq!(snapshot.entries().count(), snapshot.entry_count());
                }
            }
        }
    }
}
