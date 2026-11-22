//! Carries the conversation's requests to the local model service and what the service says back,
//! over a channel that takes and gives bytes when it can. No device here: the caller hands in how to
//! read and how to write, so the same code runs against PCP OS's `dev/talk` and against a test.
//!
//! What this program believes about the service comes only from the channel: no channel, no model
//! (`Absent`); a channel and nothing said yet, a model being loaded (`Loading`); the service's own
//! word for ready or not started; and a channel that has ended or carried anything that is not a
//! record, a service that is gone (`Failed`). Nothing is ever guessed to be ready.
use crate::talk::{Incoming, Request, Service};
use crate::wire::{self, Kind, Packed, Reader, Step};
use crate::Ui;

/// The answer of a read or write that would have to wait (`EAGAIN`).
pub const WOULD_WAIT: isize = -11;
/// The most bytes taken from the channel in one turn of the program's loop, so that a service
/// that talks without end cannot keep the keyboard from being read.
pub const PER_TURN: usize = 4096;

pub struct Link {
    open: bool,
    ended: bool,
    /// The service said it was ready at least once. What its end is called depends on it.
    was_ready: bool,
    reader: Reader,
    sending: Option<(Packed, usize)>,
    /// Records the service sent that only this program may send, or that name nothing being waited
    /// for. Counted; never shown.
    pub refused: u64,
}

impl Link {
    /// `open`: whether PCP OS gave this program a channel at all.
    pub fn new(open: bool, ui: &mut Ui) -> Self {
        ui.talk.service_is(if open { Service::Loading } else { Service::Absent });
        Self { open, ended: false, was_ready: false, reader: Reader::new(), sending: None, refused: 0 }
    }

    fn end(&mut self, ui: &mut Ui) {
        self.ended = true;
        self.sending = None;
        // A service that was answering and is gone has stopped; one that never said it was ready
        // did not start.
        ui.talk.service_is(if self.was_ready { Service::Stopped } else { Service::Failed });
    }

