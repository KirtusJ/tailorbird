//! Explicit sample events for visual development. Never used by the normal
//! Tailorbird binary; no model, file write, permission grant or real task.
use crate::live_work::*;

pub struct Demo { pub work: Work, frame: u8 }
impl Demo {
    pub fn new() -> Self {
        Self { work: Work::new(7, "Build a greeting card", &["Draft layout", "Build preview", "Check result", "Prepare output"], true).unwrap(), frame: 0 }
    }
    fn send(&mut self, update: Update<'_>) -> bool {
        self.work.observe(7, self.work.sequence()+1, update)
    }
    fn preview(&mut self, revision: u64, applied: bool) {
        let pixels = sample_card(revision);
        self.send(Update::Preview { revision, location: "demo/greeting.card", change: if applied { ChangeState::Applied } else { ChangeState::Proposed }, pixels: &pixels });
    }
    pub fn advance(&mut self) {
        if !matches!(self.work.phase(), Phase::Ready | Phase::Running) { return; }
        if self.work.current().is_some_and(|i| self.work.steps()[i].state == StepState::Waiting) {
            self.send(Update::Start { step: self.work.current().unwrap() }); return;
        }
        match self.frame {
            0 => { self.send(Update::Begin); self.send(Update::Start { step: 0 }); }
            1 => { self.send(Update::Progress { step: 0, done: 1, total: Some(3) }); self.preview(1, false); }
            2 => { self.send(Update::Progress { step: 0, done: 2, total: Some(3) }); self.preview(2, false); }
            3 => { self.send(Update::Progress { step: 0, done: 3, total: Some(3) }); self.send(Update::Finish { step: 0 }); self.send(Update::Start { step: 1 }); self.preview(3, false); }
            4 => { self.send(Update::Finish { step: 1 }); self.send(Update::Start { step: 2 }); self.send(Update::Progress { step: 2, done: 1, total: Some(2) }); }
            5 => { self.send(Update::Progress { step: 2, done: 2, total: Some(2) }); self.send(Update::Finish { step: 2 }); self.send(Update::Start { step: 3 }); }
            6 => { self.preview(4, true); self.send(Update::Finish { step: 3 }); self.send(Update::Completed); }
            _ => return,
        }
        self.frame += 1;
    }
    pub fn wait(&mut self) { if let Some(step) = self.work.current() { self.send(Update::Wait { step, reason: "Sample wait / Space resumes" }); } }
    pub fn request_stop(&mut self) { self.work.request_stop(); }
    pub fn confirm_stop(&mut self) { self.send(Update::Stopped); }
    pub fn disconnect(&mut self) { self.work.disconnect(); }
    pub fn fail(&mut self) { self.send(Update::Failed { reason: "Sample build check failed" }); }
    pub fn key(&mut self, code: usize) -> bool {
        match code {
            0x39 => self.advance(), 0x11 => self.wait(), 0x1f => self.request_stop(),
            0x2e => self.confirm_stop(), 0x20 => self.disconnect(), 0x21 => self.fail(),
            0x13 => *self = Self::new(), _ => return false,
        }
        true
    }
}
impl Default for Demo { fn default() -> Self { Self::new() } }

fn sample_card(revision: u64) -> [u32; PREVIEW_PIXELS] {
    let mut p = [0xe8eef3; PREVIEW_PIXELS];
    let mut rect = |x: usize, y: usize, w: usize, h: usize, c| {
        for row in y..y+h { p[row*PREVIEW_WIDTH+x..row*PREVIEW_WIDTH+x+w].fill(c); }
    };
    rect(0, 0, 64, 5, 0x18364f);
    rect(4, 9, 56, 27, 0xffffff);
    if revision >= 2 {
        rect(8, 13, 48, 4, 0x41bad7);
        rect(8, 20, 35, 2, 0x9fb4c8);
        rect(8, 24, 44, 2, 0x9fb4c8);
        rect(40, 29, 16, 4, 0x18364f);
    }
    if revision >= 3 {
        rect(8, 13, 48, 14, 0xf2f7fc);
        // A tiny raster HELLO is artifact data, not a renderer special case.
        let letters = [[17,17,17,31,17,17,17], [31,16,16,30,16,16,31],
                       [16,16,16,16,16,16,31], [16,16,16,16,16,16,31], [14,17,17,17,17,17,14]];
        for (i, glyph) in letters.iter().enumerate() { for (y, row) in glyph.iter().enumerate() {
            for x in 0..5 { if row & (1 << (4-x)) != 0 { p[(16+y)*64+17+i*6+x] = 0x18364f; } }
        }}
    }
    if revision >= 4 { for y in 29..33 { p[y*64+40..y*64+56].fill(0x8fe0c2); } }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_changes_before_completion_and_remains_marked_as_sample() {
        let mut d = Demo::new(); d.advance(); d.advance();
        let first = d.work.preview().unwrap().pixels;
        assert_eq!(d.work.phase(), Phase::Running); assert!(d.work.is_demo());
        d.advance(); assert_ne!(d.work.preview().unwrap().pixels, first);
        for _ in 0..4 { d.advance(); }
        assert_eq!(d.work.phase(), Phase::Completed); assert!(d.work.is_demo());
        assert_eq!(d.work.preview().unwrap().change, ChangeState::Applied);
    }
    #[test]
    fn early_interruptions_never_claim_a_retained_preview() {
        for key in [0x2e, 0x20, 0x21] {
            let mut d = Demo::new(); d.advance();
            if key == 0x2e { d.request_stop(); }
            d.key(key);
            assert!(d.work.preview().is_none());
            assert!(!d.work.detail().contains("retained"));
            assert!(!d.work.detail().contains("Last received preview"));
            assert!(matches!(d.work.phase(), Phase::Stopped | Phase::Disconnected | Phase::Failed));
            let phase = d.work.phase(); d.advance(); assert_eq!(d.work.phase(), phase);
        }
    }
}
