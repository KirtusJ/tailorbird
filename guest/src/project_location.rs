//! Exact locations for the first project-folder prototype.
//!
//! A selected project owns its build-folder snapshot. Changing the default for
//! future projects cannot retarget that project or an already resolved file.
//! Pass the same `FileDestination` to preview and future request construction;
//! do not reconstruct a result's location from the current default folder.
//!
//! These are names in PCP OS, not host paths. Limits match the naming subset in
//! pcp-os #344 at e612f20: 120 bytes, at most three folders, lowercase ASCII,
//! digits, dot, underscore and hyphen; no component starts with a dot. A root
//! may have two components, leaving one folder for the project and one file.
//! Names are rejected, never trimmed, case-folded, truncated or transliterated.
//!
//! This module has no I/O and grants no authority. It cannot establish folder
//! availability, uniqueness, persistence, absence of a file or successful
//! creation. The kernel must validate and authorize the exact destination and
//! content; only its matching receipt may confirm a result. Settings, project
//! browsing, kernel namespace operations and receipt transport are separate.

pub const MAX_PATH_BYTES: usize = 120;
pub const DEFAULT_BUILD_FOLDER: &str = "taylor";
// Reserve /<project>/<file>, with each leaf at least one byte.
const MAX_ROOT_BYTES: usize = MAX_PATH_BYTES - 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationError {
    Empty,
    UnsupportedName,
    ReservedRoot,
    TooDeep,
    TooLong,
    OutsideBuildFolder,
    NotProjectFile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExactPath {
    bytes: [u8; MAX_PATH_BYTES],
    len: usize,
}

impl ExactPath {
    // Callers have checked ASCII syntax and the capacity. Keep unused bytes
    // zero so value equality compares the exact path, not construction history.
    fn validated(source: &str) -> Self {
        let mut path = Self { bytes: [0; MAX_PATH_BYTES], len: source.len() };
        path.bytes[..source.len()].copy_from_slice(source.as_bytes());
        path
    }

    fn join(&self, component: &str, reserve: usize) -> Result<Self, LocationError> {
        check_component(component)?;
        let len = self.len + 1 + component.len();
        if len > MAX_PATH_BYTES - reserve {
            return Err(LocationError::TooLong);
        }
        let mut path = *self;
        path.bytes[self.len] = b'/';
        path.bytes[self.len + 1..len].copy_from_slice(component.as_bytes());
        path.len = len;
        Ok(path)
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).expect("validated ASCII path")
    }
}

fn check_component(name: &str) -> Result<(), LocationError> {
    if name.is_empty() {
        return Err(LocationError::Empty);
    }
    if name.len() > MAX_PATH_BYTES {
        return Err(LocationError::TooLong);
    }
    if name.starts_with('.') || !name.bytes().all(|b|
        b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(LocationError::UnsupportedName);
    }
    Ok(())
}

/// A configurable default for future projects, expressed in the PCP OS namespace.
/// Parsing a folder name neither creates it nor proves that it can be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildFolder(ExactPath);

impl BuildFolder {
    pub fn new(folder: &str) -> Result<Self, LocationError> {
        if folder.len() > MAX_ROOT_BYTES {
            return Err(LocationError::TooLong);
        }
        let mut parts = folder.split('/');
        let first = parts.next().ok_or(LocationError::Empty)?;
        check_component(first)?;
        if first == "dev" {
            return Err(LocationError::ReservedRoot);
        }
        if let Some(second) = parts.next() {
            check_component(second)?;
        }
        if parts.next().is_some() {
            return Err(LocationError::TooDeep);
        }
        Ok(Self(ExactPath::validated(folder)))
    }

    pub fn as_str(&self) -> &str { self.0.as_str() }

    /// Parse an exact file name from a listing under this launch's build folder.
    /// A file belongs to one project directly below the root. Root-level files
    /// or deeper layouts return `NotProjectFile`; a browser must not turn an
    /// unsupported or incomplete listing into a claim that the folder is empty.
    /// This parses names only: it does not list, open, authorize or verify a file.
    pub fn parse_project_file(&self, path: &str) -> Result<(ProjectLocation, FileDestination), LocationError> {
        if path.len() > MAX_PATH_BYTES { return Err(LocationError::TooLong); }
        let relative = path.strip_prefix(self.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
            .ok_or(LocationError::OutsideBuildFolder)?;
        let (project_name, file_name) = relative.split_once('/')
            .ok_or(LocationError::NotProjectFile)?;
        if file_name.contains('/') { return Err(LocationError::NotProjectFile); }
        let project = ProjectLocation::new(*self, project_name)?;
        let destination = project.file(file_name)?;
        Ok((project, destination))
    }
}

impl Default for BuildFolder {
    fn default() -> Self {
        Self::new(DEFAULT_BUILD_FOLDER).expect("valid default build folder")
    }
}

/// A stable selected location, independent of later default-folder changes.
/// Equal project names under different build folders are different locations.
/// This is not a persistent project ID or a reservation of its folder name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectLocation {
    path: ExactPath,
    root_len: usize,
}

