//! Bounded presentation state for one task. This is not PT's wire protocol.
//! The caller must authenticate events and read back effects before presenting
//! them here. Nothing in this module executes an action or grants permission.
use crate::Text;

pub const MAX_STEPS: usize = 6;
pub const PREVIEW_WIDTH: usize = 64;
pub const PREVIEW_HEIGHT: usize = 40;
pub const PREVIEW_PIXELS: usize = PREVIEW_WIDTH * PREVIEW_HEIGHT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase { Ready, Running, StopRequested, Stopped, Failed, Disconnected, Completed }
impl Phase {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready to begin", Self::Running => "Work in progress",
            Self::StopRequested => "Stop requested", Self::Stopped => "Stopped",
            Self::Failed => "Work failed", Self::Disconnected => "Connection lost",
            Self::Completed => "Work complete",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState { Queued, Active, Waiting, Done, Failed, Stopped }
impl StepState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Queued => "Not started", Self::Active => "Working",
            Self::Waiting => "Waiting", Self::Done => "Done",
            Self::Failed => "Failed", Self::Stopped => "Stopped",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub label: Text<24>, pub state: StepState,
    pub done: u64, pub total: Option<u64>, pub reported: bool,
}
impl Step {
    const EMPTY: Self = Self { label: Text::new(), state: StepState::Queued, done: 0, total: None, reported: false };
    /// None means no denominator was reported; never infer a percentage.
    pub fn percent(&self) -> Option<u8> {
        self.total.map(|total| ((self.done as u128 * 100) / total as u128) as u8)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeState { Proposed, Applied }

pub struct Preview {
    pub revision: u64, pub location: Text<48>, pub change: ChangeState,
    /// An overview snapshot, not exact artifact/source bytes or an executable.
    pub pixels: [u32; PREVIEW_PIXELS],
}

pub enum Update<'a> {
    Begin,
    Start { step: usize },
    Progress { step: usize, done: u64, total: Option<u64> },
    Wait { step: usize, reason: &'a str },
    Finish { step: usize },
    Preview { revision: u64, location: &'a str, change: ChangeState, pixels: &'a [u32] },
    Stopped,
    Failed { reason: &'a str },
    Completed,
}

pub struct Work {
    task: u64, seq: u64, phase: Phase,
    title: Text<48>, steps: [Step; MAX_STEPS], count: usize,
    preview: Option<Preview>, detail: Text<48>, demo: bool,
}

fn exact<const N: usize>(s: &str) -> Option<Text<N>> {
    if s.is_empty() || s.len() > N { return None; }
    let mut text = Text::new();
    for b in s.bytes() { if !text.push_ascii(b) { return None; } }
    Some(text)
}

impl Work {
    /// A Work instance binds one nonzero task. Construct another for a new
    /// task; the UI must not silently replace a running task's identity.
    pub fn new(task: u64, title: &str, labels: &[&str], demo: bool) -> Option<Self> {
        if task == 0 || labels.is_empty() || labels.len() > MAX_STEPS { return None; }
        let mut steps = [Step::EMPTY; MAX_STEPS];
        for (step, label) in steps.iter_mut().zip(labels) { step.label = exact(label)?; }
        Some(Self { task, seq: 0, phase: Phase::Ready, title: exact(title)?,
                    steps, count: labels.len(), preview: None, detail: Text::new(), demo })
    }
    pub fn task(&self) -> u64 { self.task }
    pub fn sequence(&self) -> u64 { self.seq }
    pub fn phase(&self) -> Phase { self.phase }
    pub fn title(&self) -> &str { self.title.as_str() }
    pub fn steps(&self) -> &[Step] { &self.steps[..self.count] }
    pub fn preview(&self) -> Option<&Preview> { self.preview.as_ref() }
    pub fn detail(&self) -> &str { self.detail.as_str() }
    pub fn is_demo(&self) -> bool { self.demo }
    pub fn current(&self) -> Option<usize> {
        self.steps().iter().position(|s| matches!(s.state, StepState::Active | StepState::Waiting))
    }
    pub fn preview_is_partial(&self) -> bool { self.phase != Phase::Completed }
    fn terminal(&self) -> bool { matches!(self.phase, Phase::Completed | Phase::Failed | Phase::Stopped) }

    /// Local presentation only. The controller must separately request the
    /// worker stop; acknowledgement is not evidence of termination.
    pub fn request_stop(&mut self) -> bool {
        if self.phase != Phase::Running { return false; }
        self.phase = Phase::StopRequested;
        self.detail = exact("Waiting for the worker to stop").unwrap();
        true
    }
    /// Freeze progress and preserve the last preview. Reconnection needs a
    /// reconciled service snapshot; ordinary late output cannot reopen work.
    pub fn disconnect(&mut self) -> bool {
        if !matches!(self.phase, Phase::Running | Phase::StopRequested) { return false; }
        self.phase = Phase::Disconnected;
        self.detail = exact(if self.preview.is_some() { "Last received preview; work status unknown" }
                            else { "No preview received; work status unknown" }).unwrap();
        true
    }
    pub fn observe(&mut self, task: u64, seq: u64, update: Update<'_>) -> bool {
        if task != self.task || seq <= self.seq || self.terminal() { return false; }
        if !matches!(update, Update::Stopped | Update::Failed { .. } | Update::Completed)
            && matches!(self.phase, Phase::StopRequested | Phase::Disconnected) { return false; }
        match update {
            Update::Begin if self.phase == Phase::Ready => self.phase = Phase::Running,
            Update::Start { step } if self.phase == Phase::Running && step < self.count => {
                let current = self.current();
                if current.is_some() && current != Some(step) { return false; }
                if !matches!(self.steps[step].state, StepState::Queued | StepState::Waiting) { return false; }
                if self.steps[..step].iter().any(|s| s.state != StepState::Done) { return false; }
                self.steps[step].state = StepState::Active;
                self.detail.clear();
            }
            Update::Progress { step, done, total } if self.phase == Phase::Running && self.current() == Some(step) => {
                let item = &mut self.steps[step];
                if item.state != StepState::Active || done < item.done
                    || total.is_some_and(|n| n == 0 || done > n)
                    || (item.total.is_some() && item.total != total) { return false; }
                item.done = done; item.total = total; item.reported = true;
            }
            Update::Wait { step, reason } if self.phase == Phase::Running && self.current() == Some(step) => {
                let Some(detail) = exact(reason) else { return false; };
                self.steps[step].state = StepState::Waiting; self.detail = detail;
            }
            Update::Finish { step } if self.phase == Phase::Running && self.current() == Some(step) => {
                let item = &mut self.steps[step];
                if item.state != StepState::Active || item.total.is_some_and(|n| n != item.done) { return false; }
                item.state = StepState::Done; self.detail.clear();
            }
            Update::Preview { revision, location, change, pixels } if self.phase == Phase::Running => {
                let Some(location) = exact::<48>(location) else { return false; };
                if revision == 0 || pixels.len() != PREVIEW_PIXELS || pixels.iter().any(|p| p & 0xff000000 != 0) { return false; }
                if let Some(old) = &self.preview {
                    if revision <= old.revision || old.location != location
                        || (old.change == ChangeState::Applied && change != ChangeState::Applied) { return false; }
                }
                let mut bitmap = [0; PREVIEW_PIXELS]; bitmap.copy_from_slice(pixels);
                self.preview = Some(Preview { revision, location, change, pixels: bitmap });
            }
            Update::Stopped if self.phase != Phase::Ready => {
                for step in &mut self.steps[..self.count] {
                    if matches!(step.state, StepState::Active | StepState::Waiting) { step.state = StepState::Stopped; }
                }
                self.phase = Phase::Stopped;
                self.detail = exact(if self.preview.is_some() { "Worker stopped; partial preview retained" }
                                    else { "Worker stopped before any preview arrived" }).unwrap();
            }
            Update::Failed { reason } if self.phase != Phase::Ready => {
                let Some(detail) = exact(reason) else { return false; };
                for step in &mut self.steps[..self.count] {
                    if matches!(step.state, StepState::Active | StepState::Waiting) { step.state = StepState::Failed; }
                }
                self.phase = Phase::Failed; self.detail = detail;
            }
            Update::Completed if self.phase != Phase::Ready && self.steps().iter().all(|s| s.state == StepState::Done) => {
                self.phase = Phase::Completed;
                self.detail = exact("Result received").unwrap();
            }
            _ => return false,
        }
        self.seq = seq;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn work() -> Work { Work::new(7, "Build a card", &["Build", "Check"], true).unwrap() }
    fn started() -> Work {
        let mut w = work(); assert!(w.observe(7, 1, Update::Begin));
        assert!(w.observe(7, 2, Update::Start { step: 0 })); w
    }
    #[test]
    fn progress_is_measured_and_completion_is_explicit() {
        let mut w = started(); assert_eq!(w.steps()[0].percent(), None);
        assert!(!w.steps()[0].reported);
        assert!(w.observe(7, 3, Update::Progress { step: 0, done: 1, total: Some(2) }));
        assert_eq!(w.steps()[0].percent(), Some(50));
        assert!(w.steps()[0].reported);
        assert!(!w.observe(7, 4, Update::Completed));
        assert!(!w.observe(7, 4, Update::Finish { step: 0 }));
        assert!(w.observe(7, 4, Update::Progress { step: 0, done: 2, total: Some(2) }));
        assert!(w.observe(7, 5, Update::Finish { step: 0 }));
        assert!(w.observe(7, 6, Update::Start { step: 1 }));
        assert!(w.observe(7, 7, Update::Finish { step: 1 }));
        assert_eq!(w.phase(), Phase::Running);
        assert!(w.observe(7, 8, Update::Completed));
        assert!(!w.observe(7, 9, Update::Begin));
    }
    #[test]
    fn rejects_misbound_stale_invalid_and_regressive_events_without_advancing() {
        let mut w = started();
        assert!(!w.observe(8, 3, Update::Failed { reason: "wrong task" }));
        assert!(!w.observe(7, 2, Update::Failed { reason: "old" }));
        assert!(!w.observe(7, 3, Update::Start { step: 1 }));
        assert!(!w.observe(7, 3, Update::Progress { step: 0, done: 1, total: Some(0) }));
        assert_eq!(w.sequence(), 2);
        assert!(w.observe(7, 3, Update::Progress { step: 0, done: u64::MAX-1, total: Some(u64::MAX) }));
        assert_eq!(w.steps()[0].percent(), Some(99));
        assert!(!w.observe(7, 4, Update::Progress { step: 0, done: 1, total: None }));
        assert_eq!(w.sequence(), 3);
    }
    #[test]
    fn interrupted_work_keeps_partial_preview_and_rejects_late_ordinary_updates() {
        for stop in [false, true] {
            let mut w = started(); let pixels = [0x123456; PREVIEW_PIXELS];
            assert!(w.observe(7, 3, Update::Preview { revision: 1, location: "card", change: ChangeState::Proposed, pixels: &pixels }));
            if stop { assert!(w.request_stop()); } else { assert!(w.disconnect()); }
            assert!(!w.observe(7, 4, Update::Progress { step: 0, done: 2, total: Some(2) }));
            assert!(!w.observe(7, 4, Update::Completed));
            assert_eq!(w.preview().unwrap().pixels, pixels);
            assert!(w.preview_is_partial());
            assert!(w.observe(7, 4, Update::Stopped));
            assert_eq!(w.phase(), Phase::Stopped);
            assert!(!w.observe(7, 5, Update::Start { step: 0 }));
        }
    }
    #[test]
    fn preview_identity_revision_and_exact_labels_cannot_silently_change() {
        let mut w = started(); let pixels = [0; PREVIEW_PIXELS];
        assert!(w.observe(7, 3, Update::Preview { revision: 1, location: "card", change: ChangeState::Applied, pixels: &pixels }));
        for (revision, location, change, data) in [(1, "card", ChangeState::Applied, &pixels[..]),
            (2, "other", ChangeState::Applied, &pixels[..]), (2, "card", ChangeState::Proposed, &pixels[..]),
            (2, "card", ChangeState::Applied, &pixels[..1]), (2, "card\n", ChangeState::Applied, &pixels[..])] {
            assert!(!w.observe(7, 4, Update::Preview { revision, location, change, pixels: data }));
        }
        assert_eq!(w.sequence(), 3);
        assert!(Work::new(0, "task", &["step"], true).is_none());
        assert!(Work::new(1, "task", &[], true).is_none());
        assert!(Work::new(1, "task\n", &["step"], true).is_none());
    }
    #[test]
    fn waiting_and_failure_keep_the_last_known_work() {
        let mut w = started();
        assert!(w.observe(7, 3, Update::Wait { step: 0, reason: "Waiting for input" }));
        assert!(!w.observe(7, 4, Update::Progress { step: 0, done: 1, total: Some(2) }));
        assert!(w.observe(7, 4, Update::Start { step: 0 }));
        assert!(w.observe(7, 5, Update::Failed { reason: "Build check failed" }));
        assert_eq!(w.steps()[0].state, StepState::Failed);
        assert_eq!(w.steps()[1].state, StepState::Queued);
        assert!(!w.observe(7, 6, Update::Completed));
    }
}
