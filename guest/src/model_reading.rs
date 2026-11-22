//! Optional prompt progress uses the same framed stream as the answer.
use crate::wire::{self, Packed};

/// Created for one SAY. An older UI never requests a progress record.
pub struct ReadingProgress {
    wanted: bool,
    ids: (u64, u64),
    last_us: Option<u64>,
}

impl ReadingProgress {
    pub fn new(wanted: bool, conversation: u64, reply: u64) -> Self {
        Self { wanted, ids: (conversation, reply), last_us: None }
    }

    pub fn token_processed(&mut self, completed: usize, total: usize, clock: impl FnOnce() -> u64) -> Option<Packed> {
        if !self.wanted {
            return None;
        }
        let now = clock();
        if self.last_us.is_some_and(|last| now.saturating_sub(last) < 250_000) {
            return None;
        }
        let packet = wire::reading(self.ids.0, self.ids.1, completed as u64, total as u64)?;
        self.last_us = Some(now);
        Some(packet)
    }
}

/// At most one partly written READING. A full queue drops an untouched update.
/// Once any byte has gone out, finish its frame before sending another record.
#[derive(Default)]
pub struct ProgressOutput {
    partial: Option<(Packed, usize)>,
}

impl ProgressOutput {
    pub fn pending(&self) -> bool {
        self.partial.is_some()
    }

    /// One nonblocking write per call. The writer returns zero on would-block.
    pub fn pump(&mut self, next: Option<Packed>, write: impl FnOnce(&[u8]) -> Result<usize, &'static str>) -> Result<(), &'static str> {
        let Some((packet, at)) = self.partial.take().or_else(|| next.map(|p| (p, 0))) else {
            return Ok(());
        };
        let rest = &packet.bytes()[at..];
        let sent = write(rest)?;
        if sent > rest.len() {
            return Err("invalid progress write length");
        }
        let at = at + sent;
        if at > 0 && at < packet.bytes().len() {
            self.partial = Some((packet, at));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model_inbox::Inbox, talk::Outgoing, wire::{Kind, Reader, Record, Step}, Text};

    #[test]
    fn old_and_new_say_choose_progress_independently_and_keep_their_history_bit() {
        let mut inbox = Inbox::new();
        for wanted in [false, true, false, true] {
            for history in [true, false] {
                let request = Outgoing { conversation: 2, reply: 7, history, text: Text::display("hello") };
                let say = if wanted {
                    wire::say(&request)
                } else {
                    wire::pack(Record {kind: Kind::Say, history, conversation: 2, reply: 7, body: b"hello"})
                }.unwrap();
                for &byte in say.bytes() { inbox.take(byte, None).unwrap(); }
                let received = inbox.pending.take().unwrap();
                assert_eq!((received.history, received.progress), (history, wanted));
                let mut reading = ReadingProgress::new(received.progress, received.conversation, received.reply);
                let packet = reading.token_processed(1, 100, || { assert!(wanted); 10 });
                assert_eq!(packet.is_some(), wanted);
                if let Some(packet) = packet {
                    let mut reader = Reader::new();
                    for &byte in packet.bytes() { assert_ne!(reader.take(byte), Step::Broken); }
                    let r = reader.record().unwrap();
                    assert_eq!((r.kind, r.conversation, r.reply), (Kind::Reading, 2, 7));
                    assert_eq!(wire::progress(r.body), Some((1, 100)));
                }
            }
        }
    }

    #[test]
    fn report_the_first_token_then_at_most_four_times_per_second() {
        let mut progress = ReadingProgress::new(true, 7, 9);
        let mut sent = alloc::vec::Vec::new();
        for i in 0..2048 {
            if progress.token_processed(i+1, 2048, || 100+i as u64*1000).is_some() {
                sent.push(i+1);
            }
        }
        assert_eq!(sent, [1,251,501,751,1001,1251,1501,1751,2001]);
        assert!(progress.token_processed(2048, 2048, || 0).is_none());
    }

    #[test]
    fn a_full_queue_skips_an_untouched_update_and_completes_every_possible_partial_frame() {
        let first = wire::reading(1, 2, 1, 100).unwrap();
        let later = wire::reading(1, 2, 50, 100).unwrap();
        for split in 0..=first.bytes().len() {
            let mut output = ProgressOutput::default();
            let mut bytes = alloc::vec::Vec::new();
            output.pump(Some(first), |src| { bytes.extend_from_slice(&src[..split]); Ok(split) }).unwrap();
            let partial = split > 0 && split < first.bytes().len();
            assert_eq!(output.pending(), partial);
            for _ in 0..205 {
                output.pump(None, |_| Ok(0)).unwrap();
            }
            output.pump(Some(later), |src| { bytes.extend_from_slice(src); Ok(src.len()) }).unwrap();
            assert!(!output.pending());
            if split == 0 { assert_eq!(bytes, later.bytes()); }
            else if partial { assert_eq!(bytes, first.bytes()); }
            else { assert_eq!(bytes, [first.bytes(), later.bytes()].concat()); }
        }
    }

    #[test]
    fn terminal_write_errors_propagate() {
        let p = wire::reading(1, 2, 1, 2).unwrap();
        let mut output = ProgressOutput::default();
        assert!(output.pump(Some(p), |src| Ok(src.len()+1)).is_err());
        output.pump(Some(p), |_| Ok(3)).unwrap();
        assert_eq!(output.pump(None, |_| Err("closed")), Err("closed"));
    }
}