impl ProjectLocation {
    pub fn new(root: BuildFolder, project_name: &str) -> Result<Self, LocationError> {
        Ok(Self {
            // Leave room for the slash and at least one filename byte.
            path: root.0.join(project_name, 2)?,
            root_len: root.0.len,
        })
    }

    pub fn as_str(&self) -> &str { self.path.as_str() }
    pub fn build_folder(&self) -> &str { &self.as_str()[..self.root_len] }
    pub fn project_name(&self) -> &str { &self.as_str()[self.root_len + 1..] }

    /// Accept only one filename component. A suggested name cannot supply a
    /// different root, another project, an absolute path or a parent traversal.
    pub fn file(&self, file_name: &str) -> Result<FileDestination, LocationError> {
        Ok(FileDestination {
            path: self.path.join(file_name, 0)?,
            project_len: self.path.len,
        })
    }
}

/// Immutable exact target data for preview, a future proposal and result lookup.
/// It is not a kernel receipt, and constructing it does not mean a file exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileDestination {
    path: ExactPath,
    project_len: usize,
}

impl FileDestination {
    pub fn as_str(&self) -> &str { self.path.as_str() }
    pub fn project_folder(&self) -> &str { &self.as_str()[..self.project_len] }
    pub fn file_name(&self) -> &str { &self.as_str()[self.project_len + 1..] }
}

#[cfg(test)]
mod listing_tests {
    use super::*;

    #[test]
    fn listed_file_retains_its_project_and_exact_open_target() {
        for folder in ["taylor", "work/taylor"] {
            let root = BuildFolder::new(folder).unwrap();
            let path = std::format!("{folder}/garden/note-1.txt");
            let (project, destination) = root.parse_project_file(&path).unwrap();
            assert_eq!(project.build_folder(), folder);
            assert_eq!(project.project_name(), "garden");
            assert_eq!(destination.project_folder(), project.as_str());
            assert_eq!(destination.file_name(), "note-1.txt");
            assert_eq!(destination.as_str(), path);
            let new_default = BuildFolder::new("elsewhere").unwrap();
            assert_eq!(new_default.parse_project_file(&path), Err(LocationError::OutsideBuildFolder));
            assert_eq!(destination.as_str(), path);
        }
    }

