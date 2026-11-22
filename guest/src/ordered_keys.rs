//! Lossless, ordered input reduction for the Tailorbird native guest.
//!
//! The kernel exposes two-byte /dev/kbd events in their original FIFO order.
//! The shared game Display::poll collapses them into pressed/held bitmaps,
//! losing repeated presses and order within a batch. Tailorbird must not
//! use those bitmaps for a text editor. This parser consumes each event in
//! arrival order without altering any kernel or shared guest functionality.
use crate::{exact_ascii, Action, Text, Ui};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyOutcome {
    Unchanged,
    Redraw,
    Exit,
    /// A completed Alt+N chord asks the kernel to review a note.
    MakeNote,
    /// Alt+P turned the project list on or off; when on, the program reads a fresh list.
    Projects,
    /// Alt+G pressed a second time: give up making files for the rest of this launch.
    GiveUp,
}

pub struct OrderedKeyboard {
    held: [bool; 128],
    note_chord: bool,
}
impl OrderedKeyboard {
    pub const fn new() -> Self {
        Self { held: [false; 128], note_chord: false }
    }

    /// Consume one kernel event, as received from the keyboard FIFO.
    ///
    /// code: US-PC set-1 key index; down: true on press/repeat, false on release.
    /// Unsupported/extended keys are not a replacement for a locale-aware IME.
    pub fn event(&mut self, code: usize, down: bool, ui: &mut Ui) -> KeyOutcome {
        // A key that takes back the first Alt+G takes its words off the screen as well, even a key
        // that does nothing else, such as Shift on its own: the screen is drawn again (Director B3
        // saw the prompt stay up after it no longer applied).
        let armed = ui.give_up_armed;
        let outcome = self.key(code, down, ui);
        if armed && !ui.give_up_armed && outcome == KeyOutcome::Unchanged { KeyOutcome::Redraw } else { outcome }
    }

