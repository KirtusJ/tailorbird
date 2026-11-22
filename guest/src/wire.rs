//! The records of `talk` as bytes on the channel between this program and a local model service
//! (PCP OS's `dev/talk`). No I/O here: bytes in, records out, and the reverse. A channel is a stream
//! of bytes with no boundaries of its own, so every record says how long it is, and a record that is
//! not well formed ends the exchange: nothing after it can be trusted to start where it claims.
//!
//! One record: 24 bytes, then `length` bytes.
//!
//! | bytes | holds |
//! |---|---|
//! | 0..2 | `TB` |
//! | 2 | version, 1 |
//! | 3 | kind |
//! | 4 | flags (SAY: bit 0, answer with the conversation so far in mind; otherwise 0) |
//! | 5 | 0 |
//! | 6..8 | length of what follows, little-endian, at most 480 |
//! | 8..16 | conversation, little-endian |
//! | 16..24 | reply, little-endian |
//!
//! From this program: SAY (the person's message), STOP (the person stopped that reply), FORGET (that
//! conversation is over; keep nothing of it). From the service: LOADING, READY, NOT_STARTED (about
//! itself; conversation and reply 0), and CHUNK, DONE, FAILED (about one reply).
use crate::talk::Outgoing;

pub const HEADER: usize = 24;
/// The most one record carries after its header: as much as one turn of the conversation keeps.
pub const MOST: usize = crate::talk::TURN;
pub const VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Say = 1,
    Stop = 2,
    Forget = 3,
    Loading = 16,
    Ready = 17,
    NotStarted = 18,
    Chunk = 32,
    Done = 33,
    Failed = 34,
}

impl Kind {
    fn of(byte: u8) -> Option<Kind> {
        Some(match byte {
            1 => Kind::Say,
            2 => Kind::Stop,
            3 => Kind::Forget,
            16 => Kind::Loading,
            17 => Kind::Ready,
            18 => Kind::NotStarted,
            32 => Kind::Chunk,
            33 => Kind::Done,
            34 => Kind::Failed,
            _ => return None,
        })
    }

    /// Whether a record of this kind may carry anything after its header.
    fn carries(self) -> bool {
        matches!(self, Kind::Say | Kind::Chunk)
    }

    /// Whether a record of this kind is about one reply of one conversation.
    fn names_a_reply(self) -> bool {
        matches!(self, Kind::Say | Kind::Stop | Kind::Chunk | Kind::Done | Kind::Failed)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record<'a> {
    pub kind: Kind,
    pub history: bool,
    pub conversation: u64,
    pub reply: u64,
    pub body: &'a [u8],
}

/// A record as bytes, ready to write. Held whole so that a channel with no room can be tried again
/// later from where it stopped.
#[derive(Clone, Copy)]
pub struct Packed {
    bytes: [u8; HEADER + MOST],
    length: usize,
}

impl Packed {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

/// Put one record into bytes. None if it is not one this module would read back: a body longer than
/// a record carries, a body on a kind that has none, or a reply named where none is meant.
pub fn pack(record: Record<'_>) -> Option<Packed> {
    if record.body.len() > MOST || (!record.body.is_empty() && !record.kind.carries()) {
        return None;
    }
    if record.history && record.kind != Kind::Say {
        return None;
    }
    if !record.kind.names_a_reply() && record.reply != 0 {
        return None;
    }
    if record.kind.names_a_reply() && (record.conversation == 0 || record.reply == 0) {
        return None;
    }
    let mut bytes = [0u8; HEADER + MOST];
    bytes[0] = b'T';
    bytes[1] = b'B';
    bytes[2] = VERSION;
    bytes[3] = record.kind as u8;
    bytes[4] = record.history as u8;
    bytes[6..8].copy_from_slice(&(record.body.len() as u16).to_le_bytes());
    bytes[8..16].copy_from_slice(&record.conversation.to_le_bytes());
    bytes[16..24].copy_from_slice(&record.reply.to_le_bytes());
    bytes[HEADER..HEADER + record.body.len()].copy_from_slice(record.body);
    Some(Packed { bytes, length: HEADER + record.body.len() })
}

/// The person's message, as the record the service reads.
pub fn say(outgoing: &Outgoing) -> Option<Packed> {
    pack(Record { kind: Kind::Say, history: outgoing.history, conversation: outgoing.conversation, reply: outgoing.reply, body: outgoing.text.as_str().as_bytes() })
}

pub fn stop(conversation: u64, reply: u64) -> Option<Packed> {
    pack(Record { kind: Kind::Stop, history: false, conversation, reply, body: &[] })
}

pub fn forget(conversation: u64) -> Option<Packed> {
    pack(Record { kind: Kind::Forget, history: false, conversation, reply: 0, body: &[] })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The byte was taken; no record is complete yet.
    More,
    /// The byte completed a record: read it with `record()` before giving another byte.
    Whole,
    /// The bytes are not a record. Nothing more is read from this channel.
    Broken,
}

/// Reads records out of a stream of bytes, however the bytes arrive.
pub struct Reader {
    bytes: [u8; HEADER + MOST],
    have: usize,
    whole: bool,
    broken: bool,
}

impl Reader {
    pub const fn new() -> Self {
        Self { bytes: [0; HEADER + MOST], have: 0, whole: false, broken: false }
    }