    #[test]
    fn raw_prefixes_and_host_paths_are_not_inside_the_selected_folder() {
        let root = BuildFolder::default();
        for path in ["taylor", "taylor-old/garden/note.txt", "taylor2/garden/note.txt",
                     "/taylor/garden/note.txt", "other/taylor/garden/note.txt",
                     "taylor\\garden\\note.txt", "D:/taylor/garden/note.txt"] {
            assert_eq!(root.parse_project_file(path), Err(LocationError::OutsideBuildFolder), "{path}");
        }
        let nested = BuildFolder::new("work/taylor").unwrap();
        assert_eq!(nested.parse_project_file("work/taylor-old/garden/note.txt"), Err(LocationError::OutsideBuildFolder));
        let (a, _) = root.parse_project_file("taylor/garden/note.txt").unwrap();
        let (b, _) = root.parse_project_file("taylor/garden-next/note.txt").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn unsupported_layouts_and_names_are_reported_without_repair() {
        let root = BuildFolder::default();
        for path in ["taylor/note.txt", "taylor/garden/sub/note.txt", "taylor/garden/sub/"] {
            assert_eq!(root.parse_project_file(path), Err(LocationError::NotProjectFile));
        }
        for path in ["taylor//note.txt", "taylor/garden/", "taylor/../note.txt",
                     "taylor/garden/..", "taylor/Garden/note.txt", "taylor/garden/bad name.txt",
                     "taylor/garden/note.txt ", "taylor/garden/caf\u{e9}.txt", "taylor/garden/note\0.txt"] {
            assert!(root.parse_project_file(path).is_err(), "{path}");
        }
    }

    #[test]
    fn full_length_listing_targets_are_not_clipped_or_aliased() {
        let folder = "a".repeat(116);
        let root = BuildFolder::new(&folder).unwrap();
        let path = std::format!("{folder}/p/z");
        assert_eq!(path.len(), MAX_PATH_BYTES);
        let (_, target) = root.parse_project_file(&path).unwrap();
        assert_eq!(target.as_str(), path);
        let (_, other) = root.parse_project_file(&std::format!("{folder}/p/y")).unwrap();
        assert_ne!(target, other);
        assert_eq!(root.parse_project_file(&std::format!("{path}x")), Err(LocationError::TooLong));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_custom_folder_keep_the_entire_location() {
        assert_eq!(BuildFolder::default().as_str(), "taylor");
        let root = BuildFolder::new("work/taylor").unwrap();
        let project = ProjectLocation::new(root, "birthday-card").unwrap();
        let file = project.file("card.txt").unwrap();
        assert_eq!(project.build_folder(), "work/taylor");
        assert_eq!(project.project_name(), "birthday-card");
        assert_eq!(file.project_folder(), project.as_str());
        assert_eq!(file.file_name(), "card.txt");
        assert_eq!(file.as_str(), "work/taylor/birthday-card/card.txt");
    }

    #[test]
    fn changing_default_does_not_move_selected_project_or_pending_file() {
        let mut default = BuildFolder::default();
        let selected = ProjectLocation::new(default, "card").unwrap();
        let pending = selected.file("note.txt").unwrap();
        default = BuildFolder::new("work").unwrap();
        let future = ProjectLocation::new(default, "card").unwrap();
        assert_ne!(selected, future);
        assert_eq!(selected.as_str(), "taylor/card");
        assert_eq!(pending.as_str(), "taylor/card/note.txt");
        assert_eq!(selected.file("note.txt").unwrap(), pending);
        assert_ne!(future.file("note.txt").unwrap(), pending);
    }

    #[test]
    fn model_suggested_filename_cannot_switch_root_or_project() {
        let project = ProjectLocation::new(BuildFolder::default(), "one").unwrap();
        for name in ["../other.txt", "/other.txt", "two/other.txt", "taylor/two/note.txt",
                     "dev/proposal", "a//b", "a\\b", "C:\\temp\\a", "..", ".", ""] {
            assert!(project.file(name).is_err(), "accepted {name:?}");
            assert!(ProjectLocation::new(BuildFolder::default(), name).is_err());
        }
        assert_eq!(project.file("other.txt").unwrap().as_str(), "taylor/one/other.txt");
    }

    #[test]
    fn roots_reject_ambiguous_reserved_or_unsupported_paths() {
        for root in ["", "/taylor", "taylor/", "taylor//x", ".", "..", "a/../b",
                     ".hidden", "a/.hidden", "dev", "dev/work", "a/b/c",
                     "C:\\C3", "C:/C3", "taylor ", "Taylor", "caf\u{e9}"] {
            assert!(BuildFolder::new(root).is_err(), "accepted {root:?}");
        }
        assert_eq!(BuildFolder::new("dev"), Err(LocationError::ReservedRoot));
        assert_eq!(BuildFolder::new("a/b/c"), Err(LocationError::TooDeep));
        assert!(BuildFolder::new("device").is_ok());
        assert!(BuildFolder::new("a/dev").is_ok());
    }

    #[test]
    fn names_are_exact_or_rejected_never_repaired() {
        let project = ProjectLocation::new(BuildFolder::default(), "my-project_2").unwrap();
        for name in ["Note.txt", " note.txt", "note.txt ", "caf\u{e9}.txt", "a\n.txt",
                     "a\r.txt", "a\t.txt", "a\0.txt", ".note", "a\u{202e}.txt"] {
            assert!(project.file(name).is_err(), "accepted {name:?}");
        }
        let name = "note.._2-.txt";
        assert_eq!(project.file(name).unwrap().file_name(), name);
    }

    #[test]
    fn every_ascii_byte_is_checked_in_each_name_role() {
        for byte in 0u8..=127 {
            let name = std::format!("a{}b", byte as char);
            let valid = byte.is_ascii_lowercase() || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-');
            let project = ProjectLocation::new(BuildFolder::default(), "project").unwrap();
            assert_eq!(project.file(&name).is_ok(), valid, "filename byte {byte}");
            assert_eq!(ProjectLocation::new(BuildFolder::default(), &name).is_ok(), valid,
                       "project byte {byte}");
            // Slash is allowed only as the separator of two nonempty root components.
            assert_eq!(BuildFolder::new(&name).is_ok(), valid || byte == b'/', "root byte {byte}");
        }
    }

    #[test]
    fn total_byte_limit_includes_every_folder_and_separator() {
        let root = BuildFolder::new(&"a".repeat(116)).unwrap();
        let project = ProjectLocation::new(root, "p").unwrap();
        assert_eq!(project.file("f").unwrap().as_str().len(), MAX_PATH_BYTES);
        assert_eq!(project.file("ff"), Err(LocationError::TooLong));
        assert_eq!(ProjectLocation::new(root, "pp"), Err(LocationError::TooLong));
        assert_eq!(BuildFolder::new(&"a".repeat(117)), Err(LocationError::TooLong));
        let project = ProjectLocation::new(BuildFolder::default(), "p").unwrap();
        assert_eq!(project.file(&"f".repeat(111)).unwrap().as_str().len(), MAX_PATH_BYTES);
        assert_eq!(project.file(&"f".repeat(112)), Err(LocationError::TooLong));
        assert_eq!(project.file(&"f".repeat(10_000)), Err(LocationError::TooLong));
    }

    #[test]
    fn same_full_path_is_equal_but_similar_prefixes_do_not_mix_projects() {
        let root = BuildFolder::default();
        let one = ProjectLocation::new(root, "card").unwrap();
        let two = ProjectLocation::new(root, "card-2").unwrap();
        let target = one.file("note.txt").unwrap();
        assert_eq!(target, ProjectLocation::new(root, "card").unwrap().file("note.txt").unwrap());
        assert_ne!(target, two.file("note.txt").unwrap());
        assert_ne!(target, one.file("note-2.txt").unwrap());
        assert_eq!(target.project_folder(), "taylor/card");
        assert_eq!(two.file("note.txt").unwrap().project_folder(), "taylor/card-2");
    }
}
