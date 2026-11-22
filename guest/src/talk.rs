//! Talking with Taylor: the conversation itself, and the two records that cross between this program
//! and a local model service. No I/O here and no task: talk never makes a file job, a file or a
//! question to PCP OS. Every turn says who really produced it; nothing is shown as Taylor's that a
//! model service did not send for this conversation and this reply.
use crate::Text;

/// The local model service, as far as this program knows. Apart from any one reply's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    /// This PCP OS carries no model, or none has been found. The usual state without a model.
    Absent,
    /// A model is being read into memory. Messages wait; nothing is answered yet.
    Loading,
    Ready,
    /// A model was expected and could not be started. Distinct from Absent: something is wrong.
    Failed,
    /// A model was running and is gone. It is started again when Tailorbird is next opened.
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Person,
    Taylor,
    /// Said by this program about the state of things. Never Taylor's words.
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    /// Not a reply (a person's or a system turn).
    None,
    Streaming,
    Complete,
    /// The person stopped it. What had arrived stays, marked as stopped.
    Cancelled,
    Failed,
}

pub const TURN: usize = 480;
pub const TURNS: usize = 24;
pub const NO_MODEL: &str = "No model is running on this computer, so Taylor cannot answer yet.";
pub const STILL_LOADING: &str = "Taylor's model is still loading. Your message was not sent; send it again when ready.";
pub const NOT_STARTED: &str = "Taylor's model could not be started, so Taylor cannot answer.";
pub const HAS_STOPPED: &str = "Taylor's model has stopped, so Taylor cannot answer. Leaving and opening Tailorbird starts it again.";
pub const TOO_LONG: &str = "That message is longer than can be sent at once, so none of it was sent.";
pub const BUSY: &str = "Taylor is still answering. Stop that answer first, or wait.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Turn {
    pub who: Who,
    pub text: Text<TURN>,
    /// More was said than a turn keeps; the text is its beginning.
    pub cut: bool,
    /// Which reply of the conversation this is (0 for turns that are not replies).
    pub reply: u64,
    pub state: Reply,
    /// For a person's turn: whether it was handed to the model service.
    pub sent: bool,
}

/// What this program hands the model service: one message of one conversation, with the number the
/// reply must carry. `history` says whether the service is to answer with the conversation so far in
/// mind; the conversation is kept on the screen either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outgoing {
    pub conversation: u64,
    pub reply: u64,
    pub history: bool,
    pub text: Text<{ crate::INPUT_LIMIT }>,
}

/// What the model service hands back. Each names the conversation and reply it belongs to; one that
/// names another is late or stray, and is counted and never shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Incoming<'a> {
    Chunk { conversation: u64, reply: u64, text: &'a str },
    Done { conversation: u64, reply: u64 },
    Failed { conversation: u64, reply: u64 },
}

/// Something this program has to tell the model service. Made by the conversation, carried by
/// whoever holds the channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Say(Outgoing),
    /// The person stopped this reply.
    Stop { conversation: u64, reply: u64 },
}

/// Requests waiting to be carried. Four is more than one press can make; a fifth is refused, and
/// the caller says so instead of losing it.
pub const WAITING: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taken {
    Shown,
    /// It named a conversation or reply that is not the one being waited for.
    Stray,
}

pub struct Talk {
    pub service: Service,
    /// Changes with every new chat, so nothing of an earlier one can arrive in a later one.
    conversation: u64,
    next_reply: u64,
    turns: [Turn; TURNS],
    count: usize,
    /// Turns that scrolled out of what is kept (the oldest first). Counted and said, not hidden.
    pub dropped: usize,
    /// The reply being waited for or received.
    pending: Option<u64>,
    pub strays: u64,
}

const EMPTY: Turn = Turn { who: Who::System, text: Text::new(), cut: false, reply: 0, state: Reply::None, sent: false };

impl Talk {
    pub const fn new() -> Self {
        Self { service: Service::Absent, conversation: 1, next_reply: 1, turns: [EMPTY; TURNS], count: 0, dropped: 0, pending: None, strays: 0 }
    }

    pub fn turns(&self) -> &[Turn] { &self.turns[..self.count] }
    pub fn conversation(&self) -> u64 { self.conversation }
    pub fn answering(&self) -> bool { self.pending.is_some() }
    pub fn is_empty(&self) -> bool { self.count == 0 && self.dropped == 0 }

