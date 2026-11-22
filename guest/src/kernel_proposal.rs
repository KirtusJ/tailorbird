//! The words Tailorbird and the kernel exchange about one new file (pcp-os feature `proposals`, pull
//! request 344). Nothing here asks the kernel anything: it builds the record a proposal is, and reads the
//! kernel's receipt. A receipt this module cannot read whole is refused, never guessed at.
use crate::file_job::{Binding, Outcome, Receipt};
use crate::project_location::FileDestination;
use crate::sha256::sha256;

/// A proposal's fixed part: version, kind, attempt, proposal number, the lengths of name and content.
pub const HEADER: usize = 32;
pub const RECEIPT: usize = 100;
/// The kernel's limits. A record over them is not built, since the kernel would refuse it unseen.
pub const MAX_NAME: usize = crate::project_location::MAX_PATH_BYTES;
pub const MAX_CONTENT: usize = 16 * 1024;
/// Source-line admission in the kernel. Its actual display-page limit is checked separately there.
pub const MAX_LINES: usize = 64 * 22;
pub const LINE_COLUMNS: usize = 76;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    /// The content is longer than the kernel takes. Destinations are already validated.
    TooLong,
    /// The content has a character the kernel's screen cannot show exactly (anything but printable
    /// characters, spaces and new lines), so it could not ask about it.
    NotShowable,
    /// Too many source rows after wrapping; a trailing newline adds an empty row.
    TooManyLines,
    /// The place given to build the record in is too small.
    NoRoom,
    /// The receipt is not 100 bytes of version 1 with an outcome this program knows.
    BadReceipt,
}

/// Whether the kernel's screen can show these bytes exactly.
pub fn showable(content: &[u8]) -> bool {
    content.iter().all(|b| (0x20..0x7f).contains(b) || *b == b'\n')
}

/// The digest the kernel puts in its receipt for this exact request: lengths, name, content.
pub fn request_digest(name: &str, content: &[u8]) -> [u8; 32] {
    sha256(&[&(name.len() as u32).to_le_bytes(), &(content.len() as u32).to_le_bytes(), name.as_bytes(), content])
}

/// What the receipt for this proposal must say, worked out from the bytes Tailorbird itself holds.
pub fn binding(proposal: u64, destination: &FileDestination, content: &[u8]) -> Result<Binding, WireError> {
    check(content)?;
    let name = destination.as_str();
    Ok(Binding { proposal, content_bytes: content.len() as u32, request_digest: request_digest(name, content), content_digest: sha256(&[content]) })
}

fn check(content: &[u8]) -> Result<(), WireError> {
    if content.len() > MAX_CONTENT {
        return Err(WireError::TooLong);
    }
    if !showable(content) {
        return Err(WireError::NotShowable);
    }
    let lines: usize = content.split(|b| *b == b'\n')
        .map(|line| line.len().div_ceil(LINE_COLUMNS).max(1)).sum();
    if lines > MAX_LINES { return Err(WireError::TooManyLines); }
    Ok(())
}

/// Build the record with the same typed destination used by the preview and job.
/// This validates syntax and source bounds, not current-launch authority or display capacity.
pub fn record(out: &mut [u8], attempt: u64, proposal: u64, destination: &FileDestination, content: &[u8]) -> Result<usize, WireError> {
    check(content)?;
    let name = destination.as_str();
    let length = HEADER + name.len() + content.len();
    if out.len() < length {
        return Err(WireError::NoRoom);
    }
    out[0..4].copy_from_slice(&1u32.to_le_bytes());
    out[4..8].copy_from_slice(&1u32.to_le_bytes());
    out[8..16].copy_from_slice(&attempt.to_le_bytes());
    out[16..24].copy_from_slice(&proposal.to_le_bytes());
    out[24..28].copy_from_slice(&(name.len() as u32).to_le_bytes());
    out[28..32].copy_from_slice(&(content.len() as u32).to_le_bytes());
    out[HEADER..HEADER + name.len()].copy_from_slice(name.as_bytes());
    out[HEADER + name.len()..length].copy_from_slice(content);
    Ok(length)
}