    pub fn broken(&self) -> bool {
        self.broken
    }

    fn body_length(&self) -> usize {
        u16::from_le_bytes([self.bytes[6], self.bytes[7]]) as usize
    }

    /// Whether the header held so far could begin a record. Checked as each byte arrives, so that
    /// what is not a record is refused at its first wrong byte and never waited on.
    fn header_holds(&self) -> bool {
        let b = &self.bytes;
        let at = self.have;
        if at >= 1 && b[0] != b'T' || at >= 2 && b[1] != b'B' || at >= 3 && b[2] != VERSION {
            return false;
        }
        let kind = if at >= 4 { Kind::of(b[3]) } else { None };
        if at >= 4 && kind.is_none() {
            return false;
        }
        if at >= 5 && (b[4] > 1 || b[4] == 1 && kind != Some(Kind::Say)) || at >= 6 && b[5] != 0 {
            return false;
        }
        if at >= 8 {
            let length = self.body_length();
            if length > MOST || length != 0 && !kind.is_some_and(Kind::carries) {
                return false;
            }
        }
        if at >= HEADER {
            let conversation = u64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]);
            let reply = u64::from_le_bytes([b[16], b[17], b[18], b[19], b[20], b[21], b[22], b[23]]);
            let names = kind.is_some_and(Kind::names_a_reply);
            if names && (conversation == 0 || reply == 0) || !names && reply != 0 {
                return false;
            }
        }
        true
    }

    pub fn take(&mut self, byte: u8) -> Step {
        if self.broken {
            return Step::Broken;
        }
        if self.whole {
            self.have = 0;
            self.whole = false;
        }
        self.bytes[self.have] = byte;
        self.have += 1;
        if self.have <= HEADER && !self.header_holds() {
            self.broken = true;
            return Step::Broken;
        }
        if self.have >= HEADER && self.have == HEADER + self.body_length() {
            self.whole = true;
            return Step::Whole;
        }
        Step::More
    }

    /// The record the last byte completed.
    pub fn record(&self) -> Option<Record<'_>> {
        if !self.whole {
            return None;
        }
        let b = &self.bytes;
        Some(Record {
            kind: Kind::of(b[3])?,
            history: b[4] == 1,
            conversation: u64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]),
            reply: u64::from_le_bytes([b[16], b[17], b[18], b[19], b[20], b[21], b[22], b[23]]),
            body: &b[HEADER..self.have],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Text;

    fn read_all(bytes: &[u8]) -> (std::vec::Vec<(Kind, bool, u64, u64, std::vec::Vec<u8>)>, bool) {
        let mut reader = Reader::new();
        let mut records = std::vec::Vec::new();
        for byte in bytes {
            match reader.take(*byte) {
                Step::More => {}
                Step::Whole => {
                    let r = reader.record().unwrap();
                    records.push((r.kind, r.history, r.conversation, r.reply, r.body.to_vec()));
                }
                Step::Broken => return (records, true),
            }
        }
        (records, false)
    }