    fn push(&mut self, who: Who, words: &str, reply: u64, state: Reply, sent: bool) {
        if self.count == TURNS {
            // The oldest turn makes room, except the answer still under way: that one is kept however
            // far the conversation rolls on, so that what arrives for it, and its end, still find it.
            let active = |t: &Turn| self.pending.is_some_and(|reply| t.who == Who::Taylor && t.reply == reply && t.state == Reply::Streaming);
            let oldest = if active(&self.turns[0]) { 1 } else { 0 };
            self.turns.copy_within(oldest + 1.., oldest);
            self.count -= 1;
            self.dropped += 1;
        }
        let mut text = Text::<TURN>::new();
        let mut cut = false;
        for b in words.bytes() {
            // What cannot be shown exactly is shown as a question mark, never dropped silently.
            let shown = if (0x20..=0x7e).contains(&b) { b } else if b == b'\n' { b' ' } else { b'?' };
            if !text.push_ascii(shown) {
                cut = true;
                break;
            }
        }
        self.turns[self.count] = Turn { who, text, cut, reply, state, sent };
        self.count += 1;
    }

    /// The person sends what they typed. It is always kept as their turn. With a model ready and no
    /// answer under way it is handed over (the record is returned); otherwise it is not sent, and a
    /// system turn says why in plain words.
    pub fn say(&mut self, words: &str, history: bool) -> Option<Outgoing> {
        if words.is_empty() {
            return None;
        }
        // What crosses to the model service is the whole message or nothing: one longer than the
        // record holds is kept on the screen as far as a turn shows, marked cut, and not sent.
        let why_not = match (self.service, self.pending) {
            _ if words.len() > crate::INPUT_LIMIT => Some(TOO_LONG),
            (Service::Absent, _) => Some(NO_MODEL),
            (Service::Loading, _) => Some(STILL_LOADING),
            (Service::Failed, _) => Some(NOT_STARTED),
            (Service::Stopped, _) => Some(HAS_STOPPED),
            (Service::Ready, Some(_)) => Some(BUSY),
            (Service::Ready, None) => None,
        };
        self.push(Who::Person, words, 0, Reply::None, why_not.is_none());
        if let Some(why) = why_not {
            self.push(Who::System, why, 0, Reply::None, false);
            return None;
        }
        let reply = self.next_reply;
        self.next_reply += 1;
        self.pending = Some(reply);
        self.push(Who::Taylor, "", reply, Reply::Streaming, false);
        Some(Outgoing { conversation: self.conversation, reply, history, text: Text::display(words) })
    }

    fn pending_turn(&mut self, conversation: u64, reply: u64) -> Option<&mut Turn> {
        if conversation != self.conversation || self.pending != Some(reply) {
            return None;
        }
        self.turns[..self.count].iter_mut().rev().find(|t| t.who == Who::Taylor && t.reply == reply && t.state == Reply::Streaming)
    }

    /// Something from the model service. Only what names this conversation and the reply being waited
    /// for is shown; anything else (late, after a stop, from an earlier chat) is counted and dropped.
    pub fn receive(&mut self, incoming: Incoming<'_>) -> Taken {
        let (conversation, reply) = match incoming {
            Incoming::Chunk { conversation, reply, .. } | Incoming::Done { conversation, reply } | Incoming::Failed { conversation, reply } => (conversation, reply),
        };
        let Some(turn) = self.pending_turn(conversation, reply) else {
            self.strays += 1;
            return Taken::Stray;
        };
        match incoming {
            Incoming::Chunk { text, .. } => {
                for b in text.bytes() {
                    let shown = if (0x20..=0x7e).contains(&b) { b } else if b == b'\n' { b' ' } else { b'?' };
                    if !turn.text.push_ascii(shown) {
                        turn.cut = true;
                        break;
                    }
                }
            }
            Incoming::Done { .. } => {
                turn.state = Reply::Complete;
                self.pending = None;
            }
            Incoming::Failed { .. } => {
                turn.state = Reply::Failed;
                self.pending = None;
            }
        }
        Taken::Shown
    }

    /// The person stops the answer under way. What had arrived stays and is marked stopped; anything
    /// that arrives for it afterwards is a stray. Returns the reply stopped, to tell the service.
    pub fn cancel(&mut self) -> Option<(u64, u64)> {
        let reply = self.pending.take()?;
        if let Some(turn) = self.turns[..self.count].iter_mut().rev().find(|t| t.reply == reply && t.state == Reply::Streaming) {
            turn.state = Reply::Cancelled;
        }
        Some((self.conversation, reply))
    }

