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
//! | 4 | flags (SAY: bit 0, answer with the conversation so far in mind; bit 1, say how far the reading of this message has got; otherwise 0) |
//! | 5 | 0 |
//! | 6..8 | length of what follows, little-endian, at most 480 |
//! | 8..16 | conversation, little-endian |
//! | 16..24 | reply, little-endian |
//!
//! From this program: SAY (the person's message), STOP (the person stopped that reply), FORGET (that
//! conversation is over; keep nothing of it). From the service: LOADING, READY, NOT_STARTED (about
//! itself), and READING, CHUNK, DONE, FAILED (about one reply).
//!
//! Who a record is about is part of whether it is a record at all (agreed with C3, who writes the
//! service): LOADING, READY and NOT_STARTED carry conversation 0 and reply 0; FORGET carries a
//! conversation and reply 0; SAY, STOP, READING, CHUNK, DONE and FAILED carry both, neither 0.
//!
//! LOADING has no body, or sixteen bytes: how many of the model's bytes the service has checked so
//! far and how many there are, each little-endian, the first no more than the second and the second
//! not 0. It says how far a check has got. It never means ready: only READY does, and the service
//! sends READY only when it can answer.
//!
//! READING has sixteen bytes, of the same form: how many tokens of the message (with whatever the
//! service puts around it) it has read so far and how many there are. The service sends it only for
//! a reply whose SAY had bit 1 of the flags set, and only before that reply's first CHUNK. A reader
//! that does not know READING refuses it like any unknown kind, and one that does not know bit 1
//! refuses that SAY: the two ends of a channel are built from one revision.
//! Each READING is checked by itself here. What holds across the READINGs of one reply (the count
//! never goes back, the total never changes) is kept by the conversation: one that breaks it is not
//! shown, and is counted with what arrived late.
//!
//! What the service is expected to do, which this module cannot enforce: send a CHUNK only at the
//! boundary of whole UTF-8 characters, holding back the bytes of an unfinished one (an answer is as
//! many CHUNKs as it needs; 480 bytes is one record, not one answer); on STOP, stop that reply within
//! a bounded time, send nothing more for it, and stay ready for the next SAY; on FORGET, drop what it
//! kept of that conversation; with `history` clear on a SAY, answer that message alone without
//! dropping what it kept.
use crate::talk::Outgoing;

pub const HEADER: usize = 24;
/// The most one record carries after its header: as much as one turn of the conversation keeps.
pub const MOST: usize = crate::talk::TURN;
pub const VERSION: u8 = 1;
/// The length of the body of a LOADING record that says how far a check has got.
pub const PROGRESS: usize = 16;

