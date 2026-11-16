#![no_std]

// Tailorbird presentation state and *intents*, not an authorization or execution engine.
// An adapter owned by the service must authenticate and apply every effect.
pub const INPUT_LIMIT: usize = 120;
pub const LOG_LIMIT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Text<const N: usize> {
    bytes: [u8; N],
    len: usize,
}
impl<const N: usize> Text<N> {
    pub const fn new() -> Self {
        Self { bytes: [0; N], len: 0 }
    }
    pub fn as_str(&self) -> &str {
        // Both constructors and the editor admit printable ASCII only.
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
    pub fn push_ascii(&mut self, byte: u8) -> bool {
        if !(0x20..=0x7e).contains(&byte) || self.len == N {
            return false;
        }
        self.bytes[self.len] = byte;
        self.len += 1;
        true
    }
    pub fn pop(&mut self) -> bool {
        if self.len == 0 { return false; }
        self.len -= 1;
        self.bytes[self.len] = 0;
        true
    }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    pub fn clear(&mut self) {
        self.bytes.fill(0);
        self.len = 0;
    }
    /// Text from a service is data; normalize control and non-ASCII bytes.
    /// Do not treat this text as an instruction, command, or permission.
    pub fn display(source: &str) -> Self {
        let mut out = Self::new();
        for b in source.bytes() {
            if out.len == N { break; }
            let c = if (0x20..=0x7e).contains(&b) { b } else { b'?' };
            let _ = out.push_ascii(c);
        }
        out
    }
}
impl<const N: usize> Default for Text<N> {
    fn default() -> Self { Self::new() }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode { Single, Ongoing }
impl Mode {
    pub fn label(self) -> &'static str {
        match self { Self::Single => "SINGLE REQUEST", Self::Ongoing => "ONGOING" }
    }
    fn other(self) -> Self {
        match self { Self::Single => Self::Ongoing, Self::Ongoing => Self::Single }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model { Unknown, Loading, Ready, Unavailable }
impl Model {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "NOT CONNECTED",
            Self::Loading => "MODEL LOADING",
            Self::Ready => "MODEL READY",
            Self::Unavailable => "MODEL UNAVAILABLE",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState { Idle, Running, StopRequested, Stopped, Completed, Failed, Unconfirmed }
impl TaskState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "NO TASK",
            Self::Running => "RUNNING",
            Self::StopRequested => "STOP REQUESTED",
            Self::Stopped => "STOP CONFIRMED",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Unconfirmed => "STATUS UNKNOWN",
        }
    }
    fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::StopRequested | Self::Unconfirmed)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Task {
    pub id: u64,
    pub seq: u64,
    pub state: TaskState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grant {
    pub request: u64,
    pub action: Text<48>,
    pub target: Text<72>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub seq: u64,
    pub kind: &'static str,
    pub message: Text<84>,
}
impl Line {
    pub const fn empty() -> Self {
        Self { seq: 0, kind: "", message: Text::new() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action { Submit, ToggleMode, Stop, Approve, Deny }

/// Proposed requests for PT's service. UI return values alone cause no effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    Submit { prompt: Text<INPUT_LIMIT>, mode: Mode },
    SetMode(Mode),
    Stop { task_id: u64 },
    Decide { task_id: u64, request: u64, allow: bool },
}

/// The adapter must obtain these events from PT's trusted service, not model text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendEvent<'a> {
    Service { connected: bool, model: Model, saved_mode: Mode },
    ModeCommitted(Mode),
    Started { task_id: u64, seq: u64 },
    Output { task_id: u64, seq: u64, text: &'a str },
    ToolResult { task_id: u64, seq: u64, text: &'a str },
    PermissionNeeded { task_id: u64, seq: u64, request: u64, action: &'a str, target: &'a str },
    PermissionResolved { task_id: u64, seq: u64, request: u64, allowed: bool },
    StopAccepted { task_id: u64, seq: u64 },
    Stopped { task_id: u64, seq: u64 },
    Completed { task_id: u64, seq: u64 },
    Failed { task_id: u64, seq: u64, reason: &'a str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Controls {
    pub mode_visible: bool,
    pub stop_visible: bool,
    pub stop_enabled: bool,
    pub submit_enabled: bool,
    pub permission_visible: bool,
    /// Presentation affordance only. PT's trusted service decides whether an
    /// immutable task-scoped edit is actually authorized and applied.
    pub decision_enabled: bool,
}

pub struct Ui {
    pub input: Text<INPUT_LIMIT>,
    pub mode: Mode,
    pub mode_pending: Option<Mode>,
    pub model: Model,
    pub connected: bool,
    pub task: Task,
    pub permission: Option<Grant>,
    /// A GUI confirmation may be sent only once for the pending request.
    /// This is not an authorization record or proof of delivery.
    pub decision_pending: Option<u64>,
    /// The last GUI choice for the current pending request. A service-side
    /// grant without this exact Allow choice cannot be represented as consent.
    /// This field is presentation evidence, not an authorization credential.
    pub decision_choice: Option<bool>,
    pub notice: Text<84>,
    lines: [Line; LOG_LIMIT],
    count: usize,
}
impl Ui {
    pub const fn new() -> Self {
        Self {
            input: Text::new(), mode: Mode::Single, mode_pending: None,
            model: Model::Unknown, connected: false,
            task: Task { id: 0, seq: 0, state: TaskState::Idle },
            permission: None, decision_pending: None, decision_choice: None,
            notice: Text::new(),
            lines: [Line::empty(); LOG_LIMIT], count: 0,
        }
    }
    pub fn controls(&self) -> Controls {
        Controls {
            mode_visible: true, stop_visible: true,
            // When a service reconnects, an unresolved task may still be running.
            // Keep the task-scoped stop *intent* available; the service must
            // perform the actual cancellation and return terminal evidence.
            stop_enabled: self.connected && matches!(self.task.state,
                TaskState::Running | TaskState::Unconfirmed),
            submit_enabled: self.connected && self.model == Model::Ready
                && !self.input.is_empty() && !self.task.state.is_active(),
            permission_visible: self.permission.is_some(),
            decision_enabled: self.connected && self.task.state == TaskState::Running
                && self.permission.is_some() && self.decision_pending.is_none(),
        }
    }
    pub fn lines(&self) -> &[Line] { &self.lines[..self.count] }
    fn notice(&mut self, s: &str) { self.notice = Text::display(s); }
    /// The native 5x7 font has only uppercase, digits, slash, dash, dot,
    /// plus and blank space. The generic label renderer also folds case.
    /// Refuse GUI consent when either exact field cannot be shown, rather
    /// than approving a clipped or visually aliased destination.
    fn permission_field_visible(s: &str, limit: usize, allow_space: bool) -> bool {
        !s.is_empty() && s.len() <= limit && s.bytes().all(|b| {
            matches!(b, b'A'..=b'Z' | b'0'..=b'9' | b'/' | b'-' | b'.' | b'+')
                || (allow_space && b == b' ')
        }) && (!allow_space || (!s.starts_with(' ') && !s.ends_with(' ')))
    }
    fn permission_details_visible(action: &str, target: &str) -> bool {
        Self::permission_field_visible(action, 48, true)
            && Self::permission_field_visible(target, 72, false)
    }
    fn log(&mut self, seq: u64, kind: &'static str, message: &str) {
        let line = Line { seq, kind, message: Text::display(message) };
        if self.count < LOG_LIMIT {
            self.lines[self.count] = line;
            self.count += 1;
        } else {
            for i in 1..LOG_LIMIT { self.lines[i - 1] = self.lines[i]; }
            self.lines[LOG_LIMIT - 1] = line;
        }
    }
    fn accept(&mut self, id: u64, seq: u64) -> bool {
        if !self.connected || id == 0 || id != self.task.id || seq <= self.task.seq
            || !matches!(self.task.state, TaskState::Running | TaskState::StopRequested) {
            return false;
        }
        self.task.seq = seq;
        true
    }
    /// A lost connection does not terminate a task. Once reconnected, only a
    /// newer terminal event for the *same* task can settle an UNCONFIRMED
    /// state. Ordinary output/permission events must not re-open execution.
    /// Trust in event origin and final readback remains PT's service duty.
    fn accept_terminal(&mut self, id: u64, seq: u64) -> bool {
        if !self.connected || id == 0 || id != self.task.id || seq <= self.task.seq
            || !self.task.state.is_active() {
            return false;
        }
        self.task.seq = seq;
        true
    }
    /// A recovered unknown task may receive a stop-request acknowledgment
    /// after its Stop intent. This still says nothing about whether it has
    /// terminated. The service enforces the actual cancellation boundary.
    fn accept_stop_ack(&mut self, id: u64, seq: u64) -> bool {
        if !self.connected || id == 0 || id != self.task.id || seq <= self.task.seq
            || !self.task.state.is_active() {
            return false;
        }
        self.task.seq = seq;
        true
    }
    pub fn intent(&mut self, action: Action) -> Option<Intent> {
        match action {
            Action::Submit if self.controls().submit_enabled => {
                self.notice("REQUEST INTENT ONLY - AWAIT SERVICE");
                Some(Intent::Submit { prompt: self.input, mode: self.mode })
            }
            Action::ToggleMode if self.connected && self.mode_pending.is_none() => {
                let next = self.mode.other();
                self.mode_pending = Some(next);
                self.notice("MODE CHANGE NOT YET CONFIRMED");
                Some(Intent::SetMode(next))
            }
            Action::Stop if self.controls().stop_enabled => {
                self.notice("STOP INTENT ONLY - AWAIT SERVICE");
                Some(Intent::Stop { task_id: self.task.id })
            }
            Action::Approve | Action::Deny
                if self.controls().decision_enabled =>
            {
                let g = self.permission.unwrap();
                self.decision_pending = Some(g.request);
                self.decision_choice = Some(action == Action::Approve);
                self.notice("PERMISSION DECISION SENT - AWAIT SERVICE");
                Some(Intent::Decide {
                    task_id: self.task.id, request: g.request,
                    allow: action == Action::Approve,
                })
            }
            Action::Submit => { self.notice("NO MODEL SERVICE - OS STILL USABLE"); None }
            Action::ToggleMode => { self.notice("MODE CHANGE UNAVAILABLE"); None }
            Action::Stop => { self.notice("NO STOPPABLE TASK - USE OS STOP KEY"); None }
            Action::Approve | Action::Deny if self.decision_pending.is_some() => {
                self.notice("DECISION PENDING - AWAIT SERVICE"); None
            }
            Action::Approve | Action::Deny => { self.notice("NO VALID PERMISSION REQUEST"); None }
        }
    }
    pub fn observe(&mut self, event: BackendEvent<'_>) -> bool {
        match event {
            BackendEvent::Service { connected, model, saved_mode } => {
                let requested_mode = self.mode_pending;
                self.connected = connected;
                self.model = if connected { model } else { Model::Unknown };
                // A service status is an observation about the saved choice,
                // not proof that the user's pending mode-change request
                // succeeded. Never silently turn a mismatch into success.
                self.mode = saved_mode;
                self.mode_pending = None;
                if !connected {
                    if self.task.state.is_active() {
                        self.task.state = TaskState::Unconfirmed;
                    }
                    self.permission = None;
                    self.decision_pending = None;
                    self.decision_choice = None;
                    self.notice("SERVICE LOST - NO STOP CONFIRMATION");
                } else if requested_mode.is_some() && requested_mode != Some(saved_mode) {
                    self.notice("MODE NOT APPLIED - SERVICE STATUS");
                } else if requested_mode.is_some() {
                    self.notice("MODE MATCHES SERVICE STATUS");
                } else {
                    self.notice("SERVICE STATUS RECEIVED");
                }
                true
            }
            BackendEvent::ModeCommitted(mode) if self.connected => {
                let requested = self.mode_pending;
                self.mode = mode;
                self.mode_pending = None;
                self.notice(match requested {
                    Some(wanted) if wanted == mode => "MODE CONFIRMED BY SERVICE",
                    Some(_) => "MODE NOT APPLIED - SERVICE REPLIED DIFFERENTLY",
                    None => "SERVICE MODE UPDATE - NO UI REQUEST",
                });
                true
            }
            BackendEvent::Started { task_id, seq }
                if self.connected && task_id != 0 && seq > 0
                    // An immediate replay of the just-finished task ID must
                    // not erase its terminal state or displayed readback.
                    // PT's trusted service remains responsible for global
                    // task-ID uniqueness and authenticated event delivery.
                    && task_id != self.task.id
                    && !self.task.state.is_active() =>
            {
                self.task = Task { id: task_id, seq, state: TaskState::Running };
                self.permission = None;
                self.decision_pending = None;
                self.decision_choice = None;
                self.count = 0;
                self.input.clear();
                self.log(seq, "START", "TASK START REPORTED");
                self.notice("RUNNING - SERVICE REPORTED");
                true
            }
            BackendEvent::Output { task_id, seq, text } => {
                if !self.accept(task_id, seq) { return false; }
                self.log(seq, "OUTPUT", text);
                true
            }
            BackendEvent::ToolResult { task_id, seq, text } => {
                if !self.accept(task_id, seq) { return false; }
                self.log(seq, "TOOL", text);
                true
            }
            BackendEvent::PermissionNeeded { task_id, seq, request, action, target } => {
                // A task whose Stop was acknowledged is no longer permitted to
                // present fresh effect requests. The service must independently
                // enforce the stop fence; this is only UI state hygiene.
                if self.task.state != TaskState::Running
                    || request == 0 || self.permission.is_some()
                    || !self.accept(task_id, seq) {
                    return false;
                }
                // A truncated, case-folded or unsupported glyph can make
                // two different filesystem destinations look identical.
                // The preview must not offer an Allow intent unless the
                // whole immutable action and target are literally visible.
                if !Self::permission_details_visible(action, target) {
                    self.task.state = TaskState::Unconfirmed;
                    self.decision_pending = None;
                    self.decision_choice = None;
                    self.log(seq, "ANOMALY", "PERMISSION DETAILS NOT DISPLAYABLE");
                    self.notice("PERMISSION NOT DISPLAYABLE - STOP TASK");
                    return true;
                }
                self.permission = Some(Grant {
                    request, action: Text::display(action), target: Text::display(target),
                });
                self.decision_pending = None;
                self.decision_choice = None;
                self.log(seq, "PERMISSION", "DECISION REQUIRED");
                self.notice("PERMISSION PENDING - DEFAULT DENY");
                true
            }
            BackendEvent::PermissionResolved { task_id, seq, request, allowed } => {
                if self.permission.map(|p| p.request) != Some(request) || !self.accept(task_id, seq) {
                    return false;
                }
                // If PT's service reports a grant for an action explicitly
                // denied here, or never approved here, display an anomaly
                // rather than silently treating the event as GUI consent.
                // Effects may already have happened; the service must audit
                // them and enforce the actual destination permission.
                let unexplained_grant = allowed && self.decision_choice != Some(true);
                self.permission = None;
                self.decision_pending = None;
                self.decision_choice = None;
                if unexplained_grant {
                    self.task.state = TaskState::Unconfirmed;
                    self.log(seq, "ANOMALY", "SERVICE GRANT WITHOUT GUI CONSENT");
                    self.notice("SERVICE GRANT MISMATCH - EFFECT UNKNOWN");
                } else {
                    self.log(seq, "DECISION", if allowed {
                        "SERVICE REPORTED GRANT"
                    } else {
                        "SERVICE REPORTED DENIAL"
                    });
                }
                true
            }
            BackendEvent::StopAccepted { task_id, seq } => {
                if !self.accept_stop_ack(task_id, seq) { return false; }
                self.task.state = TaskState::StopRequested;
                // An earlier pending UI grant is no longer actionable after
                // Stop acknowledgment. Its service-side outcome, including
                // any effect already dispatched, remains unknown until readback.
                self.permission = None;
                self.decision_pending = None;
                self.decision_choice = None;
                self.notice("STOP ACCEPTED - NO NEW GRANTS; END UNCONFIRMED");
                self.log(seq, "STOP", "REQUEST ACCEPTED BY SERVICE");
                true
            }
            BackendEvent::Stopped { task_id, seq } => {
                if !self.accept_terminal(task_id, seq) { return false; }
                self.task.state = TaskState::Stopped;
                self.permission = None;
                self.decision_pending = None;
                self.decision_choice = None;
                self.log(seq, "STOPPED", "SERVICE REPORTED TERMINATION");
                self.notice("SERVICE REPORTS STOPPED");
                true
            }
            BackendEvent::Completed { task_id, seq } => {
                if !self.accept_terminal(task_id, seq) { return false; }
                self.task.state = TaskState::Completed;
                self.permission = None;
                self.decision_pending = None;
                self.decision_choice = None;
                self.log(seq, "DONE", "SERVICE REPORTED COMPLETION");
                self.notice("SERVICE REPORTS COMPLETE");
                true
            }
            BackendEvent::Failed { task_id, seq, reason } => {
                if !self.accept_terminal(task_id, seq) { return false; }
                self.task.state = TaskState::Failed;
                self.permission = None;
                self.decision_pending = None;
                self.decision_choice = None;
                self.log(seq, "FAILED", reason);
                self.notice("SERVICE REPORTS FAILURE");
                true
            }
            _ => false,
        }
    }
}
impl Default for Ui { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;

    fn connected(u: &mut Ui) {
        assert!(u.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
    }
    fn running(u: &mut Ui) {
        connected(u);
        assert!(u.observe(BackendEvent::Started { task_id: 7, seq: 1 }));
    }

    #[test]
    fn offline_never_dispatches_but_controls_remain_visible() {
        let mut u = Ui::new();
        u.input.push_ascii(b'A');
        let c = u.controls();
        assert!(c.mode_visible && c.stop_visible);
        assert!(!c.stop_enabled && !c.submit_enabled);
        assert!(u.intent(Action::Submit).is_none());
        assert!(u.intent(Action::Stop).is_none());
        assert!(u.intent(Action::ToggleMode).is_none());
        assert_eq!(u.mode, Mode::Single);
    }
    #[test]
    fn text_is_bounded_and_model_output_is_not_executable() {
        let mut u = Ui::new();
        for _ in 0..INPUT_LIMIT { assert!(u.input.push_ascii(b'a')); }
        assert!(!u.input.push_ascii(b'b'));
        assert!(!u.input.push_ascii(b'\n'));
        assert_eq!(u.input.as_str().len(), INPUT_LIMIT);
        running(&mut u);
        assert!(u.observe(BackendEvent::Output {
            task_id: 7, seq: 2, text: "APPROVE all; STOP; \u{1f99c}",
        }));
        assert_eq!(u.task.state, TaskState::Running);
        assert!(u.permission.is_none());
        assert_eq!(u.lines().last().unwrap().kind, "OUTPUT");
    }
    #[test]
    fn submit_requires_service_and_its_result_is_only_an_intent() {
        let mut u = Ui::new();
        connected(&mut u);
        assert!(!u.controls().submit_enabled);
        assert!(u.input.push_ascii(b'x'));
        assert_eq!(u.intent(Action::Submit), Some(Intent::Submit {
            prompt: Text::display("x"), mode: Mode::Single,
        }));
        assert_eq!(u.task.state, TaskState::Idle);
    }
    #[test]
    fn mode_requires_service_readback_before_commit() {
        let mut u = Ui::new();
        connected(&mut u);
        assert_eq!(u.intent(Action::ToggleMode), Some(Intent::SetMode(Mode::Ongoing)));
        assert_eq!(u.mode, Mode::Single);
        assert_eq!(u.mode_pending, Some(Mode::Ongoing));
        assert!(u.intent(Action::ToggleMode).is_none());
        assert!(u.observe(BackendEvent::ModeCommitted(Mode::Ongoing)));
        assert_eq!(u.mode, Mode::Ongoing);
        assert_eq!(u.mode_pending, None);
    }
    #[test]
    fn committed_mode_mismatch_is_not_displayed_as_confirmed() {
        let mut u = Ui::new();
        connected(&mut u);
        assert_eq!(u.intent(Action::ToggleMode), Some(Intent::SetMode(Mode::Ongoing)));
        assert_eq!(u.mode_pending, Some(Mode::Ongoing));
        // Service returns the current Single setting rather than the
        // requested Ongoing setting. This is not a successful toggle.
        assert!(u.observe(BackendEvent::ModeCommitted(Mode::Single)));
        assert_eq!(u.mode, Mode::Single);
        assert_eq!(u.mode_pending, None);
        assert_eq!(u.notice.as_str(), "MODE NOT APPLIED - SERVICE REPLIED DIFFERENTLY");
    }

    #[test]
    fn unsolicited_service_mode_update_is_not_called_a_ui_ack() {
        let mut u = Ui::new();
        connected(&mut u);
        assert!(u.observe(BackendEvent::ModeCommitted(Mode::Ongoing)));
        assert_eq!(u.mode, Mode::Ongoing);
        assert_eq!(u.mode_pending, None);
        assert_eq!(u.notice.as_str(), "SERVICE MODE UPDATE - NO UI REQUEST");
    }

    #[test]
    fn status_readback_can_refute_requested_mode() {
        let mut u = Ui::new();
        connected(&mut u);
        assert_eq!(u.intent(Action::ToggleMode), Some(Intent::SetMode(Mode::Ongoing)));
        assert!(u.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        assert_eq!(u.mode, Mode::Single);
        assert_eq!(u.mode_pending, None);
        assert_eq!(u.notice.as_str(), "MODE NOT APPLIED - SERVICE STATUS");

        assert_eq!(u.intent(Action::ToggleMode), Some(Intent::SetMode(Mode::Ongoing)));
        assert!(u.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Ongoing,
        }));
        assert_eq!(u.mode, Mode::Ongoing);
        assert_eq!(u.notice.as_str(), "MODE MATCHES SERVICE STATUS");
    }

    #[test]
    fn stop_needs_task_and_service_then_separate_ack_and_termination() {
        let mut u = Ui::new();
        running(&mut u);
        assert_eq!(u.intent(Action::Stop), Some(Intent::Stop { task_id: 7 }));
        assert_eq!(u.task.state, TaskState::Running);
        assert!(u.observe(BackendEvent::StopAccepted { task_id: 7, seq: 2 }));
        assert_eq!(u.task.state, TaskState::StopRequested);
        assert!(!u.controls().stop_enabled);
        assert!(u.observe(BackendEvent::Stopped { task_id: 7, seq: 3 }));
        assert_eq!(u.task.state, TaskState::Stopped);
        assert!(!u.observe(BackendEvent::Output { task_id: 7, seq: 4, text: "late" }));
    }
    #[test]
    fn wrong_task_stale_sequence_and_duplicate_start_are_rejected() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(!u.observe(BackendEvent::Output { task_id: 8, seq: 2, text: "wrong" }));
        assert!(!u.observe(BackendEvent::Output { task_id: 7, seq: 1, text: "same seq" }));
        assert!(!u.observe(BackendEvent::Started { task_id: 9, seq: 1 }));
        assert_eq!(u.lines().len(), 1);
    }
    #[test]
    fn grants_are_visible_and_never_implicitly_accepted() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 2, request: 14, action: "WRITE FILE", target: "/APPS/test",
        }));
        assert!(u.controls().permission_visible);
        assert_eq!(u.permission.unwrap().target.as_str(), "/APPS/test");
        assert!(!u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 3, request: 15, action: "DELETE", target: "/",
        }));
        assert_eq!(u.intent(Action::Deny), Some(Intent::Decide {
            task_id: 7, request: 14, allow: false,
        }));
        assert!(u.permission.is_some());
        assert!(u.observe(BackendEvent::PermissionResolved {
            task_id: 7, seq: 3, request: 14, allowed: false,
        }));
        assert!(u.permission.is_none());
    }
    #[test]
    fn visually_aliased_targets_and_actions_cannot_offer_consent() {
        let invalid: [(&str, &str); 6] = [
            ("WRITE FILE", "/data/file"),      // font folds lowercase to uppercase
            ("WRITE FILE", "/DATA/FILE_NAME"), // underscore has no glyph
            ("WRITE_FILE", "/DATA/FILE"),
            ("WRITE FILE", "/DATA/FILE NAME"), // blank is ambiguous in a path
            ("WRITE FILE", "/DATA/FILE\\NAME"), // backslash has no glyph
            ("WRITE FILE", ""),
        ];
        for (action, target) in invalid {
            let mut u = Ui::new();
            running(&mut u);
            assert!(u.observe(BackendEvent::PermissionNeeded {
                task_id: 7, seq: 2, request: 17, action, target,
            }));
            assert_eq!(u.task.state, TaskState::Unconfirmed);
            assert!(u.permission.is_none());
            assert!(!u.controls().decision_enabled);
            assert!(u.intent(Action::Approve).is_none());
            assert!(u.intent(Action::Deny).is_none());
            assert!(u.controls().stop_enabled);
            assert_eq!(u.lines().last().unwrap().kind, "ANOMALY");
        }
    }

    #[test]
    fn permission_metadata_overflow_and_controls_are_refused() {
        let invalid: [(&str, &str); 4] = [
            ("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "/DATA/FILE"),
            ("WRITE FILE", "/BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"),
            ("WRITE FILE", "/DATA/FILE\nHIDDEN"),
            ("WRITE FILE", "/DATA/FILE🦜"),
        ];
        for (action, target) in invalid {
            let mut u = Ui::new();
            running(&mut u);
            assert!(u.observe(BackendEvent::PermissionNeeded {
                task_id: 7, seq: 2, request: 17, action, target,
            }));
            assert_eq!(u.task.state, TaskState::Unconfirmed);
            assert!(!u.controls().decision_enabled);
            assert!(u.permission.is_none());
        }
    }

    #[test]
    fn exactly_renderable_72_byte_target_remains_decidable() {
        let mut u = Ui::new();
        running(&mut u);
        let target = "/CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC";
        assert_eq!(target.len(), 72);
        assert!(u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 2, request: 17,
            action: "WRITE FILE", target,
        }));
        assert_eq!(u.task.state, TaskState::Running);
        assert_eq!(u.permission.unwrap().target.as_str(), target);
        assert_eq!(u.intent(Action::Approve), Some(Intent::Decide {
            task_id: 7, request: 17, allow: true,
        }));
    }

    #[test]
    fn post_stop_permission_proposal_is_not_displayed_or_approved() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::StopAccepted { task_id: 7, seq: 2 }));
        assert_eq!(u.task.state, TaskState::StopRequested);
        assert_eq!(u.task.seq, 2);
        assert!(!u.controls().decision_enabled);
        assert!(!u.controls().permission_visible);

        // This newer service-supplied proposal is not a reason to reopen a
        // permission prompt after the service acknowledged a stop request.
        assert!(!u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 3, request: 46, action: "WRITE", target: "/APPS/bird.txt",
        }));
        assert_eq!(u.task.seq, 2); // rejected proposal consumes no event seq
        assert!(u.permission.is_none());
        assert_eq!(u.notice.as_str(), "STOP ACCEPTED - NO NEW GRANTS; END UNCONFIRMED");
        assert_eq!(u.intent(Action::Approve), None);
        assert_eq!(u.intent(Action::Deny), None);
        assert_eq!(u.task.state, TaskState::StopRequested);
        assert!(u.observe(BackendEvent::Stopped { task_id: 7, seq: 4 }));
        assert_eq!(u.task.state, TaskState::Stopped);
        assert!(!u.controls().permission_visible);
    }

    #[test]
    fn stop_ack_retracts_existing_prompt_and_late_grant_ack_is_rejected() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 2, request: 17,
            action: "REPLACE", target: "/APPS/old",
        }));
        assert_eq!(u.intent(Action::Approve), Some(Intent::Decide {
            task_id: 7, request: 17, allow: true,
        }));
        assert!(u.controls().permission_visible);
        assert_eq!(u.decision_pending, Some(17));
        assert!(u.observe(BackendEvent::StopAccepted { task_id: 7, seq: 3 }));
        assert!(u.permission.is_none());
        assert!(u.decision_pending.is_none());
        assert!(!u.controls().permission_visible);
        assert_eq!(u.intent(Action::Approve), None);
        assert_eq!(u.intent(Action::Deny), None);
        // A delayed successful service decision must not re-open or imply
        // that the UI granted permission after Stop was acknowledged.
        assert!(!u.observe(BackendEvent::PermissionResolved {
            task_id: 7, seq: 4, request: 17, allowed: true,
        }));
        assert_eq!(u.task.seq, 3);
        assert!(!u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 4, request: 18,
            action: "CREATE", target: "/APPS/new",
        }));
        assert!(u.observe(BackendEvent::Stopped { task_id: 7, seq: 5 }));
        assert!(u.permission.is_none() && u.decision_pending.is_none());
    }

    #[test]
    fn no_approval_after_stop_request() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 2, request: 1, action: "WRITE", target: "/TMP",
        }));
        assert!(u.observe(BackendEvent::StopAccepted { task_id: 7, seq: 3 }));
        assert!(u.intent(Action::Approve).is_none());
    }
    #[test]
    fn disconnect_is_not_success_or_confirmed_stop() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::Service {
            connected: false, model: Model::Unavailable, saved_mode: Mode::Single,
        }));
        assert_eq!(u.task.state, TaskState::Unconfirmed);
        assert!(!u.controls().stop_enabled);
        assert!(!u.observe(BackendEvent::Stopped { task_id: 7, seq: 2 }));
    }
    #[test]
    fn reconnect_cannot_accept_unverified_progress_but_can_settle_terminal_state() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::Output {
            task_id: 7, seq: 2, text: "before loss",
        }));
        assert!(u.observe(BackendEvent::Service {
            connected: false, model: Model::Unknown, saved_mode: Mode::Single,
        }));
        assert_eq!(u.task.state, TaskState::Unconfirmed);
        assert!(!u.observe(BackendEvent::Stopped { task_id: 7, seq: 3 }));

        assert!(u.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        assert_eq!(u.task.state, TaskState::Unconfirmed);
        assert!(!u.controls().submit_enabled);
        assert!(!u.controls().decision_enabled);
        assert!(u.controls().stop_visible);
        assert!(u.controls().stop_enabled);
        assert_eq!(u.intent(Action::Stop), Some(Intent::Stop { task_id: 7 }));
        assert_eq!(u.task.state, TaskState::Unconfirmed);

        assert!(!u.observe(BackendEvent::Output {
            task_id: 7, seq: 3, text: "do not resume output",
        }));
        assert!(!u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 3, request: 99,
            action: "DELETE", target: "/APPS",
        }));
        assert!(!u.observe(BackendEvent::Started { task_id: 8, seq: 1 }));
        assert!(!u.observe(BackendEvent::Stopped { task_id: 8, seq: 3 }));
        assert!(!u.observe(BackendEvent::Stopped { task_id: 7, seq: 2 }));
        assert!(u.observe(BackendEvent::Stopped { task_id: 7, seq: 3 }));
        assert_eq!(u.task.state, TaskState::Stopped);
        assert_eq!(u.task.seq, 3);
        assert!(!u.controls().stop_enabled);
        assert!(!u.observe(BackendEvent::Completed { task_id: 7, seq: 4 }));
        assert!(u.observe(BackendEvent::Started { task_id: 8, seq: 1 }));
        assert_eq!(u.task.state, TaskState::Running);
    }

    #[test]
    fn reconnected_unknown_task_accepts_stop_ack_without_claiming_termination() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::Output { task_id: 7, seq: 2, text: "working" }));
        assert!(u.observe(BackendEvent::Service {
            connected: false, model: Model::Unknown, saved_mode: Mode::Single,
        }));
        assert!(u.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        assert_eq!(u.task.state, TaskState::Unconfirmed);
        assert_eq!(u.intent(Action::Stop), Some(Intent::Stop { task_id: 7 }));
        assert!(!u.observe(BackendEvent::StopAccepted { task_id: 8, seq: 3 }));
        assert!(!u.observe(BackendEvent::StopAccepted { task_id: 7, seq: 2 }));
        assert!(u.observe(BackendEvent::StopAccepted { task_id: 7, seq: 3 }));
        assert_eq!(u.task.state, TaskState::StopRequested);
        assert_eq!(u.task.seq, 3);
        assert_eq!(u.notice.as_str(), "STOP ACCEPTED - NO NEW GRANTS; END UNCONFIRMED");
        assert!(!u.controls().stop_enabled);
        assert!(!u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 4, request: 99,
            action: "WRITE", target: "/APPS/blocked",
        }));
        assert!(u.observe(BackendEvent::Stopped { task_id: 7, seq: 5 }));
        assert_eq!(u.task.state, TaskState::Stopped);
    }

    #[test]
    fn reconnect_terminal_reconciliation_accepts_failure_not_new_grants() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 2, request: 10,
            action: "REPLACE", target: "/APPS/example",
        }));
        assert!(u.intent(Action::Approve).is_some());
        assert!(u.observe(BackendEvent::Service {
            connected: false, model: Model::Unavailable, saved_mode: Mode::Single,
        }));
        assert!(u.observe(BackendEvent::Service {
            connected: true, model: Model::Ready, saved_mode: Mode::Single,
        }));
        assert_eq!(u.task.state, TaskState::Unconfirmed);
        assert!(u.permission.is_none() && u.decision_pending.is_none());
        assert_eq!(u.intent(Action::Approve), None);
        assert!(!u.observe(BackendEvent::PermissionResolved {
            task_id: 7, seq: 3, request: 10, allowed: true,
        }));
        assert!(u.observe(BackendEvent::Failed {
            task_id: 7, seq: 3, reason: "service readback: failed",
        }));
        assert_eq!(u.task.state, TaskState::Failed);
        assert!(!u.controls().stop_enabled);
    }

    #[test]
    fn completed_task_cannot_restart_with_the_same_id() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::Completed { task_id: 7, seq: 2 }));
        assert_eq!(u.task.state, TaskState::Completed);
        let prior_lines = *u.lines().last().unwrap();
        assert!(!u.observe(BackendEvent::Started { task_id: 7, seq: 1 }));
        assert!(!u.observe(BackendEvent::Started { task_id: 7, seq: 99 }));
        assert_eq!(u.task.id, 7);
        assert_eq!(u.task.seq, 2);
        assert_eq!(u.task.state, TaskState::Completed);
        assert_eq!(u.lines().last(), Some(&prior_lines));
        assert!(u.observe(BackendEvent::Started { task_id: 8, seq: 1 }));
        assert_eq!(u.task.state, TaskState::Running);
        assert_eq!(u.task.id, 8);
    }

    #[test]
    fn stopped_and_failed_tasks_cannot_restart_with_the_same_id() {
        for state in [TaskState::Stopped, TaskState::Failed] {
            let mut u = Ui::new();
            running(&mut u);
            let done = match state {
                TaskState::Stopped => u.observe(BackendEvent::Stopped { task_id: 7, seq: 2 }),
                TaskState::Failed => u.observe(BackendEvent::Failed {
                    task_id: 7, seq: 2, reason: "service reported failure",
                }),
                _ => unreachable!(),
            };
            assert!(done);
            assert_eq!(u.task.state, state);
            assert!(!u.observe(BackendEvent::Started { task_id: 7, seq: 1 }));
            assert!(!u.observe(BackendEvent::Started { task_id: 7, seq: 3 }));
            assert_eq!(u.task.state, state);
            assert!(u.observe(BackendEvent::Started { task_id: 8, seq: 1 }));
            assert_eq!(u.task.id, 8);
        }
    }

    #[test]
    fn later_output_and_terminal_events_cannot_rewrite_result() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::Completed { task_id: 7, seq: 2 }));
        assert!(!u.observe(BackendEvent::Stopped { task_id: 7, seq: 3 }));
        assert!(!u.observe(BackendEvent::Output { task_id: 7, seq: 3, text: "late" }));
        assert_eq!(u.task.state, TaskState::Completed);
    }
    #[test]
    fn bounded_feed_retains_only_latest_ordered_events() {
        let mut u = Ui::new();
        running(&mut u);
        for seq in 2..20 {
            assert!(u.observe(BackendEvent::ToolResult { task_id: 7, seq, text: "tool" }));
        }
        assert_eq!(u.lines().len(), LOG_LIMIT);
        assert_eq!(u.lines()[0].seq, 12);
        assert_eq!(u.lines()[LOG_LIMIT - 1].seq, 19);
    }
    #[test]
    fn pending_permission_choice_is_sent_once_even_with_contradictory_keys() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 2, request: 19,
            action: "REPLACE", target: "/APPS/bird.txt",
        }));
        assert!(u.controls().permission_visible && u.controls().decision_enabled);
        assert_eq!(u.intent(Action::Approve), Some(Intent::Decide {
            task_id: 7, request: 19, allow: true,
        }));
        assert_eq!(u.decision_pending, Some(19));
        assert!(!u.controls().decision_enabled);
        assert_eq!(u.intent(Action::Approve), None);
        assert_eq!(u.intent(Action::Deny), None);
        assert_eq!(u.permission.unwrap().request, 19);
        // A stale/mismatched service event cannot unlock a second GUI decision.
        assert!(!u.observe(BackendEvent::PermissionResolved {
            task_id: 7, seq: 3, request: 20, allowed: false,
        }));
        assert!(!u.observe(BackendEvent::PermissionResolved {
            task_id: 7, seq: 2, request: 19, allowed: false,
        }));
        assert_eq!(u.decision_pending, Some(19));
        assert!(u.observe(BackendEvent::PermissionResolved {
            task_id: 7, seq: 3, request: 19, allowed: true,
        }));
        assert!(u.permission.is_none() && u.decision_pending.is_none());

        assert!(u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 4, request: 20, action: "CREATE", target: "/APPS/other",
        }));
        assert_eq!(u.intent(Action::Deny), Some(Intent::Decide {
            task_id: 7, request: 20, allow: false,
        }));
        assert!(u.controls().permission_visible);
        assert!(!u.controls().decision_enabled);
    }

    #[test]
    fn stop_or_disconnect_invalidates_pending_gui_decisions() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 2, request: 25,
            action: "WRITE", target: "/TMP/test",
        }));
        assert!(u.intent(Action::Deny).is_some());
        assert!(u.observe(BackendEvent::StopAccepted { task_id: 7, seq: 3 }));
        assert_eq!(u.intent(Action::Approve), None);
        assert_eq!(u.intent(Action::Deny), None);
        assert!(u.observe(BackendEvent::Stopped { task_id: 7, seq: 4 }));
        assert!(u.permission.is_none() && u.decision_pending.is_none());
        assert_eq!(u.intent(Action::Approve), None);

        assert!(u.observe(BackendEvent::Started { task_id: 8, seq: 1 }));
        assert!(u.observe(BackendEvent::PermissionNeeded {
            task_id: 8, seq: 2, request: 26,
            action: "DELETE", target: "/TMP/test",
        }));
        assert!(u.intent(Action::Approve).is_some());
        assert!(u.observe(BackendEvent::Service {
            connected: false, model: Model::Unavailable, saved_mode: Mode::Single,
        }));
        assert!(u.permission.is_none() && u.decision_pending.is_none());
        assert_eq!(u.task.state, TaskState::Unconfirmed);
        assert_eq!(u.intent(Action::Approve), None);
        assert_eq!(u.intent(Action::Deny), None);
    }

    #[test]
    fn explicit_deny_followed_by_reported_grant_flags_contradiction() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 2, request: 21,
            action: "REPLACE", target: "/APPS/bird.txt",
        }));
        assert_eq!(u.intent(Action::Deny), Some(Intent::Decide {
            task_id: 7, request: 21, allow: false,
        }));
        assert_eq!(u.decision_choice, Some(false));
        assert!(u.observe(BackendEvent::PermissionResolved {
            task_id: 7, seq: 3, request: 21, allowed: true,
        }));
        assert_eq!(u.task.seq, 3);
        assert_eq!(u.task.state, TaskState::Unconfirmed);
        assert_eq!(u.lines().last().unwrap().kind, "ANOMALY");
        assert!(u.controls().stop_enabled);
        assert!(!u.observe(BackendEvent::ToolResult {
            task_id: 7, seq: 4, text: "unverified result",
        }));
        assert!(!u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 4, request: 22,
            action: "WRITE", target: "/APPS/new",
        }));
        assert!(u.observe(BackendEvent::Failed {
            task_id: 7, seq: 4, reason: "verified service failure",
        }));
        assert_eq!(u.task.state, TaskState::Failed);
        assert!(!u.controls().stop_enabled);
    }

    #[test]
    fn denial_after_allow_and_grant_after_allow_are_distinguished() {
        for (allowed, expected) in [
            (false, "SERVICE REPORTED DENIAL"),
            (true, "SERVICE REPORTED GRANT"),
        ] {
            let mut u = Ui::new();
            running(&mut u);
            assert!(u.observe(BackendEvent::PermissionNeeded {
                task_id: 7, seq: 2, request: 33,
                action: "CREATE", target: "/APPS/demo",
            }));
            assert_eq!(u.intent(Action::Approve), Some(Intent::Decide {
                task_id: 7, request: 33, allow: true,
            }));
            assert_eq!(u.decision_choice, Some(true));
            assert!(u.observe(BackendEvent::PermissionResolved {
                task_id: 7, seq: 3, request: 33, allowed,
            }));
            assert_eq!(u.task.state, TaskState::Running);
            assert_eq!(u.lines().last().unwrap().kind, "DECISION");
            assert_eq!(u.lines().last().unwrap().message.as_str(), expected);
            assert!(u.permission.is_none());
            assert!(u.decision_pending.is_none());
            assert!(u.decision_choice.is_none());
            // A service event is a report. It does not independently show a
            // completed file write or a host-enforced permission decision.
        }
    }

    #[test]
    fn unapproved_service_ack_does_not_imply_gui_authorization() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 2, request: 9, action: "READ", target: "/HOME",
        }));
        assert!(u.observe(BackendEvent::PermissionResolved {
            task_id: 7, seq: 3, request: 9, allowed: true,
        }));
        // A grant without an Allow intent is a contradiction visible to the
        // person, not evidence of a GUI authorization or confirmed effect.
        assert_eq!(u.lines().last().unwrap().kind, "ANOMALY");
        assert_eq!(u.lines().last().unwrap().message.as_str(),
            "SERVICE GRANT WITHOUT GUI CONSENT");
        assert_eq!(u.task.state, TaskState::Unconfirmed);
        assert_eq!(u.notice.as_str(), "SERVICE GRANT MISMATCH - EFFECT UNKNOWN");
        assert!(u.permission.is_none() && u.decision_pending.is_none());
        assert!(!u.controls().decision_enabled);
        assert!(!u.controls().submit_enabled);
        assert!(u.controls().stop_enabled);
        assert!(u.observe(BackendEvent::Stopped { task_id: 7, seq: 4 }));
        assert_eq!(u.task.state, TaskState::Stopped);
    }
}