/// Read a receipt. Every field is taken as the kernel wrote it; matching it to a job is the job's work.
pub fn receipt(bytes: &[u8]) -> Result<Receipt, WireError> {
    if bytes.len() != RECEIPT {
        return Err(WireError::BadReceipt);
    }
    let word = |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let long = |at: usize| u64::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3], bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]]);
    let outcome = match word(4) {
        1 => Outcome::Created,
        2 => Outcome::Declined,
        3 => Outcome::NameTaken,
        5 => Outcome::NoSpace,
        6 => Outcome::CannotDisplay,
        _ => return Err(WireError::BadReceipt),
    };
    if word(0) != 1 || word(96) > 1 {
        return Err(WireError::BadReceipt);
    }
    let (mut request, mut content) = ([0u8; 32], [0u8; 32]);
    request.copy_from_slice(&bytes[32..64]);
    content.copy_from_slice(&bytes[64..96]);
    Ok(Receipt { launch: long(8), attempt: long(16), proposal: long(24), request_digest: request, content_digest: content, outcome, durable: word(96) == 1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_location::BuildFolder;

    const NAME: &str = "taylor/first-project/note.txt";
    const CONTENT: &[u8] = b"Hello.\nThis file was made by PCP OS after you allowed it.\n";

    fn name() -> FileDestination { BuildFolder::default().parse_project_file(NAME).unwrap().1 }

    fn kernel_receipt(outcome: u32, launch: u64, attempt: u64, proposal: u64) -> [u8; RECEIPT] {
        // As the kernel writes it (pcp-os sys.rs `receipt`), from the same name and content.
        let mut r = [0u8; RECEIPT];
        r[0..4].copy_from_slice(&1u32.to_le_bytes());
        r[4..8].copy_from_slice(&outcome.to_le_bytes());
        r[8..16].copy_from_slice(&launch.to_le_bytes());
        r[16..24].copy_from_slice(&attempt.to_le_bytes());
        r[24..32].copy_from_slice(&proposal.to_le_bytes());
        r[32..64].copy_from_slice(&request_digest(NAME, CONTENT));
        r[64..96].copy_from_slice(&sha256(&[CONTENT]));
        r
    }

    #[test]
    fn the_record_is_the_one_the_kernels_test_program_writes() {
        let mut out = [0u8; 256];
        let length = record(&mut out, 7, 1, &name(), CONTENT).unwrap();
        assert_eq!(length, 32 + NAME.len() + CONTENT.len());
        assert_eq!(&out[0..8], &[1, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(u64::from_le_bytes(out[8..16].try_into().unwrap()), 7);
        assert_eq!(u64::from_le_bytes(out[16..24].try_into().unwrap()), 1);
        assert_eq!(&out[32..32 + NAME.len()], NAME.as_bytes());
        assert_eq!(&out[32 + NAME.len()..length], CONTENT);
    }

    #[test]
    fn what_the_kernel_would_refuse_unseen_is_not_built() {
        let mut out = [0u8; 64];
        assert_eq!(record(&mut out, 1, 1, &name(), b"a\tb"), Err(WireError::NotShowable));
        assert_eq!(record(&mut out, 1, 1, &name(), "caf\u{e9}".as_bytes()), Err(WireError::NotShowable));
        assert_eq!(record(&mut out, 1, 1, &name(), CONTENT), Err(WireError::NoRoom));
        assert_eq!(binding(1, &name(), &[b'a'; MAX_CONTENT + 1]), Err(WireError::TooLong));
    }

    #[test]
    fn a_receipt_is_read_whole_or_not_at_all() {
        let r = kernel_receipt(1, 3, 7, 1);
        let read = receipt(&r).unwrap();
        assert_eq!((read.launch, read.attempt, read.proposal, read.outcome, read.durable), (3, 7, 1, Outcome::Created, false));
        let expected = binding(1, &name(), CONTENT).unwrap();
        assert_eq!((read.request_digest, read.content_digest), (expected.request_digest, expected.content_digest));
        assert_eq!(receipt(&r[..99]), Err(WireError::BadReceipt));
        for (at, value) in [(0usize, 2u8), (4, 4), (4, 9), (96, 2)] {
            let mut bad = r;
            bad[at] = value;
            assert_eq!(receipt(&bad), Err(WireError::BadReceipt), "byte {} set to {}", at, value);
        }
        for (code, outcome) in [(2, Outcome::Declined), (3, Outcome::NameTaken), (5, Outcome::NoSpace), (6, Outcome::CannotDisplay)] {
            assert_eq!(receipt(&kernel_receipt(code, 1, 1, 1)).unwrap().outcome, outcome);
        }
    }

    #[test]
    fn another_name_with_the_same_content_is_another_request() {
        assert_ne!(request_digest(NAME, CONTENT), request_digest("taylor/first-project/other.txt", CONTENT));
        assert_ne!(request_digest("ab", b"c"), request_digest("a", b"bc"));
    }

    #[test]
    fn source_line_limit_matches_empty_trailing_and_wrapped_rows() {
        let target = name();
        let mut out = [0xa5; HEADER + MAX_NAME + MAX_CONTENT];
        for content in [std::vec![], std::vec![b'\n'; MAX_LINES - 1]] {
            assert!(binding(1, &target, &content).is_ok());
            assert!(record(&mut out, 1, 1, &target, &content).is_ok());
        }
        let mut wrapped = std::vec![b'\n'; MAX_LINES - 1];
        wrapped.extend_from_slice(&[b'x'; LINE_COLUMNS]);
        assert!(binding(1, &target, &wrapped).is_ok());
        assert!(record(&mut out, 1, 1, &target, &wrapped).is_ok());
        wrapped.push(b'x');
        for content in [std::vec![b'\n'; MAX_LINES], wrapped] {
            out.fill(0xa5);
            assert_eq!(binding(1, &target, &content), Err(WireError::TooManyLines));
            assert_eq!(record(&mut out, 1, 1, &target, &content), Err(WireError::TooManyLines));
            assert!(out.iter().all(|b| *b == 0xa5));
        }
        assert!(binding(1, &target, &[b'x'; MAX_CONTENT]).is_ok());
    }

    #[test]
    fn full_typed_destination_is_identical_in_wire_binding_and_preview() {
        let root_text = "a".repeat(116);
        let root = BuildFolder::new(&root_text).unwrap();
        let project = crate::project_location::ProjectLocation::new(root, "p").unwrap();
        let target = project.file("z").unwrap();
        assert_eq!(target.as_str().len(), MAX_NAME);
        let mut out = [0; HEADER + MAX_NAME + 3];
        let length = record(&mut out, 9, 2, &target, b"yes").unwrap();
        assert_eq!(length, out.len());
        let name_len = u32::from_le_bytes(out[24..28].try_into().unwrap()) as usize;
        let emitted = core::str::from_utf8(&out[HEADER..HEADER + name_len]).unwrap();
        assert_eq!(emitted, target.as_str());
        let expected = binding(2, &target, b"yes").unwrap();
        assert_eq!(expected.request_digest, request_digest(emitted, &out[HEADER + name_len..length]));
        assert_ne!(expected.request_digest, binding(2, &project.file("y").unwrap(), b"yes").unwrap().request_digest);
    }
}

#[cfg(test)]
mod against_the_kernel {
    use super::*;

    /// The digests the kernel itself printed for this name and content in its own check (pcp-os
    /// `examples/proposals`, recorded run of 22 November 2026): this module computes the same.
    #[test]
    fn digests_equal_the_ones_a_running_kernel_gave() {
        let name = "taylor/first-project/note.txt";
        let content = b"Hello.\nThis file was made by PCP OS after you allowed it.\n";
        let hex = |d: [u8; 32]| -> std::string::String { d.iter().map(|b| std::format!("{:02x}", b)).collect() };
        assert_eq!(hex(request_digest(name, content)), "fbc17b936b4736c46543d49312eba888c378c347657646a5b65747cc89024dc1");
        assert_eq!(hex(sha256(&[content])), "b7507ea2ffd5de388cc7545bc17c37022be3b8a7d738c1b963db34717c46b110");
    }
}