    /// The service stopped being ready while an answer was under way: that answer failed.
    pub fn service_is(&mut self, service: Service) {
        self.service = service;
        if service != Service::Ready {
            if let Some(reply) = self.pending.take() {
                if let Some(turn) = self.turns[..self.count].iter_mut().rev().find(|t| t.reply == reply && t.state == Reply::Streaming) {
                    turn.state = Reply::Failed;
                }
            }
        }
    }

    /// Start over: nothing of this conversation is kept or can arrive in the next.
    pub fn new_chat(&mut self) {
        self.conversation += 1;
        self.count = 0;
        self.dropped = 0;
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_model_the_message_is_kept_and_the_reason_is_a_system_turn() {
        let mut talk = Talk::new();
        assert!(talk.is_empty());
        for (service, why) in [(Service::Absent, NO_MODEL), (Service::Loading, STILL_LOADING), (Service::Failed, NOT_STARTED), (Service::Stopped, HAS_STOPPED)] {
            talk.service_is(service);
            assert_eq!(talk.say("hello", true), None);
            let turns = talk.turns();
            let (mine, said) = (turns[turns.len() - 2], turns[turns.len() - 1]);
            assert_eq!((mine.who, mine.text.as_str(), mine.sent), (Who::Person, "hello", false));
            assert_eq!((said.who, said.text.as_str(), said.state), (Who::System, why, Reply::None));
        }
        // Nothing here is Taylor's, and nothing is being waited for.
        assert!(talk.turns().iter().all(|t| t.who != Who::Taylor) && !talk.answering());
        assert_eq!(talk.say("", true), None);
        assert_eq!(talk.turns().len(), 8);
    }

    #[test]
    fn a_reply_is_shown_only_for_this_conversation_and_this_reply() {
        let mut talk = Talk::new();
        talk.service_is(Service::Ready);
        let out = talk.say("what is a note?", true).unwrap();
        assert_eq!((out.conversation, out.reply, out.history, out.text.as_str()), (1, 1, true, "what is a note?"));
        assert!(talk.answering());
        // A second message while answering is kept, not sent, and says why.
        assert_eq!(talk.say("and a project?", true), None);
        assert_eq!(talk.turns().last().unwrap().text.as_str(), BUSY);
        // Wrong reply, wrong conversation: stray, nothing shown.
        assert_eq!(talk.receive(Incoming::Chunk { conversation: 1, reply: 2, text: "x" }), Taken::Stray);
        assert_eq!(talk.receive(Incoming::Chunk { conversation: 2, reply: 1, text: "x" }), Taken::Stray);
        assert_eq!(talk.receive(Incoming::Chunk { conversation: 1, reply: 1, text: "A note is " }), Taken::Shown);
        assert_eq!(talk.receive(Incoming::Chunk { conversation: 1, reply: 1, text: "a small file.\nTab\there" }), Taken::Shown);
        assert_eq!(talk.receive(Incoming::Done { conversation: 1, reply: 1 }), Taken::Shown);
        let answer = talk.turns().iter().find(|t| t.who == Who::Taylor).unwrap();
        assert_eq!((answer.text.as_str(), answer.state, answer.cut), ("A note is a small file. Tab?here", Reply::Complete, false));
        assert!(!talk.answering());
        // After it is done, more for the same reply is a stray.
        assert_eq!(talk.receive(Incoming::Chunk { conversation: 1, reply: 1, text: "more" }), Taken::Stray);
        assert_eq!(talk.strays, 3);
    }

    #[test]
    fn a_stopped_answer_keeps_what_came_and_takes_no_more_and_a_new_chat_takes_nothing_old() {
        let mut talk = Talk::new();
        talk.service_is(Service::Ready);
        let first = talk.say("go on", false).unwrap();
        assert!(!first.history);
        talk.receive(Incoming::Chunk { conversation: 1, reply: 1, text: "Well," });
        assert_eq!(talk.cancel(), Some((1, 1)));
        assert_eq!(talk.cancel(), None);
        assert_eq!(talk.receive(Incoming::Chunk { conversation: 1, reply: 1, text: " late" }), Taken::Stray);
        let stopped = *talk.turns().last().unwrap();
        assert_eq!((stopped.text.as_str(), stopped.state), ("Well,", Reply::Cancelled));
        // The next message gets a new reply number; the service failing mid-answer fails that answer.
        let second = talk.say("again", true).unwrap();
        assert_eq!(second.reply, 2);
        talk.service_is(Service::Failed);
        assert_eq!(talk.turns().last().unwrap().state, Reply::Failed);
        assert!(!talk.answering());
        // A new chat: empty, a new conversation number, and nothing addressed to the old one is shown.
        talk.service_is(Service::Ready);
        talk.new_chat();
        assert!(talk.is_empty() && talk.conversation() == 2);
        let third = talk.say("hello", true).unwrap();
        assert_eq!((third.conversation, third.reply), (2, 3));
        assert_eq!(talk.receive(Incoming::Done { conversation: 1, reply: 3 }), Taken::Stray);
        assert!(talk.answering());
    }

    #[test]
    fn an_answer_under_way_is_never_rolled_out_of_the_conversation() {
        // C3's case: with an answer under way, twelve more messages (each kept with its reason) fill
        // the conversation and would have evicted the answer, leaving its chunks and its end as strays
        // and the conversation answering for ever.
        for ending in 0..3 {
            let mut talk = Talk::new();
            talk.service_is(Service::Ready);
            talk.say("first", true).unwrap();
            for _ in 0..TURNS {
                assert_eq!(talk.say("while busy", true), None);
            }
            assert_eq!(talk.turns().len(), TURNS);
            assert!(talk.dropped > 0 && talk.answering());
            assert_eq!(talk.receive(Incoming::Chunk { conversation: 1, reply: 1, text: "still here" }), Taken::Shown);
            let state = match ending {
                0 => {
                    assert_eq!(talk.receive(Incoming::Done { conversation: 1, reply: 1 }), Taken::Shown);
                    Reply::Complete
                }
                1 => {
                    assert_eq!(talk.receive(Incoming::Failed { conversation: 1, reply: 1 }), Taken::Shown);
                    Reply::Failed
                }
                _ => {
                    assert_eq!(talk.cancel(), Some((1, 1)));
                    Reply::Cancelled
                }
            };
            assert!(!talk.answering());
            let answer = talk.turns().iter().find(|t| t.who == Who::Taylor).unwrap();
            assert_eq!((answer.text.as_str(), answer.state), ("still here", state));
            // Once it has ended it is a turn like any other, and rolls out in its turn.
            talk.service_is(Service::Absent);
            for _ in 0..TURNS {
                talk.say("later", true);
            }
            assert!(talk.turns().iter().all(|t| t.who != Who::Taylor));
        }
    }

    #[test]
    fn a_message_too_long_to_send_whole_is_not_sent_in_part() {
        let mut talk = Talk::new();
        talk.service_is(Service::Ready);
        let long = [b'a'; crate::INPUT_LIMIT + 1];
        assert_eq!(talk.say(core::str::from_utf8(&long).unwrap(), true), None);
        let turns = talk.turns();
        assert_eq!((turns[0].who, turns[0].sent, turns[0].text.as_str().len()), (Who::Person, false, crate::INPUT_LIMIT + 1));
        assert_eq!((turns[1].who, turns[1].text.as_str()), (Who::System, TOO_LONG));
        assert!(!talk.answering());
        // Exactly the most is sent whole.
        let out = talk.say(core::str::from_utf8(&long[1..]).unwrap(), true).unwrap();
        assert_eq!(out.text.as_str().len(), crate::INPUT_LIMIT);
    }

    #[test]
    fn long_and_many_turns_are_marked_not_lost_silently() {
        let mut talk = Talk::new();
        talk.service_is(Service::Ready);
        talk.say("long one", true).unwrap();
        let long = [b'a'; TURN + 5];
        talk.receive(Incoming::Chunk { conversation: 1, reply: 1, text: core::str::from_utf8(&long).unwrap() });
        let answer = *talk.turns().last().unwrap();
        assert_eq!((answer.text.as_str().len(), answer.cut), (TURN, true));
        talk.receive(Incoming::Done { conversation: 1, reply: 1 });
        talk.service_is(Service::Absent);
        for _ in 0..TURNS {
            talk.say("x", true);
        }
        assert_eq!(talk.turns().len(), TURNS);
        assert_eq!(talk.dropped, 2 + 2 * TURNS - TURNS);
    }
}
