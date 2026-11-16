//! Conservative pointer-to-UI intent adapter, not a mouse device driver.
//!
//! The pinned public PCP-OS kernel exposes /dev/kbd and /dev/fb0, but
//! DOES NOT provide a pointer device/capability to this guest. This module
//! accepts already decoded, OS-supplied pointer events only when a future
//! trusted device adapter is provided by PT/B. Do not synthesize events
//! from LLM text, remote network packets, or unverified tool output.
//!
//! No pointer action can authorize an edit: Approve/Deny are deliberately
//! absent until an exact immutable effect and service-bound human approval
//! panel have been implemented and verified. Keyboard remains the fallback.

use crate::{Action, Intent, Mode, TaskState, Ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
impl Rect {
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y
            && (x as i64) < self.x as i64 + self.width as i64
            && (y as i64) < self.y as i64 + self.height as i64
    }
}

/// These must be the very same pixel rectangles drawn in native main.rs.
/// Fixed 960x540 guest coordinates. A future compositor must map the actual
/// pointer to guest-local pixels with correct clipping/scale/rotation.
pub const MODE_RECT: Rect = Rect { x: 34, y: 218, width: 254, height: 58 };
pub const STOP_RECT: Rect = Rect { x: 34, y: 318, width: 254, height: 72 };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton { Primary, Secondary, Middle }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerPhase { Down, Move, Up, Cancel }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerEvent {
    pub x: i32,
    pub y: i32,
    pub button: PointerButton,
    pub phase: PointerPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Capture {
    Mode { original: Mode, service_epoch: u64 },
    Stop { task_id: u64, original_state: TaskState, service_epoch: u64 },
}

impl Capture {
    fn rect(self) -> Rect {
        match self {
            Self::Mode { .. } => MODE_RECT,
            Self::Stop { .. } => STOP_RECT,
        }
    }
    fn service_epoch(self) -> u64 {
        match self {
            Self::Mode { service_epoch, .. } |
            Self::Stop { service_epoch, .. } => service_epoch,
        }
    }
}

#[derive(Default)]
pub struct PointerUi {
    captured: Option<Capture>,
    // Physical primary-button edge latch. A drag-out or chord invalidates
    // capture but cannot turn a repeated held Down into a fresh click.
    primary_held: bool,
}

impl PointerUi {
    pub const fn new() -> Self { Self { captured: None, primary_held: false } }

    /// Must also be called when the OS reports pointer focus loss, unplug,
    /// screen resize/mode change, display ownership transfer or GUI exit.
    pub fn cancel(&mut self) {
        self.captured = None;
        self.primary_held = false;
    }

    /// Only a primary-button DOWN, followed by UP in the same region, with
    /// the original user-visible control still enabled, emits an Intent.
    /// No event claims the action was delivered, authorized or executed.
    pub fn input(&mut self, e: PointerEvent, ui: &mut Ui) -> Option<Intent> {
        match e.phase {
            PointerPhase::Cancel => {
                self.cancel();
                None
            }
            PointerPhase::Down if e.button == PointerButton::Primary => {
                // A new gesture requires an actual Up/Down edge. While held,
                // a repeat Down after leaving/re-entering a control MUST NOT
                // recreate a capture that Move already invalidated.
                if self.primary_held { return None; }
                self.primary_held = true;
                if self.captured.is_none() && ui.service_epoch != u64::MAX {
                    self.captured = if MODE_RECT.contains(e.x, e.y)
                        && ui.connected && ui.mode_pending.is_none()
                        && ui.pending_submit.is_none()
                    {
                        Some(Capture::Mode {
                            original: ui.mode, service_epoch: ui.service_epoch,
                        })
                    } else if STOP_RECT.contains(e.x, e.y)
                        && ui.controls().stop_enabled
                    {
                        Some(Capture::Stop {
                            task_id: ui.task.id, original_state: ui.task.state,
                            service_epoch: ui.service_epoch,
                        })
                    } else {
                        None
                    };
                }
                None
            }
            PointerPhase::Down => {
                // A chord invalidates capture, not the held-primary latch.
                // Another primary Down before the real Up cannot rearm.
                self.captured = None;
                None
            }
            PointerPhase::Move => {
                if self.captured.is_some_and(|c| !c.rect().contains(e.x, e.y)) {
                    // A drag-out kills the target but not the press itself.
                    // Only a primary Up or explicit stream Cancel releases it.
                    self.captured = None;
                }
                None
            }
            PointerPhase::Up => {
                if e.button != PointerButton::Primary {
                    self.captured = None;
                    return None;
                }
                let was_held = self.primary_held;
                self.primary_held = false;
                if !was_held {
                    self.captured = None;
                    return None;
                }
                let capture = self.captured.take()?;
                if !capture.rect().contains(e.x, e.y)
                    || capture.service_epoch() != ui.service_epoch {
                    // A disconnect/reconnect or service-status refresh can
                    // occur without an input device's focus-loss Cancel.
                    // Never resume a gesture across those state boundaries.
                    return None;
                }
                match capture {
                    Capture::Mode { original, .. }
                        if ui.connected && ui.mode_pending.is_none()
                            && ui.pending_submit.is_none()
                            && ui.mode == original =>
                    {
                        ui.intent(Action::ToggleMode)
                    }
                    Capture::Stop { task_id, original_state, .. }
                        if ui.connected && ui.task.id == task_id
                            && ui.task.state == original_state
                            && ui.controls().stop_enabled =>
                    {
                        ui.intent(Action::Stop)
                    }
                    _ => None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackendEvent, Model, Text};

    fn event(x: i32, y: i32, phase: PointerPhase) -> PointerEvent {
        PointerEvent { x, y, button: PointerButton::Primary, phase }
    }
    fn connected() -> Ui {
        let mut ui = Ui::new();
        assert!(ui.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        ui
    }

    #[test]
    fn exact_half_open_rectangles_reject_neighbouring_pixels() {
        assert!(MODE_RECT.contains(34, 218));
        assert!(MODE_RECT.contains(287, 275));
        assert!(!MODE_RECT.contains(288, 275));
        assert!(!MODE_RECT.contains(287, 276));
        assert!(STOP_RECT.contains(34, 318));
        assert!(STOP_RECT.contains(287, 389));
        assert!(!STOP_RECT.contains(33, 318));
        assert!(!STOP_RECT.contains(34, 390));
        assert!(!STOP_RECT.contains(i32::MIN, i32::MIN));
        assert!(!MODE_RECT.contains(i32::MAX, i32::MAX));
    }

    #[test]
    fn mode_click_emits_only_request_and_requires_service_receipt() {
        let mut ui = connected();
        let mut pointer = PointerUi::new();
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Down), &mut ui), None);
        assert_eq!(ui.mode, Mode::Single);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui),
                   Some(Intent::SetMode(Mode::Ongoing)));
        assert_eq!(ui.mode, Mode::Single);
        assert_eq!(ui.mode_pending, Some(Mode::Ongoing));
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Down), &mut ui), None);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert!(ui.observe(BackendEvent::ModeCommitted(Mode::Ongoing)));
        assert_eq!(ui.mode, Mode::Ongoing);
    }

    #[test]
    fn stop_click_is_task_scoped_not_termination_or_authorization() {
        let mut ui = connected();
        let mut pointer = PointerUi::new();
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Down), &mut ui), None);
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Up), &mut ui), None);
        assert!(ui.observe(BackendEvent::Started { task_id: 7, seq: 1 }));
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Down), &mut ui), None);
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Up), &mut ui),
                   Some(Intent::Stop { task_id: 7 }));
        assert_eq!(ui.task.state, TaskState::Running);
        assert!(ui.observe(BackendEvent::StopAccepted { task_id: 7, seq: 2 }));
        assert_eq!(ui.task.state, TaskState::StopRequested);
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Down), &mut ui), None);
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Up), &mut ui), None);
    }

    #[test]
    fn click_must_start_and_finish_inside_same_unchanged_target() {
        let mut ui = connected();
        let mut pointer = PointerUi::new();
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Down), &mut ui), None);
        assert_eq!(pointer.input(event(400, 400, PointerPhase::Move), &mut ui), None);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.mode_pending, None);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Down), &mut ui), None);
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.mode_pending, None);
    }

    #[test]
    fn disable_or_change_task_mid_gesture_cancels_click() {
        let mut ui = connected();
        let mut pointer = PointerUi::new();
        assert!(ui.observe(BackendEvent::Started { task_id: 7, seq: 1 }));
        pointer.input(event(60, 340, PointerPhase::Down), &mut ui);
        assert!(ui.observe(BackendEvent::Stopped { task_id: 7, seq: 2 }));
        assert!(ui.observe(BackendEvent::Started { task_id: 8, seq: 1 }));
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.task.id, 8);
        pointer.input(event(50, 234, PointerPhase::Down), &mut ui);
        assert!(ui.observe(BackendEvent::ModeCommitted(Mode::Ongoing)));
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
    }

    #[test]
    fn pointer_cannot_approve_deny_or_invoke_hidden_grants() {
        let mut ui = connected();
        let mut pointer = PointerUi::new();
        assert!(ui.observe(BackendEvent::Started { task_id: 7, seq: 1 }));
        assert!(ui.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 2, request: 11,
            action: "WRITE", target: "/APPS/DEMO",
        }));
        for (x, y) in [(760, 392), (820, 405), (500, 420), (150, 480)] {
            pointer.input(event(x, y, PointerPhase::Down), &mut ui);
            assert_eq!(pointer.input(event(x, y, PointerPhase::Up), &mut ui), None);
        }
        assert_eq!(ui.permission.unwrap().request, 11);
        assert_eq!(ui.decision_pending, None);
        assert_eq!(ui.task.state, TaskState::Running);
        assert_eq!(ui.input.as_str(), "");
    }

    #[test]
    fn unplug_cancel_and_nonprimary_chords_cannot_activate_capture() {
        let mut ui = connected();
        let mut pointer = PointerUi::new();
        pointer.input(event(50, 234, PointerPhase::Down), &mut ui);
        pointer.cancel();
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);

        pointer.input(event(50, 234, PointerPhase::Down), &mut ui);
        assert_eq!(pointer.input(PointerEvent {
            x: 50, y: 234, button: PointerButton::Secondary, phase: PointerPhase::Down,
        }, &mut ui), None);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);

        pointer.input(event(50, 234, PointerPhase::Down), &mut ui);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Cancel), &mut ui), None);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.mode_pending, None);
    }

    #[test]
    fn mode_press_across_disconnect_and_reconnect_is_rejected() {
        let mut ui = connected();
        let mut pointer = PointerUi::new();
        assert_eq!(ui.service_epoch, 1);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Down), &mut ui), None);
        assert!(ui.observe(BackendEvent::Service {
            connected: false, model: Model::Unknown, saved_mode: Mode::Single,
        }));
        assert!(ui.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        assert_eq!(ui.service_epoch, 3);
        // Same apparent mode and service connectivity at Up must not
        // resurrect the stale Down event as a new user request.
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.mode_pending, None);
        assert_eq!(ui.mode, Mode::Single);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Down), &mut ui), None);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui),
                   Some(Intent::SetMode(Mode::Ongoing)));
    }

    #[test]
    fn repeated_service_snapshot_invalidates_mode_press_even_if_unchanged() {
        let mut ui = connected();
        let mut pointer = PointerUi::new();
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Down), &mut ui), None);
        assert!(ui.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.mode_pending, None);
        assert_eq!(ui.mode, Mode::Single);
    }

    #[test]
    fn stop_gesture_from_reconnected_unknown_task_must_use_fresh_down() {
        let mut ui = connected();
        let mut pointer = PointerUi::new();
        assert!(ui.observe(BackendEvent::Started { task_id: 7, seq: 1 }));
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Down), &mut ui), None);
        assert!(ui.observe(BackendEvent::Service {
            connected: false, model: Model::Unknown, saved_mode: Mode::Single,
        }));
        assert_eq!(ui.task.state, TaskState::Unconfirmed);
        assert!(ui.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        assert!(ui.controls().stop_enabled);
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.task.state, TaskState::Unconfirmed);
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Down), &mut ui), None);
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Up), &mut ui),
                   Some(Intent::Stop { task_id: 7 }));
        assert_eq!(ui.task.state, TaskState::Unconfirmed); // intent, not stop
    }

    #[test]
    fn exhausted_service_epoch_disables_pointer_capture_without_wrap() {
        let mut ui = connected();
        ui.service_epoch = u64::MAX - 1;
        let mut pointer = PointerUi::new();
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Down), &mut ui), None);
        assert!(ui.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        assert_eq!(ui.service_epoch, u64::MAX);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Down), &mut ui), None);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.mode_pending, None);
        assert!(ui.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        assert_eq!(ui.service_epoch, u64::MAX); // no wrap to an old epoch
    }


    #[test]
    fn drag_out_and_repeat_down_on_reentry_never_resurrect_click() {
        let mut ui = connected();
        let mut pointer = PointerUi::new();
        // Independent source-level adverse trace from Director G:
        // press Mode, leave, return, repeat held Down, then release.
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Down), &mut ui), None);
        assert_eq!(pointer.input(event(400, 234, PointerPhase::Move), &mut ui), None);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Move), &mut ui), None);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Down), &mut ui), None);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.mode_pending, None);
        // Real subsequent Up -> Down -> Up may generate a fresh UI intent.
        pointer.input(event(50, 234, PointerPhase::Down), &mut ui);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui),
                   Some(Intent::SetMode(Mode::Ongoing)));
    }

    #[test]
    fn outside_press_and_secondary_chord_cannot_rearm_during_hold() {
        let mut ui = connected();
        let mut pointer = PointerUi::new();
        pointer.input(event(400, 234, PointerPhase::Down), &mut ui);
        pointer.input(event(50, 234, PointerPhase::Move), &mut ui);
        pointer.input(event(50, 234, PointerPhase::Down), &mut ui);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.mode_pending, None);

        pointer.input(event(50, 234, PointerPhase::Down), &mut ui);
        pointer.input(PointerEvent {
            x: 50, y: 234, button: PointerButton::Secondary, phase: PointerPhase::Down,
        }, &mut ui);
        pointer.input(event(50, 234, PointerPhase::Down), &mut ui);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.mode_pending, None);
    }

    #[test]
    fn device_loss_cancels_pointer_and_keyboard_fallback_requests_stop() {
        use crate::ordered_keys::{KeyOutcome, OrderedKeyboard};
        let mut ui = connected();
        let mut pointer = PointerUi::new();
        assert!(ui.observe(BackendEvent::Started { task_id: 91, seq: 1 }));
        pointer.input(event(60, 340, PointerPhase::Down), &mut ui);
        // Synthetic loss signal from the future trusted OS input source.
        // No public PS/2 mouse, USB/HID or Bluetooth driver is being run.
        pointer.input(event(60, 340, PointerPhase::Cancel), &mut ui);
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.task.state, TaskState::Running);

        let mut keyboard = OrderedKeyboard::new();
        assert_eq!(keyboard.event(56, true, &mut ui), KeyOutcome::Unchanged);
        assert_eq!(keyboard.event(31, true, &mut ui), KeyOutcome::Redraw);
        keyboard.event(31, false, &mut ui);
        keyboard.event(56, false, &mut ui);
        // Keyboard path remains usable, but it has only made a Stop intent.
        assert_eq!(ui.notice.as_str(), "STOP INTENT ONLY - AWAIT SERVICE");
        assert_eq!(ui.task.id, 91);
        assert_eq!(ui.task.state, TaskState::Running);
        assert!(ui.observe(BackendEvent::StopAccepted { task_id: 91, seq: 2 }));
        assert_eq!(ui.task.state, TaskState::StopRequested);
    }

    #[test]
    fn disconnected_and_pending_submissions_have_no_pointer_effect() {
        let mut ui = Ui::new();
        let mut pointer = PointerUi::new();
        pointer.input(event(50, 234, PointerPhase::Down), &mut ui);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.mode_pending, None);

        assert!(ui.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        ui.input = Text::display("demo");
        assert!(ui.intent(Action::Submit).is_some());
        pointer.input(event(50, 234, PointerPhase::Down), &mut ui);
        assert_eq!(pointer.input(event(50, 234, PointerPhase::Up), &mut ui), None);
        assert_eq!(ui.mode_pending, None);
        assert_eq!(ui.pending_submit.unwrap().idempotency_key, 1);
        pointer.input(event(60, 340, PointerPhase::Down), &mut ui);
        assert_eq!(pointer.input(event(60, 340, PointerPhase::Up), &mut ui), None);
    }
}