    fn key(&mut self, code: usize, down: bool, ui: &mut Ui) -> KeyOutcome {
        if code >= self.held.len() {
            return KeyOutcome::Unchanged;
        }
        // Preserve typematic repeats for ordinary text, but a repeated
        // Down must never be reinterpreted as a *new* Alt shortcut. This
        // includes a letter held before Alt and repeated afterward.
        let already_down = self.held[code];
        self.held[code] = down;
        // The kernel takes the keyboard while it reviews a note. Consume both
        // releases here so the next editor shortcut starts with accurate state.
        if self.note_chord {
            if code == 49 || code == 56 {
                if !self.held[49] && !self.held[56] {
                    self.note_chord = false;
                    return KeyOutcome::MakeNote;
                }
                return KeyOutcome::Unchanged;
            }
            if down {
                self.note_chord = false;
            }
        }
        if !down {
            return KeyOutcome::Unchanged;
        }
        // Giving up making files takes Alt+G twice with nothing else pressed between: any other
        // fresh key press (not Alt itself, not a repeat) takes the first one back.
        let armed = ui.give_up_armed;
        if !already_down && code != 56 && !(code == 34 && self.held[56]) {
            ui.give_up_armed = false;
        }
        if code == 34 && self.held[56] && !self.held[29] {
            if already_down || self.held[42] || self.held[54] { return KeyOutcome::Unchanged; }
            if !ui.notes || ui.gave_up {
                ui.notice = Text::display(if ui.gave_up { crate::note::GAVE_UP } else { crate::note::NOT_ALLOWED });
                return KeyOutcome::Redraw;
            }
            if armed {
                ui.give_up_armed = false;
                return KeyOutcome::GiveUp;
            }
            ui.give_up_armed = true;
            ui.notice = Text::display(crate::note::GIVE_UP_ASK);
            return KeyOutcome::Redraw;
        }
        if armed && !ui.give_up_armed {
            ui.notice = Text::new();
        }
        // F6 changes presentation once per physical press.
        if code == 64 && !self.held[56] && !self.held[29] && !self.held[42] && !self.held[54] {
            if already_down {
                return KeyOutcome::Unchanged;
            }
            // Even Fit -> Compact -> Fit must invalidate a held click.
            // At saturation, pointer capture stays disabled until a new Ui.
            ui.view_epoch = ui.view_epoch.saturating_add(1);
            ui.compact_view = !ui.compact_view;
            return KeyOutcome::Redraw;
        }
        if code == 1 {
            return KeyOutcome::Exit;
        }
        // Ctrl-modified keypresses are reserved for OS shortcuts or future
        // explicitly implemented editor commands. Neither unsupported
        // Ctrl+letters/backspace nor Ctrl+Enter may mutate the request.
        // In particular Ctrl+Alt+A must not become the UI's Alt+A Allow
        // intent. Key Down remains tracked so a held key cannot suddenly
        // activate when Ctrl is released; release/fresh Down is required.
        if self.held[29] {
            return KeyOutcome::Unchanged;
        }
        if self.held[56] {
            // A permission choice is defined as the exact Alt+A / Alt+D
            // chord. Shift changes the chord; never interpret Shift+Alt+A
            // or Shift+Alt+D as human consent. The held-key state above
            // still requires a genuine release/fresh Down after Shift lifts.
            // Stop and non-consent actions retain their existing mappings.
            if (self.held[42] || self.held[54]) && (code == 30 || code == 32) {
                return KeyOutcome::Unchanged;
            }
            if code == 49 {
                self.note_chord = !already_down && !self.held[42] && !self.held[54];
                return KeyOutcome::Unchanged;
            }
            if code == 46 {
                // Alt+C: a new conversation. Edge-triggered; Shift makes it a different chord.
                if already_down || self.held[42] || self.held[54] { return KeyOutcome::Unchanged; }
                return if ui.new_conversation() { KeyOutcome::Redraw } else { KeyOutcome::Unchanged };
            }
            if code == 25 {
                // Edge-triggered like every other command; Shift makes it a different chord.
                if already_down || self.held[42] || self.held[54] { return KeyOutcome::Unchanged; }
                ui.show_projects = !ui.show_projects;
                ui.browse_level = 0;
                ui.place = 0;
                ui.top = 0;
                return KeyOutcome::Projects;
            }
            let action = match code {
                50 => Some(Action::ToggleMode),
                19 => Some(Action::RetrySubmit),
                31 => Some(Action::Stop),
                30 => Some(Action::Approve),
                32 => Some(Action::Deny),
                _ => None,
            };
            if let Some(a) = action {
                // Commands are edge-triggered. A held text key followed by
                // Alt and an auto-repeat is *not* an explicit permission
                // decision, Stop, mode switch or retry. A release and fresh
                // press while Alt is held is permitted.
                if already_down { return KeyOutcome::Unchanged; }
                let _ = ui.intent(a);
                return KeyOutcome::Redraw;
            }
            return KeyOutcome::Unchanged;
        }
        if ui.show_projects && matches!(code, 72 | 80 | 73 | 81 | 75 | 28) {
            // In the list the arrows choose, Page Up and Page Down move a screenful, Enter opens and Left
            // goes back. Each press is one step; a held key does not run through the list or open what
            // appears under it. The rows are a window over the whole list: moving past its edge moves
            // the window, and the list is read again.
            if already_down { return KeyOutcome::Unchanged; }
            let rows = crate::projects::ROWS;
            let (place, top, shown, total) = match ui.browse_level {
                0 => (&mut ui.place, &mut ui.top, ui.projects.shown, ui.projects.projects),
                1 => (&mut ui.file_place, &mut ui.file_top, ui.files.shown, ui.files.count),
                _ => (&mut ui.file_place, &mut ui.file_top, 0, 0),
            };
            match code {
                72 if *place > 0 && shown > 0 => *place -= 1,
                72 if *top > 0 && shown > 0 => {
                    *top -= 1;
                    return KeyOutcome::Projects;
                }
                80 if *place + 1 < shown => *place += 1,
                80 if shown > 0 && *top + shown < total => {
                    *top += 1;
                    return KeyOutcome::Projects;
                }
                73 if *top > 0 => {
                    *top = top.saturating_sub(rows);
                    *place = 0;
                    return KeyOutcome::Projects;
                }
                81 if *top + shown < total => {
                    *top += rows;
                    *place = 0;
                    return KeyOutcome::Projects;
                }
                75 if ui.browse_level > 0 => {
                    ui.browse_level -= 1;
                    // Going back reads the list again: it may have changed.
                    return KeyOutcome::Projects;
                }
                28 if ui.browse_level < 2 && *place < shown => {
                    ui.browse_level += 1;
                    if ui.browse_level == 1 {
                        ui.file_place = 0;
                        ui.file_top = 0;
                    }
                    return KeyOutcome::Projects;
                }
                _ => return KeyOutcome::Unchanged,
            }
            return KeyOutcome::Redraw;
        }
        if code == 28 {
            // Submit is an admission request, not ordinary text. A held
            // Enter can outlive a service refusal or an offline-to-online
            // transition; typematic must not mint a new key or turn an
            // earlier unavailable action into consent. Require release
            // and a genuinely new Down, as for Alt permission shortcuts.
            if already_down { return KeyOutcome::Unchanged; }
            let _ = ui.intent(Action::Submit);
            return KeyOutcome::Redraw;
        }
        if ui.pending_submit.is_some() {
            // Keep the visible original prompt identical to the snapshot
            // that any explicit same-key retry would resend.
            return KeyOutcome::Unchanged;
        }
        if code == 14 {
            return if ui.input.pop() {
                KeyOutcome::Redraw
            } else {
                KeyOutcome::Unchanged
            };
        }
        let shift = self.held[42] || self.held[54];
        if let Some(ch) = exact_ascii::key_from_set1(code, shift) {
            return if ui.input.push_ascii(ch) {
                KeyOutcome::Redraw
            } else {
                KeyOutcome::Unchanged
            };
        }
        KeyOutcome::Unchanged
    }
}
impl Default for OrderedKeyboard {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{exact_ascii, BackendEvent, Mode, Model};
    use std::vec;

