//! Caller-owned rendering for the live-work surface. No input or execution.
use crate::{frame::Frame, live_work::{ChangeState, Phase, StepState, Work, PREVIEW_WIDTH, PREVIEW_HEIGHT}, palette::*};

fn text(f: &mut Frame<'_>, x: usize, y: usize, value: &str, color: u32, large: bool) {
    let (w, h) = if large { (10, 20) } else { (8, 16) };
    for (i, b) in value.bytes().enumerate() { f.glyph(x + i*w, y, w, h, b, color); }
}
fn number(f: &mut Frame<'_>, x: usize, y: usize, mut n: u64, color: u32) -> usize {
    let mut bytes = [b'0'; 20]; let mut at = bytes.len();
    loop { at -= 1; bytes[at] = b'0' + (n % 10) as u8; n /= 10; if n == 0 { break; } }
    let value = core::str::from_utf8(&bytes[at..]).unwrap();
    text(f, x, y, value, color, false); value.len()*8
}
fn phase_color(phase: Phase) -> u32 {
    match phase { Phase::Completed | Phase::Stopped => GOOD, Phase::Failed => BAD,
        Phase::Disconnected | Phase::StopRequested => AMBER, _ => TEAL }
}

/// Logical 960x540 canvas, rendered through the existing scale-aware Frame.
/// The immutable demo flag stays visible in every state, including completion.
pub fn paint(f: &mut Frame<'_>, work: &Work) {
    f.rect(0, 0, 960, 540, GROUND);
    f.rect(0, 0, 960, 30, if work.is_demo() { AMBER } else { PANEL });
    text(f, 20, 7, if work.is_demo() { "PCP OS > TAILORBIRD > UI DEMO / sample events / no files written" }
         else { "PCP OS > TAILORBIRD > Work activity" }, if work.is_demo() { GROUND } else { TEXT }, false);
    text(f, 24, 47, work.title(), TEXT, true);
    let color = phase_color(work.phase());
    f.rect(24, 82, 6, 16, color);
    text(f, 40, 81, work.phase().label(), color, false);
    if let Some(i) = work.current().filter(|_| work.phase() == Phase::Running) {
        let step = &work.steps()[i];
        if step.reported {
            let n = number(f, 300, 81, step.done, SOFT);
            if let Some(total) = step.total {
                text(f, 300+n, 81, " / ", SOFT, false);
                number(f, 324+n, 81, total, TEXT);
            } else { text(f, 308+n, 81, "units / total unknown", SOFT, false); }
        } else { text(f, 300, 81, "Progress not yet reported", SOFT, false); }
    }
    text(f, 700, 81, "Steps", SOFT, false);
    let done = work.steps().iter().filter(|s| s.state == StepState::Done).count() as u64;
    let n = number(f, 755, 81, done, TEXT);
    text(f, 755+n, 81, " / ", SOFT, false);
    number(f, 779+n, 81, work.steps().len() as u64, TEXT);

    f.rect(24, 116, 248, 358, PANEL);
    text(f, 40, 129, "WORK STEPS", SOFT, false);
    for (i, step) in work.steps().iter().enumerate() {
        let y = 158 + i*50;
        let interrupted = matches!(work.phase(), Phase::Disconnected | Phase::StopRequested)
            && matches!(step.state, StepState::Active | StepState::Waiting);
        let ink = if interrupted { AMBER } else { match step.state { StepState::Done => GOOD, StepState::Active => TEAL,
            StepState::Waiting => AMBER, StepState::Failed => BAD, StepState::Stopped => AMBER,
            StepState::Queued => SOFT }};
        f.rect(40, y as i32 + 3, 12, 12, ink);
        if step.state == StepState::Done { text(f, 40, y, "+", GROUND, false); }
        text(f, 62, y, step.label.as_str(), if step.state == StepState::Queued { SOFT } else { TEXT }, false);
        let status = if interrupted { if work.phase() == Phase::Disconnected { "Unknown" } else { "Stopping" } }
                     else { step.state.label() };
        text(f, 62, y+20, status, ink, false);
        if step.state == StepState::Active && work.phase() == Phase::Running {
            if let Some(percent) = step.percent() {
                f.rect(160, y as i32+25, 88, 5, GROUND);
                f.rect(160, y as i32+25, (88*percent as i32)/100, 5, TEAL);
            } else { text(f, 160, y+20, "No total", SOFT, false); }
        }
    }

    f.rect(288, 116, 648, 358, PANEL);
    text(f, 308, 129, if work.is_demo() { "GREETING CARD / SAMPLE PREVIEW" } else { "ARTIFACT PREVIEW" }, SOFT, false);
    if let Some(preview) = work.preview() {
        let state = match preview.change { ChangeState::Proposed => "Proposed", ChangeState::Applied => "Applied" };
        text(f, 720, 129, state, if preview.change == ChangeState::Proposed { AMBER } else { GOOD }, false);
        let status = match work.phase() {
            Phase::Disconnected => "Last received / connection lost",
            Phase::StopRequested => "Partial preview / stopping",
            Phase::Failed | Phase::Stopped => "Partial preview retained",
            Phase::Completed => "Result preview",
            _ => "Preview so far",
        };
        text(f, 308, 150, status, color, false);
        // Bounded overview bitmap; caller retains original full artifact.
        for y in 0..PREVIEW_HEIGHT { for x in 0..PREVIEW_WIDTH {
            f.rect((388+x*6) as i32, (181+y*6) as i32, 6, 6, preview.pixels[y*PREVIEW_WIDTH+x]);
        }}
        text(f, 308, 433, preview.location.as_str(), TEXT, false);
        text(f, 308, 454, "Revision", SOFT, false);
        number(f, 388, 454, preview.revision, TEXT);
    } else {
        f.rect(553, 230, 60, 44, GROUND);
        f.rect(561, 238, 44, 6, SOFT);
        f.rect(561, 252, 27, 5, TEAL);
        text(f, 432, 298, "Preview will appear here", TEXT, false);
        text(f, 400, 324, "Updates arrive as work progresses", SOFT, false);
    }
    text(f, 24, 487, work.detail(), color, false);
    f.rect(0, 514, 960, 26, PANEL);
    text(f, 20, 519, if work.is_demo() { "Space next  W wait  S request stop  C confirm stop  D disconnect  F fail  R replay  Esc exit" }
         else { "Preview and progress reflect received events" }, SOFT, false);
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec;
    #[test]
    fn every_state_preserves_padding_and_keeps_demo_label_and_preview_visible() {
        for (width, height) in [(640,480), (800,600), (1024,768), (1920,1080)] {
            let pitch = width + 11; let guard = 0xdeadbeef;
            let mut pixels = vec![guard; pitch*height+19];
            let mut demo = crate::live_demo::Demo::new();
            for _ in 0..4 { demo.advance(); }
            paint(&mut Frame::new(&mut pixels, width, height, pitch, GROUND, false), &demo.work);
            let first = pixels.clone();
            demo.disconnect();
            paint(&mut Frame::new(&mut pixels, width, height, pitch, GROUND, false), &demo.work);
            assert_ne!(pixels, first);
            for y in 0..height { assert!(pixels[y*pitch+width..(y+1)*pitch].iter().all(|p| *p == guard)); }
            assert!(pixels[pitch*height..].iter().all(|p| *p == guard));
            assert!(pixels.iter().any(|p| *p == AMBER));
            assert!(demo.work.preview().is_some());
        }
    }
}
