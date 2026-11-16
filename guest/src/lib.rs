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
}

pub struct Ui {
    pub input: Text<INPUT_LIMIT>,
    pub mode: Mode,
    pub mode_pending: Option<Mode>,
    pub model: Model,
    pub connected: bool,
    pub task: Task,
    pub permission: Option<Grant>,
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
            permission: None, notice: Text::new(),
            lines: [Line::empty(); LOG_LIMIT], count: 0,
        }
    }
    pub fn controls(&self) -> Controls {
        Controls {
            mode_visible: true, stop_visible: true,
            stop_enabled: self.connected && self.task.state == TaskState::Running,
            submit_enabled: self.connected && self.model == Model::Ready
                && !self.input.is_empty() && !self.task.state.is_active(),
            permission_visible: self.permission.is_some(),
        }
    }
    pub fn lines(&self) -> &[Line] { &self.lines[..self.count] }
    fn notice(&mut self, s: &str) { self.notice = Text::display(s); }
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
                if self.connected && self.task.state == TaskState::Running && self.permission.is_some() =>
            {
                let g = self.permission.unwrap();
                self.notice("PERMISSION DECISION NOT YET CONFIRMED");
                Some(Intent::Decide {
                    task_id: self.task.id, request: g.request,
                    allow: action == Action::Approve,
                })
            }
            Action::Submit => { self.notice("NO MODEL SERVICE - OS STILL USABLE"); None }
            Action::ToggleMode => { self.notice("MODE CHANGE UNAVAILABLE"); None }
            Action::Stop => { self.notice("NO STOPPABLE TASK - USE OS STOP KEY"); None }
            Action::Approve | Action::Deny => { self.notice("NO VALID PERMISSION REQUEST"); None }
        }
    }
    pub fn observe(&mut self, event: BackendEvent<'_>) -> bool {
        match event {
            BackendEvent::Service { connected, model, saved_mode } => {
                self.connected = connected;
                self.model = if connected { model } else { Model::Unknown };
                self.mode = saved_mode;
                self.mode_pending = None;
                if !connected {
                    if self.task.state.is_active() {
                        self.task.state = TaskState::Unconfirmed;
                    }
                    self.permission = None;
                    self.notice("SERVICE LOST - NO STOP CONFIRMATION");
                } else {
                    self.notice("SERVICE STATUS RECEIVED");
                }
                true
            }
            BackendEvent::ModeCommitted(mode) if self.connected => {
                self.mode = mode;
                self.mode_pending = None;
                self.notice("MODE CONFIRMED BY SERVICE");
                true
            }
            BackendEvent::Started { task_id, seq }
                if self.connected && task_id != 0 && seq > 0 && !self.task.state.is_active() =>
            {
                self.task = Task { id: task_id, seq, state: TaskState::Running };
                self.permission = None;
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
                if request == 0 || self.permission.is_some() || !self.accept(task_id, seq) {
                    return false;
                }
                self.permission = Some(Grant {
                    request, action: Text::display(action), target: Text::display(target),
                });
                self.log(seq, "PERMISSION", "DECISION REQUIRED");
                self.notice("PERMISSION PENDING - DEFAULT DENY");
                true
            }
            BackendEvent::PermissionResolved { task_id, seq, request, allowed } => {
                if self.permission.map(|p| p.request) != Some(request) || !self.accept(task_id, seq) {
                    return false;
                }
                self.permission = None;
                self.log(seq, "DECISION", if allowed { "SERVICE REPORTED GRANT" } else { "SERVICE REPORTED DENIAL" });
                true
            }
            BackendEvent::StopAccepted { task_id, seq } => {
                if !self.accept(task_id, seq) { return false; }
                self.task.state = TaskState::StopRequested;
                self.notice("STOP REQUEST ACCEPTED - END UNCONFIRMED");
                self.log(seq, "STOP", "REQUEST ACCEPTED BY SERVICE");
                true
            }
            BackendEvent::Stopped { task_id, seq } => {
                if !self.accept(task_id, seq) { return false; }
                self.task.state = TaskState::Stopped;
                self.permission = None;
                self.log(seq, "STOPPED", "SERVICE REPORTED TERMINATION");
                self.notice("SERVICE REPORTS STOPPED");
                true
            }
            BackendEvent::Completed { task_id, seq } => {
                if !self.accept(task_id, seq) { return false; }
                self.task.state = TaskState::Completed;
                self.permission = None;
                self.log(seq, "DONE", "SERVICE REPORTED COMPLETION");
                self.notice("SERVICE REPORTS COMPLETE");
                true
            }
            BackendEvent::Failed { task_id, seq, reason } => {
                if !self.accept(task_id, seq) { return false; }
                self.task.state = TaskState::Failed;
                self.permission = None;
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
            task_id: 7, seq: 2, request: 14, action: "WRITE FILE", target: "/apps/test",
        }));
        assert!(u.controls().permission_visible);
        assert_eq!(u.permission.unwrap().target.as_str(), "/apps/test");
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
    fn no_approval_after_stop_request() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 2, request: 1, action: "WRITE", target: "/tmp",
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
    fn unapproved_service_ack_does_not_imply_gui_authorization() {
        let mut u = Ui::new();
        running(&mut u);
        assert!(u.observe(BackendEvent::PermissionNeeded {
            task_id: 7, seq: 2, request: 9, action: "READ", target: "/home",
        }));
        assert!(u.observe(BackendEvent::PermissionResolved {
            task_id: 7, seq: 3, request: 9, allowed: true,
        }));
        // This is a service *report*, never evidence that this GUI granted permission.
        assert_eq!(u.lines().last().unwrap().message.as_str(), "SERVICE REPORTED GRANT");
    }
}