    #[test]
    fn every_kind_is_read_back_as_it_was_packed_however_the_bytes_are_cut() {
        let message = Outgoing { conversation: 7, reply: 3, history: true, text: Text::display("What is in the notes folder?") };
        let mut stream = std::vec::Vec::new();
        stream.extend_from_slice(say(&message).unwrap().bytes());
        stream.extend_from_slice(stop(7, 3).unwrap().bytes());
        stream.extend_from_slice(forget(7).unwrap().bytes());
        for (kind, conversation, reply, body) in [(Kind::Loading, 0, 0, &b""[..]), (Kind::Ready, 0, 0, b""), (Kind::NotStarted, 0, 0, b""),
                                                  (Kind::Chunk, 8, 1, b"Two files."), (Kind::Done, 8, 1, b""), (Kind::Failed, 8, 2, b"")] {
            stream.extend_from_slice(pack(Record { kind, history: false, conversation, reply, body }).unwrap().bytes());
        }
        let (records, broken) = read_all(&stream);
        assert!(!broken);
        assert_eq!(records.len(), 9);
        assert_eq!(records[0], (Kind::Say, true, 7, 3, b"What is in the notes folder?".to_vec()));
        assert_eq!(records[1], (Kind::Stop, false, 7, 3, std::vec![]));
        assert_eq!(records[2], (Kind::Forget, false, 7, 0, std::vec![]));
        assert_eq!(records[6], (Kind::Chunk, false, 8, 1, b"Two files.".to_vec()));
        assert_eq!(records[8], (Kind::Failed, false, 8, 2, std::vec![]));
    }

    #[test]
    fn the_longest_record_is_carried_and_one_byte_more_is_not_packed() {
        let longest = [b'a'; MOST];
        let packed = pack(Record { kind: Kind::Chunk, history: false, conversation: 1, reply: 1, body: &longest }).unwrap();
        assert_eq!(packed.bytes().len(), HEADER + MOST);
        let (records, broken) = read_all(packed.bytes());
        assert!(!broken);
        assert_eq!(records[0].4.len(), MOST);
        let over = [b'a'; MOST + 1];
        assert!(pack(Record { kind: Kind::Chunk, history: false, conversation: 1, reply: 1, body: &over }).is_none());
    }

    #[test]
    fn what_is_not_a_record_is_not_packed() {
        let none = |kind, history, conversation, reply, body: &[u8]| pack(Record { kind, history, conversation, reply, body }).is_none();
        assert!(none(Kind::Done, false, 1, 1, b"x")); // a body where none belongs
        assert!(none(Kind::Chunk, true, 1, 1, b"x")); // the history flag on anything but SAY
        assert!(none(Kind::Ready, false, 0, 1, b"")); // a reply named by a record about the service
        assert!(none(Kind::Chunk, false, 0, 1, b"x")); // no conversation
        assert!(none(Kind::Chunk, false, 1, 0, b"x")); // no reply
    }

    #[test]
    fn one_wrong_byte_anywhere_in_a_header_ends_the_reading_at_that_byte() {
        let good = pack(Record { kind: Kind::Chunk, history: false, conversation: 5, reply: 2, body: b"hello" }).unwrap();
        // Each case: which byte is changed, to what, and at which byte the reader must refuse.
        for (at, value, refused_at) in [(0usize, b'X', 0usize), (1, b'X', 1), (2, 2, 2), (3, 99, 3), (4, 1, 4), (4, 2, 4), (5, 1, 5),
                                        (7, 2, 7), // length 5 + 512: more than a record carries
                                        (8, 0, 23), // conversation 0, known once the header is whole
                                        (16, 0, 23)] {
            let mut bytes = good.bytes().to_vec();
            bytes[at] = value;
            let mut reader = Reader::new();
            let mut refused = None;
            for (i, byte) in bytes.iter().enumerate() {
                if reader.take(*byte) == Step::Broken {
                    refused = Some(i);
                    break;
                }
            }
            assert_eq!(refused, Some(refused_at), "byte {at} set to {value}");
            // Once broken it stays broken, whatever follows.
            assert!(good.bytes().iter().all(|b| reader.take(*b) == Step::Broken));
            assert!(reader.record().is_none());
        }
    }

    #[test]
    fn a_body_on_a_record_that_has_none_is_refused_before_the_body_is_read() {
        let mut bytes = pack(Record { kind: Kind::Done, history: false, conversation: 1, reply: 1, body: b"" }).unwrap().bytes().to_vec();
        bytes[6] = 4;
        let (records, broken) = read_all(&bytes);
        assert!(broken && records.is_empty());
    }

    #[test]
    fn a_record_cut_short_is_not_given_and_the_rest_completes_it() {
        let packed = pack(Record { kind: Kind::Chunk, history: false, conversation: 1, reply: 1, body: b"partial" }).unwrap();
        let mut reader = Reader::new();
        let (first, rest) = packed.bytes().split_at(HEADER + 3);
        assert!(first.iter().all(|b| reader.take(*b) == Step::More));
        assert!(reader.record().is_none());
        let steps: std::vec::Vec<Step> = rest.iter().map(|b| reader.take(*b)).collect();
        assert_eq!(steps.last(), Some(&Step::Whole));
        assert_eq!(reader.record().unwrap().body, b"partial");
    }
}