    /// One turn: write what is waiting as far as the channel takes it, read what has arrived.
    /// `read` and `write` answer as PCP OS does: bytes moved, `WOULD_WAIT`, 0 from a read when the
    /// other end is gone, another negative number for a failure. True if anything on the screen
    /// may have changed.
    pub fn turn(&mut self, ui: &mut Ui, mut read: impl FnMut(&mut [u8]) -> isize, mut write: impl FnMut(&[u8]) -> isize) -> bool {
        if !self.open || self.ended {
            return false;
        }
        let mut changed = false;
        // Out: one record at a time, whole. A request leaves the queue only when its last byte went.
        loop {
            if self.sending.is_none() {
                let packed = match ui.for_service() {
                    None => break,
                    Some(Request::Say(message)) => wire::say(&message),
                    Some(Request::Stop { conversation, reply }) => wire::stop(conversation, reply),
                    Some(Request::Forget { conversation }) => wire::forget(conversation),
                };
                match packed {
                    Some(packed) => self.sending = Some((packed, 0)),
                    // The conversation made something that is not a record. It cannot be carried;
                    // saying nothing would leave the person waiting for an answer that cannot come.
                    None => {
                        self.end(ui);
                        return true;
                    }
                }
            }
            let Some((packed, sent)) = self.sending.as_mut() else { break };
            let wrote = write(&packed.bytes()[*sent..]);
            if wrote == WOULD_WAIT {
                break;
            }
            if wrote <= 0 || wrote as usize > packed.bytes().len() - *sent {
                self.end(ui);
                return true;
            }
            *sent += wrote as usize;
            if *sent == packed.bytes().len() {
                self.sending = None;
                ui.service_took();
            }
        }
        // In.
        let mut taken = 0;
        let mut bytes = [0u8; 512];
        while taken < PER_TURN {
            let got = read(&mut bytes);
            if got == WOULD_WAIT {
                break;
            }
            if got <= 0 || got as usize > bytes.len() {
                self.end(ui);
                return true;
            }
            taken += got as usize;
            for byte in &bytes[..got as usize] {
                match self.reader.take(*byte) {
                    Step::More => {}
                    Step::Broken => {
                        self.end(ui);
                        return true;
                    }
                    Step::Whole => {
                        changed = true;
                        let Some(record) = self.reader.record() else { continue };
                        let (conversation, reply) = (record.conversation, record.reply);
                        match record.kind {
                            Kind::Loading => ui.talk.loading(wire::progress(record.body)),
                            Kind::Ready => {
                                self.was_ready = true;
                                ui.model_ready();
                            }
                            Kind::NotStarted => ui.talk.service_is(Service::Failed),
                            Kind::Chunk => {
                                // Bytes that are not text are shown as one question mark each, as
                                // the conversation shows anything it cannot show exactly.
                                let mut rest = record.body;
                                // A piece with no text in it is still the answer beginning: it
                                // ends the reading, and shows nothing.
                                if rest.is_empty() {
                                    ui.observed_talk(Incoming::Chunk { conversation, reply, text: "" });
                                }
                                while !rest.is_empty() {
                                    let (text, skip) = match core::str::from_utf8(rest) {
                                        Ok(text) => (text, 0),
                                        Err(e) => (core::str::from_utf8(&rest[..e.valid_up_to()]).unwrap_or(""), 1),
                                    };
                                    if !text.is_empty() {
                                        ui.observed_talk(Incoming::Chunk { conversation, reply, text });
                                    }
                                    if skip == 1 {
                                        ui.observed_talk(Incoming::Chunk { conversation, reply, text: "?" });
                                    }
                                    rest = &rest[text.len() + skip..];
                                }
                            }
                            Kind::Done => {
                                ui.observed_talk(Incoming::Done { conversation, reply });
                            }
                            Kind::Reading => {
                                // The reader has already refused a body that does not add up.
                                if let Some((read, of)) = wire::progress(record.body) {
                                    ui.observed_talk(Incoming::Reading { conversation, reply, read, of });
                                }
                            }
                            Kind::Failed => {
                                ui.observed_talk(Incoming::Failed { conversation, reply });
                            }
                            // Only this program says these. A service that sends one is not
                            // speaking the agreed records; it is counted, and nothing is shown.
                            Kind::Say | Kind::Stop | Kind::Forget => self.refused += 1,
                        }
                    }
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::talk::{Reply, Who};
    use crate::wire::{pack, Record};
    use crate::{Action, Text};
    use std::collections::VecDeque;
    use std::vec::Vec;

    /// A channel as PCP OS gives it: so many bytes of room, bytes waiting, and whether the other end is there.
    struct Channel {
        to_service: Vec<u8>,
        room: usize,
        from_service: VecDeque<u8>,
        service_gone: bool,
    }

    impl Channel {
        fn new() -> Self {
            Self { to_service: Vec::new(), room: 65_536, from_service: VecDeque::new(), service_gone: false }
        }
        fn says(&mut self, kind: Kind, conversation: u64, reply: u64, body: &[u8]) {
            self.from_service.extend(pack(Record { kind, history: false, conversation, reply, body }).unwrap().bytes());
        }
    }

    fn turn(link: &mut Link, ui: &mut Ui, channel: &mut Channel) -> bool {
        let Channel { to_service, room, from_service, service_gone } = channel;
        link.turn(ui, |into| {
            if from_service.is_empty() {
                return if *service_gone { 0 } else { WOULD_WAIT };
            }
            // A few bytes at a time, as a channel may give them.
            let count = into.len().min(from_service.len()).min(5);
            for slot in &mut into[..count] {
                *slot = from_service.pop_front().unwrap();
            }
            count as isize
        }, |from| {
            if *service_gone {
                return -32;
            }
            let count = from.len().min(*room);
            if count == 0 {
                return WOULD_WAIT;
            }
            to_service.extend_from_slice(&from[..count]);
            *room -= count;
            count as isize
        })
    }

    fn typed(ui: &mut Ui, words: &str) {
        ui.input = Text::new();
        for b in words.bytes() {
            assert!(ui.input.push_ascii(b));
        }
    }

    #[test]
    fn with_no_channel_there_is_no_model_and_nothing_is_read_or_written() {
        let mut ui = Ui::new();
        let mut link = Link::new(false, &mut ui);
        assert_eq!(ui.talk.service, Service::Absent);
        assert!(!link.turn(&mut ui, |_| panic!("read"), |_| panic!("write")));
    }

    #[test]
    fn a_channel_alone_is_a_model_loading_until_the_service_says_ready() {
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut channel = Channel::new();
        assert_eq!(ui.talk.service, Service::Loading);
        assert!(!turn(&mut link, &mut ui, &mut channel));
        assert_eq!(ui.talk.service, Service::Loading);
        channel.says(Kind::Ready, 0, 0, b"");
        assert!(turn(&mut link, &mut ui, &mut channel));
        assert_eq!(ui.talk.service, Service::Ready);
    }

    #[test]
    fn how_far_a_check_has_got_is_kept_while_loading_and_is_never_ready() {
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut channel = Channel::new();
        let body = |checked: u64, of: u64| [checked.to_le_bytes(), of.to_le_bytes()].concat();
        channel.says(Kind::Loading, 0, 0, &body(1 << 30, 5 << 30));
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        assert_eq!((ui.talk.service, ui.talk.checked), (Service::Loading, Some((1 << 30, 5 << 30))));
        // All of it checked is still not ready: a message is not sent.
        channel.says(Kind::Loading, 0, 0, &body(5 << 30, 5 << 30));
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        typed(&mut ui, "hello");
        let _ = ui.intent(Action::Submit);
        assert!(ui.for_service().is_none());
        assert_eq!((ui.talk.service, ui.notice.as_str()), (Service::Loading, "MODEL STILL LOADING - NOT SENT"));
        channel.says(Kind::Loading, 0, 0, b"");
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        assert_eq!(ui.talk.checked, None);
        channel.says(Kind::Ready, 0, 0, b"");
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        assert_eq!((ui.talk.service, ui.talk.checked), (Service::Ready, None));
    }

    #[test]
    fn a_message_goes_out_whole_and_the_answer_comes_back_into_the_conversation() {
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut channel = Channel::new();
        channel.says(Kind::Ready, 0, 0, b"");
        turn(&mut link, &mut ui, &mut channel);
        typed(&mut ui, "What can you do?");
        let _ = ui.intent(Action::Submit);
        // The channel has room for only part of the record at first.
        channel.room = 10;
        turn(&mut link, &mut ui, &mut channel);
        assert_eq!(channel.to_service.len(), 10);
        assert!(ui.for_service().is_some(), "a request leaves only when all of it was written");
        channel.room = 65_536;
        turn(&mut link, &mut ui, &mut channel);
        assert!(ui.for_service().is_none());
        let mut reader = Reader::new();
        let steps: Vec<Step> = channel.to_service.iter().map(|b| reader.take(*b)).collect();
        assert_eq!(steps.iter().filter(|s| **s == Step::Whole).count(), 1);
        let sent = reader.record().unwrap();
        assert_eq!((sent.kind, sent.history, sent.conversation, sent.reply, sent.body), (Kind::Say, true, 1, 1, &b"What can you do?"[..]));
        channel.says(Kind::Chunk, 1, 1, b"I can keep ");
        channel.says(Kind::Chunk, 1, 1, b"notes.");
        channel.says(Kind::Done, 1, 1, b"");
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        let last = ui.talk.turns().last().unwrap();
        assert_eq!((last.who, last.text.as_str(), last.state), (Who::Taylor, "I can keep notes.", Reply::Complete));
        assert!(!ui.talk.answering());
    }

    /// Transport success is not a model-acceptance receipt. Never show "sent" merely because
    /// the local UI admitted a Say request, including when zero bytes could leave the channel.
    #[test]
    fn queued_say_never_claims_delivery_under_zero_write_partial_write_or_completed_write() {
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut channel = Channel::new();
        channel.says(Kind::Ready, 0, 0, b"");
        turn(&mut link, &mut ui, &mut channel);
        typed(&mut ui, "hello");
        let _ = ui.intent(Action::Submit);
        const QUEUED: &str = "QUEUED LOCALLY - DELIVERY UNCONFIRMED";
        assert_eq!(ui.notice.as_str(), QUEUED);
        assert!(matches!(ui.for_service(), Some(Request::Say(_))));
        channel.room = 0;
        turn(&mut link, &mut ui, &mut channel);
        assert!(channel.to_service.is_empty(), "EAGAIN did not write any Say bytes");
        assert!(ui.for_service().is_some());
        assert_eq!(ui.notice.as_str(), QUEUED);

        channel.room = 5;
        turn(&mut link, &mut ui, &mut channel);
        assert_eq!(channel.to_service.len(), 5, "only a partial frame was written");
        assert!(ui.for_service().is_some());
        assert_eq!(ui.notice.as_str(), QUEUED);

        channel.room = 65_536;
        turn(&mut link, &mut ui, &mut channel);
        assert!(ui.for_service().is_none(), "complete frame leaves the local queue");
        assert_eq!(ui.notice.as_str(), QUEUED, "channel write is not service acceptance");
        // No model reply or correlated acceptance has arrived in this test.
        assert!(ui.talk.answering());
    }


    /// A channel reply with the same conversation/reply ID may justify retiring
    /// the stale "queued" wording, but is not model identity or delivery proof.
    /// Wrong IDs, even if well-formed, must leave the locally queued warning.
    #[test]
    fn only_a_matching_reply_record_retires_the_queued_notice() {
        const QUEUED: &str = "QUEUED LOCALLY - DELIVERY UNCONFIRMED";
        const SEEN: &str = "MATCHING REPLY RECORD - MODEL UNVERIFIED";
        for kind in [Kind::Reading, Kind::Chunk, Kind::Done, Kind::Failed] {
            let mut ui = Ui::new();
            let mut link = Link::new(true, &mut ui);
            let mut channel = Channel::new();
            channel.says(Kind::Ready, 0, 0, b"");
            turn(&mut link, &mut ui, &mut channel);
            typed(&mut ui, "hello");
            let _ = ui.intent(Action::Submit);
            assert_eq!(ui.notice.as_str(), QUEUED);
            channel.room = 0;
            turn(&mut link, &mut ui, &mut channel);
            assert!(channel.to_service.is_empty());
            assert!(matches!(ui.for_service(), Some(Request::Say(_))));

            channel.says(Kind::Chunk, 1, 99, b"wrong reply");
            channel.says(Kind::Done, 99, 1, b"");
            while !channel.from_service.is_empty() {
                turn(&mut link, &mut ui, &mut channel);
            }
            assert_eq!(ui.notice.as_str(), QUEUED);
            // A bare READY frame has no reply correlation.
            channel.says(Kind::Ready, 0, 0, b"");
            turn(&mut link, &mut ui, &mut channel);
            assert_eq!(ui.notice.as_str(), QUEUED);
            match kind {
                Kind::Reading => channel.from_service.extend(wire::reading(1, 1, 1, 3).unwrap().bytes()),
                Kind::Chunk => channel.says(Kind::Chunk, 1, 1, b"reply"),
                Kind::Done => channel.says(Kind::Done, 1, 1, b""),
                Kind::Failed => channel.says(Kind::Failed, 1, 1, b""),
                _ => unreachable!(),
            }
            while !channel.from_service.is_empty() {
                turn(&mut link, &mut ui, &mut channel);
            }
            assert_eq!(ui.notice.as_str(), SEEN);
            assert!(channel.to_service.is_empty(), "a test frame is not an actual model admission");
            assert!(matches!(ui.for_service(), Some(Request::Say(_))),
                "the local SAY is still unwritten; do not call this delivery");
        }
    }

    #[test]
    fn old_chat_reply_does_not_retire_new_chat_queued_warning() {
        const QUEUED: &str = "QUEUED LOCALLY - DELIVERY UNCONFIRMED";
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut channel = Channel::new();
        channel.says(Kind::Ready, 0, 0, b"");
        turn(&mut link, &mut ui, &mut channel);
        channel.room = 0;
        typed(&mut ui, "old");
        let _ = ui.intent(Action::Submit);
        assert!(ui.new_conversation());
        typed(&mut ui, "new");
        let _ = ui.intent(Action::Submit);
        assert_eq!(ui.notice.as_str(), QUEUED);
        channel.says(Kind::Chunk, 1, 1, b"late");
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        assert_eq!(ui.notice.as_str(), QUEUED);
        channel.says(Kind::Chunk, 2, 2, b"current");
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        assert_eq!(ui.notice.as_str(), "MATCHING REPLY RECORD - MODEL UNVERIFIED");
    }

    #[test]
    fn the_message_asks_for_reading_progress_and_what_the_service_says_of_it_is_kept_until_the_answer_begins() {
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut channel = Channel::new();
        channel.says(Kind::Ready, 0, 0, b"");
        turn(&mut link, &mut ui, &mut channel);
        typed(&mut ui, "hello");
        let _ = ui.intent(Action::Submit);
        turn(&mut link, &mut ui, &mut channel);
        // The record that left carries the request for progress.
        assert_eq!(channel.to_service[4] & wire::PROGRESS_WANTED, wire::PROGRESS_WANTED);
        channel.from_service.extend(wire::reading(1, 1, 12, 90).unwrap().bytes());
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        assert_eq!(ui.talk.reading, Some((12, 90)));
        channel.says(Kind::Chunk, 1, 1, b"Hi");
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        assert_eq!(ui.talk.reading, None);
        assert_eq!(ui.talk.turns().last().unwrap().text.as_str(), "Hi");
    }

    /// C3's sequence: ready, a message, reading 1 of 3, a piece of the answer with no text in it,
    /// reading 2 of 3. The empty piece is the answer beginning; the later reading is a stray.
    #[test]
    fn a_first_piece_with_no_text_ends_the_reading_and_later_reading_does_not_bring_it_back() {
        for (first, shown) in [(&b""[..], ""), (&b"H"[..], "H")] {
            let mut ui = Ui::new();
            let mut link = Link::new(true, &mut ui);
            let mut channel = Channel::new();
            channel.says(Kind::Ready, 0, 0, b"");
            turn(&mut link, &mut ui, &mut channel);
            typed(&mut ui, "hello");
            let _ = ui.intent(Action::Submit);
            turn(&mut link, &mut ui, &mut channel);
            let mut run = |channel: &mut Channel, ui: &mut Ui, link: &mut Link| while !channel.from_service.is_empty() {
                turn(link, ui, channel);
            };
            channel.from_service.extend(wire::reading(1, 1, 1, 3).unwrap().bytes());
            run(&mut channel, &mut ui, &mut link);
            assert_eq!(ui.talk.reading, Some((1, 3)));
            channel.says(Kind::Chunk, 1, 1, first);
            run(&mut channel, &mut ui, &mut link);
            assert_eq!(ui.talk.reading, None, "after a first piece {first:?}");
            let strays = ui.talk.strays;
            channel.from_service.extend(wire::reading(1, 1, 2, 3).unwrap().bytes());
            run(&mut channel, &mut ui, &mut link);
            assert_eq!((ui.talk.reading, ui.talk.strays), (None, strays + 1), "reading after a first piece {first:?}");
            assert_eq!(ui.talk.turns().last().unwrap().text.as_str(), shown);
            assert!(ui.talk.answering());
        }
    }

    #[test]
    fn stop_is_carried_and_what_arrives_afterwards_is_not_shown() {
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut channel = Channel::new();
        channel.says(Kind::Ready, 0, 0, b"");
        turn(&mut link, &mut ui, &mut channel);
        typed(&mut ui, "go on");
        let _ = ui.intent(Action::Submit);
        turn(&mut link, &mut ui, &mut channel);
        channel.says(Kind::Chunk, 1, 1, b"First");
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        channel.to_service.clear();
        let _ = ui.intent(Action::Stop);
        turn(&mut link, &mut ui, &mut channel);
        let mut reader = Reader::new();
        for b in &channel.to_service {
            reader.take(*b);
        }
        let sent = reader.record().unwrap();
        assert_eq!((sent.kind, sent.conversation, sent.reply), (Kind::Stop, 1, 1));
        channel.says(Kind::Chunk, 1, 1, b" second");
        channel.says(Kind::Done, 1, 1, b"");
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        let last = ui.talk.turns().last().unwrap();
        assert_eq!((last.text.as_str(), last.state), ("First", Reply::Cancelled));
        assert_eq!(ui.talk.strays, 2);
    }

    #[test]
    fn a_service_that_goes_away_fails_the_answer_under_way_and_is_not_asked_again() {
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut channel = Channel::new();
        channel.says(Kind::Ready, 0, 0, b"");
        turn(&mut link, &mut ui, &mut channel);
        typed(&mut ui, "hello");
        let _ = ui.intent(Action::Submit);
        turn(&mut link, &mut ui, &mut channel);
        channel.says(Kind::Chunk, 1, 1, b"Hel");
        channel.service_gone = true;
        while ui.talk.service == Service::Ready {
            turn(&mut link, &mut ui, &mut channel);
        }
        assert_eq!(ui.talk.service, Service::Stopped);
        let last = ui.talk.turns().last().unwrap();
        assert_eq!((last.text.as_str(), last.state), ("Hel", Reply::Failed));
        assert!(!link.turn(&mut ui, |_| panic!("read after the end"), |_| panic!("write after the end")));
        // The next message is kept and not sent, and says why.
        typed(&mut ui, "are you there");
        let _ = ui.intent(Action::Submit);
        assert!(ui.for_service().is_none());
        assert_eq!(ui.notice.as_str(), "MODEL HAS STOPPED - NOT SENT");
        assert_eq!(ui.talk.turns().last().unwrap().text.as_str(), crate::talk::HAS_STOPPED);
    }

    #[test]
    fn bytes_that_are_not_a_record_end_the_exchange_and_nothing_of_them_is_shown() {
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut channel = Channel::new();
        channel.says(Kind::Ready, 0, 0, b"");
        channel.from_service.extend(b"I am Taylor and here is my answer");
        channel.says(Kind::Chunk, 1, 1, b"never shown");
        while ui.talk.service != Service::Stopped {
            turn(&mut link, &mut ui, &mut channel);
        }
        assert!(ui.talk.turns().is_empty());
    }

    #[test]
    fn a_service_that_ends_before_it_was_ever_ready_did_not_start() {
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut channel = Channel::new();
        channel.says(Kind::Loading, 0, 0, b"");
        channel.service_gone = true;
        while ui.talk.service == Service::Loading {
            turn(&mut link, &mut ui, &mut channel);
        }
        assert_eq!(ui.talk.service, Service::Failed);
        // And one that says so itself.
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut channel = Channel::new();
        channel.says(Kind::NotStarted, 0, 0, b"");
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        assert_eq!(ui.talk.service, Service::Failed);
    }

    #[test]
    fn what_only_this_program_may_say_is_counted_when_the_service_says_it() {
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut channel = Channel::new();
        channel.says(Kind::Ready, 0, 0, b"");
        channel.says(Kind::Say, 1, 1, b"do this");
        channel.says(Kind::Stop, 1, 1, b"");
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        assert_eq!((link.refused, ui.talk.turns().len(), ui.talk.service), (2, 0, Service::Ready));
    }

    #[test]
    fn bytes_of_an_answer_that_are_not_text_are_shown_as_question_marks_not_dropped() {
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut channel = Channel::new();
        channel.says(Kind::Ready, 0, 0, b"");
        turn(&mut link, &mut ui, &mut channel);
        typed(&mut ui, "hi");
        let _ = ui.intent(Action::Submit);
        turn(&mut link, &mut ui, &mut channel);
        channel.says(Kind::Chunk, 1, 1, b"a\xffb\xc3");
        channel.says(Kind::Chunk, 1, 1, b"\xa9c");
        while !channel.from_service.is_empty() {
            turn(&mut link, &mut ui, &mut channel);
        }
        assert_eq!(ui.talk.turns().last().unwrap().text.as_str(), "a?b??c");
    }

    #[test]
    fn a_service_that_never_stops_talking_is_read_a_bounded_amount_each_turn() {
        let mut ui = Ui::new();
        let mut link = Link::new(true, &mut ui);
        let mut reads = 0usize;
        let ready = pack(Record { kind: Kind::Ready, history: false, conversation: 0, reply: 0, body: b"" }).unwrap();
        let mut at = 0usize;
        link.turn(&mut ui, |into| {
            reads += 1;
            for slot in into.iter_mut() {
                *slot = ready.bytes()[at % wire::HEADER];
                at += 1;
            }
            into.len() as isize
        }, |_| WOULD_WAIT);
        assert_eq!(reads, PER_TURN / 512);
        assert_eq!(ui.talk.service, Service::Ready);
    }
}