/// How far a check has got, out of a LOADING record's body: (checked, of). None if the body is not
/// sixteen bytes that say so (checked no more than of, of not 0).
pub fn progress(body: &[u8]) -> Option<(u64, u64)> {
    if body.len() != PROGRESS {
        return None;
    }
    let checked = u64::from_le_bytes([body[0], body[1], body[2], body[3], body[4], body[5], body[6], body[7]]);
    let of = u64::from_le_bytes([body[8], body[9], body[10], body[11], body[12], body[13], body[14], body[15]]);
    (of != 0 && checked <= of).then_some((checked, of))
}

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
    Reading = 35,
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
            35 => Kind::Reading,
            _ => return None,
        })
    }

    /// Whether a body of this length may follow the header of a record of this kind.
    fn takes(self, length: usize) -> bool {
        match self {
            Kind::Say | Kind::Chunk => length <= MOST,
            Kind::Loading => length == 0 || length == PROGRESS,
            Kind::Reading => length == PROGRESS,
            _ => length == 0,
        }
    }

    /// Whether these are the conversation and reply a record of this kind must carry.
    fn names(self, conversation: u64, reply: u64) -> bool {
        match self {
            Kind::Loading | Kind::Ready | Kind::NotStarted => conversation == 0 && reply == 0,
            Kind::Forget => conversation != 0 && reply == 0,
            _ => conversation != 0 && reply != 0,
        }
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

/// Put one record into bytes. None if it is not one this module would read back: a body its kind
/// does not take, progress that does not add up, or a conversation or reply its kind does not carry.
pub fn pack(record: Record<'_>) -> Option<Packed> {
    if !record.kind.takes(record.body.len()) || !record.kind.names(record.conversation, record.reply) {
        return None;
    }
    if record.history && record.kind != Kind::Say {
        return None;
    }
    if matches!(record.kind, Kind::Loading | Kind::Reading) && !record.body.is_empty() && progress(record.body).is_none() {
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

/// Bit 1 of a SAY's flags: the service is asked to say how far its reading of this message has got.
pub const PROGRESS_WANTED: u8 = 2;

/// The person's message, as the record the service reads. It asks for reading progress.
pub fn say(outgoing: &Outgoing) -> Option<Packed> {
    let mut packed = pack(Record { kind: Kind::Say, history: outgoing.history, conversation: outgoing.conversation, reply: outgoing.reply, body: outgoing.text.as_str().as_bytes() })?;
    packed.bytes[4] |= PROGRESS_WANTED;
    Some(packed)
}

/// How far the reading of a message has got, as the record this program reads. For a service.
pub fn reading(conversation: u64, reply: u64, read: u64, of: u64) -> Option<Packed> {
    let mut body = [0u8; PROGRESS];
    body[..8].copy_from_slice(&read.to_le_bytes());
    body[8..].copy_from_slice(&of.to_le_bytes());
    pack(Record { kind: Kind::Reading, history: false, conversation, reply, body: &body })
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
        if at >= 5 && (b[4] > (1 | PROGRESS_WANTED) || b[4] != 0 && kind != Some(Kind::Say)) || at >= 6 && b[5] != 0 {
            return false;
        }
        if at >= 8 {
            if !kind.is_some_and(|k| k.takes(self.body_length())) {
                return false;
            }
        }
        if at >= HEADER {
            let conversation = u64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]);
            let reply = u64::from_le_bytes([b[16], b[17], b[18], b[19], b[20], b[21], b[22], b[23]]);
            if !kind.is_some_and(|k| k.names(conversation, reply)) {
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
            // Progress that does not add up is not a record either.
            if (self.bytes[3] == Kind::Loading as u8 || self.bytes[3] == Kind::Reading as u8) && self.have > HEADER && progress(&self.bytes[HEADER..self.have]).is_none() {
                self.broken = true;
                return Step::Broken;
            }
            self.whole = true;
            return Step::Whole;
        }
        Step::More
    }

    /// Whether the SAY the last byte completed asks for reading progress. For a service.
    pub fn progress_wanted(&self) -> bool {
        self.whole && self.bytes[3] == Kind::Say as u8 && self.bytes[4] & PROGRESS_WANTED != 0
    }

    /// The record the last byte completed.
    pub fn record(&self) -> Option<Record<'_>> {
        if !self.whole {
            return None;
        }
        let b = &self.bytes;
        Some(Record {
            kind: Kind::of(b[3])?,
            history: b[4] & 1 == 1,
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

    #[test]
    fn reading_is_a_record_only_with_progress_that_adds_up_and_a_reply_to_be_about() {
        let packed = reading(4, 7, 37, 212).unwrap();
        let (records, broken) = read_all(packed.bytes());
        assert!(!broken);
        assert_eq!(records.len(), 1);
        assert_eq!((records[0].0, records[0].2, records[0].3, progress(&records[0].4)), (Kind::Reading, 4, 7, Some((37, 212))));
        // More read than there is, nothing to read, no reply, no conversation: none is made.
        assert!(reading(4, 7, 213, 212).is_none() && reading(4, 7, 0, 0).is_none() && reading(4, 0, 1, 2).is_none() && reading(0, 7, 1, 2).is_none());
        // The same refused when read: a body of another length, and counts that do not add up.
        let mut short = packed.bytes().to_vec();
        short[6] = 8;
        assert!(read_all(&short[..HEADER + 8]).1);
        let mut over = packed.bytes().to_vec();
        over[HEADER..HEADER + 8].copy_from_slice(&213u64.to_le_bytes());
        assert!(read_all(&over).1);
    }

    #[test]
    fn a_say_asks_for_reading_progress_and_no_other_record_may_carry_a_flag() {
        for history in [false, true] {
            let outgoing = Outgoing { conversation: 1, reply: 1, history, text: Text::display("hello") };
            let packed = say(&outgoing).unwrap();
            assert_eq!(packed.bytes()[4], history as u8 | PROGRESS_WANTED);
            let mut reader = Reader::new();
            let mut whole = false;
            for byte in packed.bytes() {
                whole = reader.take(*byte) == Step::Whole;
            }
            assert!(whole && reader.progress_wanted());
            let record = reader.record().unwrap();
            assert_eq!((record.kind, record.history, record.body), (Kind::Say, history, &b"hello"[..]));
        }
        // A SAY packed without the bit does not ask (an earlier program's).
        let plain = pack(Record { kind: Kind::Say, history: true, conversation: 1, reply: 1, body: b"x" }).unwrap();
        let mut reader = Reader::new();
        for byte in plain.bytes() {
            reader.take(*byte);
        }
        assert!(!reader.progress_wanted());
        // Any flag on a record that is not a SAY, and any flag beyond the two, is refused.
        for (kind, conversation, reply, body) in [(Kind::Chunk, 1, 1, &b"x"[..]), (Kind::Stop, 1, 1, b""), (Kind::Ready, 0, 0, b"")] {
            for flag in [1u8, 2, 3] {
                let mut bytes = pack(Record { kind, history: false, conversation, reply, body }).unwrap().bytes().to_vec();
                bytes[4] = flag;
                assert!(read_all(&bytes).1, "{kind:?} with flag {flag}");
            }
        }
        let mut bytes = plain.bytes().to_vec();
        bytes[4] = 4;
        assert!(read_all(&bytes).1);
    }

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
        assert!(none(Kind::Ready, false, 7, 0, b"")); // a conversation named by one (C3: this was accepted)
        assert!(none(Kind::Loading, false, 7, 0, b""));
        assert!(none(Kind::NotStarted, false, 7, 0, b""));
        assert!(none(Kind::Forget, false, 0, 0, b"")); // forgetting no conversation
        assert!(none(Kind::Forget, false, 7, 1, b"")); // a reply named by FORGET
        assert!(none(Kind::Stop, false, 7, 0, b""));
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
    fn a_service_record_that_names_a_conversation_is_not_read_as_one() {
        // Built by hand, since `pack` will not make it: READY with conversation 7.
        let mut bytes = pack(Record { kind: Kind::Ready, history: false, conversation: 0, reply: 0, body: b"" }).unwrap().bytes().to_vec();
        bytes[8] = 7;
        let (records, broken) = read_all(&bytes);
        assert!(broken && records.is_empty());
        // And FORGET with no conversation.
        let mut bytes = forget(7).unwrap().bytes().to_vec();
        bytes[8] = 0;
        assert!(read_all(&bytes).1);
    }

    #[test]
    fn loading_says_how_far_a_check_has_got_only_when_the_numbers_add_up() {
        let body = |checked: u64, of: u64| [checked.to_le_bytes(), of.to_le_bytes()].concat();
        let made = |body: &[u8]| pack(Record { kind: Kind::Loading, history: false, conversation: 0, reply: 0, body });
        for (checked, of) in [(0u64, 5_027_783_488u64), (1 << 32, 5_027_783_488), (5_027_783_488, 5_027_783_488)] {
            let packed = made(&body(checked, of)).unwrap();
            let (records, broken) = read_all(packed.bytes());
            assert!(!broken);
            assert_eq!(progress(&records[0].4), Some((checked, of)));
        }
        assert_eq!(progress(b""), None);
        assert!(made(b"").is_some()); // loading, with nothing said about how far
        assert!(made(&body(2, 1)).is_none()); // more checked than there is
        assert!(made(&body(0, 0)).is_none()); // of nothing
        assert!(made(&[0u8; 15]).is_none() && made(&[0u8; 17]).is_none());
        // The same, arriving as bytes from a service that packed them itself.
        let mut bytes = made(&body(1, 2)).unwrap().bytes().to_vec();
        bytes[HEADER] = 3;
        assert!(read_all(&bytes).1);
        let mut bytes = made(b"").unwrap().bytes().to_vec();
        bytes[6] = 8;
        assert!(read_all(&bytes).1);
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
