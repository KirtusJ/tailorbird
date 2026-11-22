//! How often the screen is drawn again for news that is only progress.
//!
//! Drawing the whole screen is not free everywhere: on the first laptop Tailorbird ran on, one
//! picture took about a second to reach the panel, and letters typed meanwhile waited for it. While
//! a model is being read in, the service reports how far it has got many times; each report used to
//! draw the screen again. A person typing does not need each figure. So news that is only progress
//! (the model was loading before it and is loading after it) is drawn at most once in `EVERY` turns
//! of the program's loop. Everything else is drawn at once, as before: a key, a reply, the model
//! becoming ready or failing. Drawing for any reason also shows the latest progress.
//!
//! A turn of the loop sleeps twelve milliseconds and takes longer when it draws, so `EVERY` turns
//! are at least a second and "at most once a second" holds without a clock.

/// Turns of the loop between two pictures drawn for progress alone: 84 sleeps of 12 ms are 1008 ms.
pub const EVERY: u32 = 84;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pace {
    /// Progress has been reported that the screen does not yet show.
    owed: bool,
    /// Turns since the screen was last drawn. Saturates.
    since: u32,
}

impl Pace {
    /// As just after a picture was drawn.
    pub const fn new() -> Self {
        Self { owed: false, since: 0 }
    }

    /// One turn of the loop. `news`: the service said something this turn. `loading`: the model was
    /// loading before the news and still is after it. `other`: something else changed this turn (a
    /// key, a job). Returns whether to draw now.
    pub fn turn(&mut self, news: bool, loading: bool, other: bool) -> bool {
        self.since = self.since.saturating_add(1);
        if news && loading {
            self.owed = true;
        }
        let now = other || (news && !loading) || (self.owed && self.since >= EVERY);
        if now {
            self.owed = false;
            self.since = 0;
        }
        now
    }
}

impl Default for Pace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_or_a_reply_is_drawn_in_the_turn_it_happens() {
        let mut pace = Pace::new();
        assert!(pace.turn(false, false, true), "a key");
        assert!(pace.turn(true, false, false), "news while not loading: a reply, or ready");
        assert!(!pace.turn(false, false, false), "nothing happened");
        assert!(!pace.turn(false, true, false), "loading, and nothing said");
    }

    #[test]
    fn progress_alone_is_drawn_at_most_once_in_every_turns() {
        let mut pace = Pace::new();
        let mut drawn = [0u32; 8];
        let mut count = 0;
        for turn in 1..=(EVERY * 3 + 5) {
            if pace.turn(true, true, false) {
                drawn[count] = turn;
                count += 1;
            }
        }
        assert_eq!(drawn[..count], [EVERY, EVERY * 2, EVERY * 3], "a report every turn gives one picture in EVERY turns");
    }

    #[test]
    fn progress_that_was_not_drawn_is_still_owed_and_is_drawn_when_its_turn_comes() {
        let mut pace = Pace::new();
        assert!(!pace.turn(true, true, false), "the first report is held");
        for _ in 2..EVERY {
            assert!(!pace.turn(false, true, false), "held while nothing else happens");
        }
        assert!(pace.turn(false, true, false), "shown at the end of the wait though nothing new was said");
        assert!(!pace.turn(false, true, false), "and then nothing is owed");
    }

    #[test]
    fn a_key_during_loading_draws_at_once_and_shows_the_progress_too() {
        let mut pace = Pace::new();
        assert!(!pace.turn(true, true, false));
        assert!(pace.turn(false, true, true), "the key is not made to wait");
        for _ in 1..EVERY {
            assert!(!pace.turn(false, true, false), "the progress went out with the key's picture: nothing owed");
        }
        assert!(!pace.turn(false, true, false));
    }

    #[test]
    fn the_end_of_loading_is_not_held() {
        let mut pace = Pace::new();
        assert!(!pace.turn(true, true, false));
        assert!(pace.turn(true, false, false), "ready, failed or stopped is said at once");
    }

    #[test]
    fn a_long_quiet_does_not_overflow_or_draw() {
        let mut pace = Pace { owed: false, since: u32::MAX - 1 };
        assert!(!pace.turn(false, false, false));
        assert!(!pace.turn(false, false, false));
        assert!(pace.turn(true, true, false), "after a long quiet the first progress is drawn at once");
    }
}