    fn stroke(keys: &mut OrderedKeyboard, ui: &mut Ui, code: usize) {
        keys.event(code, true, ui);
        keys.event(code, false, ui);
    }

    #[test]
    fn the_rows_are_a_window_that_reaches_every_item() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        ui.show_projects = true;
        ui.projects.projects = 9;
        ui.projects.shown = crate::projects::ROWS;
        let mut press = |keys: &mut OrderedKeyboard, ui: &mut Ui, code: usize| {
            let outcome = keys.event(code, true, ui);
            keys.event(code, false, ui);
            outcome
        };
        // Down the six rows redraws; the next Down moves the window and asks for a fresh list.
        for _ in 0..crate::projects::ROWS - 1 {
            assert_eq!(press(&mut keys, &mut ui, 80), KeyOutcome::Redraw);
        }
        assert_eq!((press(&mut keys, &mut ui, 80), ui.top, ui.place), (KeyOutcome::Projects, 1, crate::projects::ROWS - 1));
        // At the end of the list nothing more happens.
        ui.top = 3;
        assert_eq!(press(&mut keys, &mut ui, 80), KeyOutcome::Unchanged);
        assert_eq!(press(&mut keys, &mut ui, 81), KeyOutcome::Unchanged);
        // Page Up goes back a screenful, never before the start; Up at the top row moves the window.
        assert_eq!((press(&mut keys, &mut ui, 73), ui.top, ui.place), (KeyOutcome::Projects, 0, 0));
        assert_eq!(press(&mut keys, &mut ui, 72), KeyOutcome::Unchanged);
        assert_eq!((press(&mut keys, &mut ui, 81), ui.top), (KeyOutcome::Projects, crate::projects::ROWS));
        ui.projects.shown = 3;
        assert_eq!((press(&mut keys, &mut ui, 72), ui.top), (KeyOutcome::Projects, crate::projects::ROWS - 1));
        // A held key is not a second press.
        assert_eq!(keys.event(72, true, &mut ui), KeyOutcome::Projects);
        assert_eq!(keys.event(72, true, &mut ui), KeyOutcome::Unchanged);
        assert!(ui.input.as_str().is_empty());
    }

    #[test]
    fn consecutive_note_shortcuts_finish_on_release_in_either_order() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        for (first, last) in [(49, 56), (56, 49)] {
            for (code, down) in [(56, true), (49, true), (49, true), (first, false)] {
                assert_eq!(keys.event(code, down, &mut ui), KeyOutcome::Unchanged);
            }
            assert_eq!(keys.event(last, false, &mut ui), KeyOutcome::MakeNote);
            assert_eq!(keys.event(last, false, &mut ui), KeyOutcome::Unchanged);
        }
        assert!(ui.input.as_str().is_empty());
    }

    #[test]
    fn another_key_cancels_an_unfinished_note_shortcut() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        for (code, down) in [(56, true), (49, true), (42, true), (42, false),
                             (49, false), (56, false)] {
            assert_eq!(keys.event(code, down, &mut ui), KeyOutcome::Unchanged);
        }
        assert!(ui.input.as_str().is_empty());
    }

    #[test]
    fn f6_preserves_input_and_requires_a_fresh_unmodified_press() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        stroke(&mut keys, &mut ui, 30);
        assert_eq!(keys.event(64, true, &mut ui), KeyOutcome::Redraw);
        assert!(ui.compact_view);
        assert_eq!(keys.event(64, true, &mut ui), KeyOutcome::Unchanged);
        keys.event(64, false, &mut ui);
        for modifier in [29, 42, 54, 56] {
            keys.event(modifier, true, &mut ui);
            keys.event(64, true, &mut ui);
            keys.event(modifier, false, &mut ui);
            keys.event(64, true, &mut ui);
            keys.event(64, false, &mut ui);
            assert!(ui.compact_view);
        }
        stroke(&mut keys, &mut ui, 64);
        assert!(!ui.compact_view);
        assert_eq!(ui.input.as_str(), "a");
        assert!(ui.pending_submit.is_none());
    }

    #[test]
    fn exact_pt_batch_sequence_keeps_repeated_keys_and_real_shift_order() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        // One poll could collect this entire event batch.
        stroke(&mut keys, &mut ui, 44); // z
        stroke(&mut keys, &mut ui, 45); // x
        stroke(&mut keys, &mut ui, 44); // z again
        keys.event(42, true, &mut ui); // Shift down
        stroke(&mut keys, &mut ui, 30); // A
        keys.event(42, false, &mut ui); // Shift up
        stroke(&mut keys, &mut ui, 30); // a
        assert_eq!(ui.input.as_str(), "zxzAa");

        // The native 960x540 renderer must draw the same ordered text
        // rather than accepting a visually re-sorted set of keys.
        let mut actual = vec![0x10283f; 960 * 540];
        let mut expected = vec![0x10283f; 960 * 540];
        exact_ascii::draw_request(&mut actual, 960, 540, 960, ui.input.as_str(), 0xf2f7fc).unwrap();
        exact_ascii::draw_request(&mut expected, 960, 540, 960, "zxzAa", 0xf2f7fc).unwrap();
        assert_eq!(actual, expected);
        let mut collapsed = vec![0x10283f; 960 * 540];
        exact_ascii::draw_request(&mut collapsed, 960, 540, 960, "azx", 0xf2f7fc).unwrap();
        assert_ne!(actual, collapsed);
    }

    #[test]
    fn order_is_not_numeric_scancode_order() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        stroke(&mut keys, &mut ui, 44);
        stroke(&mut keys, &mut ui, 30);
        stroke(&mut keys, &mut ui, 44);
        assert_eq!(ui.input.as_str(), "zaz");
    }

    #[test]
    fn modifiers_act_at_event_time_and_repeats_are_preserved() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        keys.event(42, true, &mut ui);
        assert_eq!(keys.event(30, true, &mut ui), KeyOutcome::Redraw);
        assert_eq!(keys.event(30, true, &mut ui), KeyOutcome::Redraw);
        keys.event(30, false, &mut ui);
        keys.event(42, false, &mut ui);
        stroke(&mut keys, &mut ui, 30);
        assert_eq!(ui.input.as_str(), "AAa");
    }

    #[test]
    fn keyboard_double_enter_does_not_emit_second_submission() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        assert!(ui.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        stroke(&mut keys, &mut ui, 30);
        stroke(&mut keys, &mut ui, 28);
        assert_eq!(ui.pending_submit.unwrap().idempotency_key, 1);
        stroke(&mut keys, &mut ui, 28);
        assert_eq!(ui.pending_submit.unwrap().idempotency_key, 1);
        assert!(!ui.controls().submit_enabled);
        assert_eq!(ui.input.as_str(), "a");
        stroke(&mut keys, &mut ui, 45);
        assert_eq!(ui.input.as_str(), "a"); // frozen during uncertain admission
        assert!(ui.observe(BackendEvent::SubmissionStarted {
            idempotency_key: 1, task_id: 7, seq: 1,
        }));
        assert!(ui.pending_submit.is_none());
    }

    #[test]
    fn typematic_letter_held_before_alt_cannot_become_permission_decision() {
        // A held printable key may auto-repeat after Alt is pressed. This is
        // not a fresh explicit Alt+A/Alt+D permission choice.
        for (key, allow) in [(30, true), (32, false)] {
            let mut keys = OrderedKeyboard::new();
            let mut ui = Ui::new();
            assert!(ui.observe(BackendEvent::Service {
                connected: true, model: Model::Ready, saved_mode: Mode::Single,
            }));
            assert!(ui.observe(BackendEvent::Started { task_id: 7, seq: 1 }));
            assert!(ui.observe(BackendEvent::PermissionNeeded {
                task_id: 7, seq: 2, request: 11,
                action: "WRITE", target: "/home/a.txt",
            }));
            assert_eq!(keys.event(key, true, &mut ui), KeyOutcome::Redraw);
            assert_eq!(keys.event(56, true, &mut ui), KeyOutcome::Unchanged);
            assert_eq!(keys.event(key, true, &mut ui), KeyOutcome::Unchanged);
            assert_eq!(ui.decision_pending, None);
            assert_eq!(ui.decision_choice, None);
            keys.event(key, false, &mut ui);
            assert_eq!(keys.event(key, true, &mut ui), KeyOutcome::Redraw);
            assert_eq!(ui.decision_pending, Some(11));
            assert_eq!(ui.decision_choice, Some(allow));
        }
    }

    #[test]
    fn held_alt_stop_typematic_must_not_reissue_intent() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        assert!(ui.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        assert!(ui.observe(BackendEvent::Started { task_id: 7, seq: 1 }));
        assert_eq!(keys.event(56, true, &mut ui), KeyOutcome::Unchanged);
        assert_eq!(keys.event(31, true, &mut ui), KeyOutcome::Redraw);
        assert_eq!(ui.notice.as_str(), "STOP INTENT ONLY - AWAIT SERVICE");
        assert_eq!(keys.event(31, true, &mut ui), KeyOutcome::Unchanged);
        // A separate physical release/press may intentionally retry Stop,
        // but neither event is proof of service cancellation.
        keys.event(31, false, &mut ui);
        assert_eq!(keys.event(31, true, &mut ui), KeyOutcome::Redraw);
    }

    #[test]
    fn enter_typematic_cannot_resubmit_after_matching_service_rejection() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        assert!(ui.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        stroke(&mut keys, &mut ui, 30); // a
        // First physical Enter submits one immutable key/prompt snapshot.
        assert_eq!(keys.event(28, true, &mut ui), KeyOutcome::Redraw);
        assert_eq!(ui.pending_submit.unwrap().idempotency_key, 1);
        // Service rejects before the physical Enter key is released.
        assert!(ui.observe(BackendEvent::SubmissionRejected { idempotency_key: 1 }));
        assert!(ui.pending_submit.is_none());
        assert_eq!(ui.input.as_str(), "a");
        // Auto-repeat of the SAME held Enter must not manufacture key 2.
        assert_eq!(keys.event(28, true, &mut ui), KeyOutcome::Unchanged);
        assert!(ui.pending_submit.is_none(),
                "a held Enter must not create a new admission key after refusal");
        keys.event(28, false, &mut ui);
        // Intentional release and fresh press is allowed.
        assert_eq!(keys.event(28, true, &mut ui), KeyOutcome::Redraw);
        assert_eq!(ui.pending_submit.unwrap().idempotency_key, 2);
    }

    #[test]
    fn held_enter_before_service_connects_cannot_submit_on_repeat() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        stroke(&mut keys, &mut ui, 30); // a while offline
        // Enter was pressed when no model service could accept it.
        assert_eq!(keys.event(28, true, &mut ui), KeyOutcome::Redraw);
        assert!(ui.pending_submit.is_none());
        assert!(ui.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        // A service-status transition must not turn typematic into fresh consent.
        assert_eq!(keys.event(28, true, &mut ui), KeyOutcome::Unchanged);
        assert!(ui.pending_submit.is_none());
        keys.event(28, false, &mut ui);
        assert_eq!(keys.event(28, true, &mut ui), KeyOutcome::Redraw);
        assert_eq!(ui.pending_submit.unwrap().idempotency_key, 1);
    }

    #[test]
    fn ctrl_chords_must_not_inject_editor_text_or_backspace() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        stroke(&mut keys, &mut ui, 30); // plain a
        assert_eq!(ui.input.as_str(), "a");
        keys.event(29, true, &mut ui); // Ctrl down
        for code in [30, 31, 14] { // Ctrl+A, Ctrl+S, Ctrl+Backspace
            assert_eq!(keys.event(code, true, &mut ui), KeyOutcome::Unchanged);
            keys.event(code, false, &mut ui);
        }
        assert_eq!(ui.input.as_str(), "a",
                   "unsupported Ctrl shortcuts are not literal text or deletion");
        keys.event(29, false, &mut ui);
        stroke(&mut keys, &mut ui, 48); // ordinary b after Ctrl release
        assert_eq!(ui.input.as_str(), "ab");
    }

    #[test]
    fn ctrl_enter_does_not_submit_and_release_alone_cannot_trigger_admission() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        assert!(ui.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        stroke(&mut keys, &mut ui, 30); // a
        keys.event(29, true, &mut ui); // Ctrl held
        assert_eq!(keys.event(28, true, &mut ui), KeyOutcome::Unchanged);
        assert!(ui.pending_submit.is_none());
        keys.event(29, false, &mut ui);
        assert_eq!(keys.event(28, true, &mut ui), KeyOutcome::Unchanged);
        assert!(ui.pending_submit.is_none());
        keys.event(28, false, &mut ui);
        assert_eq!(keys.event(28, true, &mut ui), KeyOutcome::Redraw);
        assert_eq!(ui.pending_submit.unwrap().idempotency_key, 1);
    }

    #[test]
    fn ctrl_alt_approve_is_not_the_documented_alt_approve_chord() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        assert!(ui.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        assert!(ui.observe(BackendEvent::Started { task_id: 7, seq: 1 }));
        assert!(ui.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 2, request: 11,
            action: "WRITE", target: "/home/a.txt",
        }));
        keys.event(29, true, &mut ui); // Ctrl down
        keys.event(56, true, &mut ui); // Alt down
        assert_eq!(keys.event(30, true, &mut ui), KeyOutcome::Unchanged);
        assert!(ui.decision_pending.is_none(),
                "an unsupported modifier chord must never create Allow intent");
        keys.event(30, false, &mut ui);
        keys.event(29, false, &mut ui);
        // A fresh exact Alt+A after Ctrl release is still the documented UI intent.
        assert_eq!(keys.event(30, true, &mut ui), KeyOutcome::Redraw);
        assert_eq!(ui.decision_pending, Some(11));
        assert_eq!(ui.decision_choice, Some(true));
    }

    #[test]
    fn shifted_alt_permission_requires_unmodified_fresh_press() {
        // Left/right Shift+Alt+A/D is NOT the documented exact Alt+A/D.
        // Releasing Shift while the choice key remains held must not turn
        // typematic repeat into an affirmative permission decision.
        for (shift, key, allow) in [
            (42usize, 30usize, true),
            (54usize, 30usize, true),
            (42usize, 32usize, false),
            (54usize, 32usize, false),
        ] {
            let mut keys = OrderedKeyboard::new();
            let mut ui = Ui::new();
            assert!(ui.observe(BackendEvent::Service {
                connected: true, model: Model::Ready, saved_mode: Mode::Single,
            }));
            assert!(ui.observe(BackendEvent::Started { task_id: 7, seq: 1 }));
            assert!(ui.observe(BackendEvent::PermissionNeeded {
                task_id: 7, seq: 2, request: 11,
                action: "WRITE", target: "/home/a.txt",
            }));
            keys.event(shift, true, &mut ui);
            keys.event(56, true, &mut ui); // Alt
            assert_eq!(keys.event(key, true, &mut ui), KeyOutcome::Unchanged,
                       "Shift+Alt must not create a permission intent");
            assert_eq!(ui.decision_pending, None);
            assert_eq!(ui.decision_choice, None);
            keys.event(shift, false, &mut ui);
            assert_eq!(keys.event(key, true, &mut ui), KeyOutcome::Unchanged,
                       "release of Shift is not a fresh consent keypress");
            assert_eq!(ui.decision_pending, None);
            keys.event(key, false, &mut ui);
            assert_eq!(keys.event(key, true, &mut ui), KeyOutcome::Redraw);
            assert_eq!(ui.decision_pending, Some(11));
            assert_eq!(ui.decision_choice, Some(allow));
        }
    }

    #[test]
    fn ordered_alt_actions_and_exit_do_not_type_text() {
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        keys.event(56, true, &mut ui); // Alt
        assert_eq!(keys.event(19, true, &mut ui), KeyOutcome::Redraw);
        keys.event(19, false, &mut ui);
        assert_eq!(keys.event(50, true, &mut ui), KeyOutcome::Redraw);
        keys.event(50, false, &mut ui);
        keys.event(56, false, &mut ui);
        assert!(ui.input.is_empty());
        assert_eq!(keys.event(1, true, &mut ui), KeyOutcome::Exit);
    }

    #[test]
    fn giving_up_takes_alt_g_twice_with_nothing_between() {
        let (alt, g, n) = (56usize, 34usize, 49usize);
        let tap = |keys: &mut OrderedKeyboard, ui: &mut Ui, code: usize| {
            let outcome = keys.event(code, true, ui);
            keys.event(code, false, ui);
            outcome
        };
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        // Where PCP OS never let this launch make files there is nothing to give up, and it says so.
        keys.event(alt, true, &mut ui);
        assert_eq!(tap(&mut keys, &mut ui, g), KeyOutcome::Redraw);
        assert_eq!((ui.give_up_armed, ui.notice.as_str()), (false, crate::note::NOT_ALLOWED));
        keys.event(alt, false, &mut ui);

        ui.notes = true;
        // Once asks; a second time, with Alt let go and pressed again in between, gives up.
        keys.event(alt, true, &mut ui);
        assert_eq!(tap(&mut keys, &mut ui, g), KeyOutcome::Redraw);
        assert_eq!((ui.give_up_armed, ui.notice.as_str()), (true, crate::note::GIVE_UP_ASK));
        keys.event(alt, false, &mut ui);
        keys.event(alt, true, &mut ui);
        assert_eq!(tap(&mut keys, &mut ui, g), KeyOutcome::GiveUp);
        assert!(!ui.give_up_armed);
        keys.event(alt, false, &mut ui);

        // A held G that repeats is one press, not two.
        keys.event(alt, true, &mut ui);
        assert_eq!(keys.event(g, true, &mut ui), KeyOutcome::Redraw);
        assert_eq!(keys.event(g, true, &mut ui), KeyOutcome::Unchanged);
        assert!(ui.give_up_armed);
        keys.event(g, false, &mut ui);
        keys.event(alt, false, &mut ui);
        // Anything else pressed in between takes the first press back, and its words with it.
        assert_eq!(tap(&mut keys, &mut ui, 30), KeyOutcome::Redraw); // the letter a
        assert_eq!((ui.give_up_armed, ui.notice.as_str()), (false, ""));
        keys.event(alt, true, &mut ui);
        assert_eq!(tap(&mut keys, &mut ui, g), KeyOutcome::Redraw);
        assert!(ui.give_up_armed);
        keys.event(alt, false, &mut ui);

        // A key that does nothing else, Shift alone, still takes the first press back and has the
        // screen drawn again without its words; the next Alt+G only asks again.
        assert_eq!(keys.event(42, true, &mut ui), KeyOutcome::Redraw);
        assert_eq!((ui.give_up_armed, ui.notice.as_str()), (false, ""));
        assert_eq!(keys.event(42, false, &mut ui), KeyOutcome::Unchanged);
        keys.event(alt, true, &mut ui);
        assert_eq!(tap(&mut keys, &mut ui, g), KeyOutcome::Redraw);
        assert_eq!((ui.give_up_armed, ui.notice.as_str()), (true, crate::note::GIVE_UP_ASK));
        keys.event(alt, false, &mut ui);
        keys.event(alt, true, &mut ui);
        assert_eq!(tap(&mut keys, &mut ui, g), KeyOutcome::GiveUp);
        keys.event(alt, false, &mut ui);

        // With Shift or Ctrl it is another chord and does nothing.
        for with in [42usize, 29] {
            let mut keys = OrderedKeyboard::new();
            let mut ui = Ui::new();
            ui.notes = true;
            keys.event(with, true, &mut ui);
            keys.event(alt, true, &mut ui);
            assert_eq!(tap(&mut keys, &mut ui, g), KeyOutcome::Unchanged);
            assert!(!ui.give_up_armed);
        }

        // Once PCP OS has confirmed, Alt+G only says so, and Alt+N is still a chord the program answers.
        let mut keys = OrderedKeyboard::new();
        let mut ui = Ui::new();
        ui.notes = true;
        ui.gave_up = true;
        keys.event(alt, true, &mut ui);
        assert_eq!(tap(&mut keys, &mut ui, g), KeyOutcome::Redraw);
        assert_eq!((ui.give_up_armed, ui.notice.as_str()), (false, crate::note::GAVE_UP));
        keys.event(n, true, &mut ui);
        keys.event(n, false, &mut ui);
        assert_eq!(keys.event(alt, false, &mut ui), KeyOutcome::MakeNote);
    }
}
