//! The words Tailorbird and the kernel exchange about one new file (pcp-os feature `proposals`, pull
//! request 344). Nothing here asks the kernel anything: it builds the record a proposal is, and reads the
//! kernel's receipt. A receipt this module cannot read whole is refused, never guessed at.
use crate::file_job::{Binding, Outcome, Receipt};
use crate::sha256::sha256;

/// A proposal's fixed part: version, kind, attempt, proposal number, the lengths of name and content.
pub const HEADER: usize = 32;
pub const RECEIPT: usize = 100;
/// The kernel's limits. A record over them is not built, since the kernel would refuse it unseen.
pub const MAX_NAME: usize = 120;
pub const MAX_CONTENT: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    /// The name or the content is longer than the kernel takes, or the name is empty.
    TooLong,
    /// The content has a character the kernel's screen cannot show exactly (anything but printable
    /// characters, spaces and new lines), so it could not ask about it.
    NotShowable,
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
pub fn binding(proposal: u64, name: &str, content: &[u8]) -> Result<Binding, WireError> {
    check(name, content)?;
    Ok(Binding { proposal, content_bytes: content.len() as u32, request_digest: request_digest(name, content), content_digest: sha256(&[content]) })
}

fn check(name: &str, content: &[u8]) -> Result<(), WireError> {
    if name.is_empty() || name.len() > MAX_NAME || content.len() > MAX_CONTENT {
        return Err(WireError::TooLong);
    }
    if !showable(content) {
        return Err(WireError::NotShowable);
    }
    Ok(())
}

/// Build the record in `out` and return its length.
pub fn record(out: &mut [u8], attempt: u64, proposal: u64, name: &str, content: &[u8]) -> Result<usize, WireError> {
    check(name, content)?;
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

    const NAME: &str = "taylor/first-project/note.txt";
    const CONTENT: &[u8] = b"Hello.\nThis file was made by PCP OS after you allowed it.\n";

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
        let length = record(&mut out, 7, 1, NAME, CONTENT).unwrap();
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
        assert_eq!(record(&mut out, 1, 1, NAME, b"a\tb"), Err(WireError::NotShowable));
        assert_eq!(record(&mut out, 1, 1, NAME, "caf\u{e9}".as_bytes()), Err(WireError::NotShowable));
        assert_eq!(record(&mut out, 1, 1, "", b"x"), Err(WireError::TooLong));
        assert_eq!(record(&mut out, 1, 1, NAME, CONTENT), Err(WireError::NoRoom));
        assert_eq!(binding(1, NAME, &[b'a'; MAX_CONTENT + 1]), Err(WireError::TooLong));
    }

    #[test]
    fn a_receipt_is_read_whole_or_not_at_all() {
        let r = kernel_receipt(1, 3, 7, 1);
        let read = receipt(&r).unwrap();
        assert_eq!((read.launch, read.attempt, read.proposal, read.outcome, read.durable), (3, 7, 1, Outcome::Created, false));
        let expected = binding(1, NAME, CONTENT).unwrap();
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
