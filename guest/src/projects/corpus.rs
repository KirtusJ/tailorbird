use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::{borrow::ToOwned, format, string::String, vec, vec::Vec};
use crate::project_location::BuildFolder;
use super::{view, Checked, Files, Listing, Opened, Projects, ROWS, VIEW_BYTES};

fn draw(state: &mut u64) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 32) as u32
}

fn snapshot(root: &str, launch: u64, files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut bytes = Vec::new();
    for field in [1, files.len() as u32, 1, root.len() as u32] {
        bytes.extend(field.to_le_bytes());
    }
    bytes.extend(launch.to_le_bytes());
    bytes.extend(root.bytes());
    for (path, contents) in files {
        bytes.extend((path.len() as u16).to_le_bytes());
        bytes.extend(0u16.to_le_bytes());
        bytes.extend((contents.len() as u32).to_le_bytes());
        bytes.extend(path.bytes());
    }
    bytes
}

#[test]
fn browse_seeded_projects_and_open_every_exact_file_across_pages() {
    let mut seed = 2026112203;
    let mut buffer = vec![0; VIEW_BYTES + 1];
    let mut opened = 0;
    for launch in 1..=16 {
        let root = format!("builds/run-{:08x}", draw(&mut seed));
        let folder = BuildFolder::new(&root).unwrap();
        let mut files = BTreeMap::new();
        for project in 0..16 {
            let category = ["site", "budget", "recipe", "schedule"][(draw(&mut seed) % 4) as usize];
            let path = format!("{root}/{category}-{project:02}");
            let value = draw(&mut seed);
            for (name, content) in [
                ("index.html", format!("<h1>{category} {value}</h1>").into_bytes()),
                ("settings.json", format!("{{\"project\":{project},\"value\":{value}}}").into_bytes()),
                ("style.css", format!("body {{ color: #{:06x}; }}", value & 0xffffff).into_bytes()),
                ("items.csv", format!("item,quantity\n{category},{value}\n").into_bytes()),
                ("notes.txt", format!("project {project} at {path}").into_bytes()),
                ("empty.txt", Vec::new()),
                ("asset.bin", vec![0xff, 0, value as u8]),
                ("long.txt", vec![b'x'; VIEW_BYTES + usize::from(project % 2 == 0)]),
            ] {
                files.insert(format!("{path}/{name}"), content);
            }
        }
        let listing = snapshot(&root, launch, &files);
        let expected = Some((folder, launch));
        let mut visited = BTreeMap::new();
        for project_top in (0..16).step_by(ROWS) {
            let projects = Projects::read_from(&listing, expected, project_top);
            assert_eq!(projects.state, Listing::Listed);
            assert!(projects.complete);
            assert_eq!((projects.projects, projects.files, projects.other), (16, 128, 0));
            for row in &projects.rows[..projects.shown] {
                let project = row.location.unwrap();
                assert_eq!(row.files, 8);
                for file_top in (0..8).step_by(ROWS) {
                    let children = Files::read_from(&listing, expected, project, file_top);
                    assert_eq!((children.state, children.count), (Listing::Listed, 8));
                    for file in &children.rows[..children.shown] {
                        let dest = file.destination.unwrap();
                        assert_eq!(dest.project_folder(), project.as_str());
                        let expected_bytes = &files[dest.as_str()];
                        assert_eq!(file.size as usize, expected_bytes.len());
                        let mut stream = Cursor::new(expected_bytes);
                        let chunk = 1 + (draw(&mut seed) % 241) as usize;
                        let result = view(dest, Some(|out: &mut [u8]| {
                            let len = out.len().min(chunk);
                            stream.read(&mut out[..len]).unwrap() as isize
                        }), &mut buffer, None);
                        let expected_state = if expected_bytes.len() > VIEW_BYTES { Opened::TooLarge }
                            else if file.name.as_str() == "asset.bin" { Opened::NotText }
                            else { Opened::Shown };
                        assert_eq!((result.destination, result.state, result.checked), (Some(dest), expected_state, Checked::NoReceipt));
                        if expected_state == Opened::Shown {
                            assert_eq!(result.bytes, expected_bytes.len());
                            if file.name.as_str() == "long.txt" {
                                assert!(result.more);
                            } else {
                                assert!(!result.more);
                                let lines: Vec<_> = result.lines[..result.shown].iter().map(|line| line.as_str()).collect();
                                assert_eq!(lines.join("\n").as_bytes(), expected_bytes.strip_suffix(b"\n").unwrap_or(expected_bytes));
                            }
                        }
                        assert!(visited.insert(dest.as_str().to_owned(), ()).is_none());
                        opened += 1;
                    }
                }
            }
        }
        assert!(visited.keys().eq(files.keys()));
        assert_eq!(Projects::read(&listing, Some((folder, launch + 1))).state, Listing::Unreadable);
    }
    assert_eq!(opened, 2048);
}
