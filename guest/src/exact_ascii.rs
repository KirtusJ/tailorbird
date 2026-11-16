//! Exact, case-sensitive native request-editor glyph rendering.
//!
//! The inherited five-by-seven Frame::text cannot show exact user input:
//! it uppercases lowercase characters and omits unsupported punctuation.
//! Use the kernel's existing, source-pinned DejaVu Sans Mono printable-ASCII
//! coverage instead. The font source is generated and keeps its own notice.
//! This renderer does not interpret text, dispatch actions, or authorize edits.
//!
//! The 960x540 Tailorbird guest reserves a 780-pixel-wide two-line editor.
//! At ten by twenty pixels per ASCII cell, it exposes the complete 120-byte
//! request (78 cells on the first row and 42 on the second).
#[path = "../../../../PCP_OS_Rust/crates/pcpos-kernel/src/font.rs"]
mod kernel_font;

pub const CELL_WIDTH: usize = kernel_font::CELL_WIDTH;
pub const CELL_HEIGHT: usize = kernel_font::CELL_HEIGHT;
pub const EDITOR_COLS: usize = 78;
pub const EDITOR_ROWS: usize = 2;
pub const EDITOR_CAPACITY: usize = EDITOR_COLS * EDITOR_ROWS;
pub const EDITOR_X: usize = 140;
pub const EDITOR_Y: usize = 460;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DrawError {
    UnrenderableInput,
    InsufficientSurface,
    InsufficientEditor,
}

pub fn exact_printable_ascii(s: &str) -> bool {
    s.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

pub fn can_show_editor_input(s: &str) -> bool {
    s.len() <= crate::INPUT_LIMIT && s.len() <= EDITOR_CAPACITY && exact_printable_ascii(s)
}

fn blend(under: u32, over: u32, alpha: u32) -> u32 {
    let alpha = alpha.min(255);
    let rest = 255 - alpha;
    let channel = |shift: u32| {
        let u = (under >> shift) & 0xff;
        let o = (over >> shift) & 0xff;
        (u * rest + o * alpha + 127) / 255
    };
    channel(16) << 16 | channel(8) << 8 | channel(0)
}

/// Rasterize an entire ASCII request into an existing framebuffer.
///
/// Preflight rejects a too-small buffer, insufficient editor footprint, or
/// unrenderable text BEFORE the first pixel is written. Pitch counts
/// u32 pixels, not bytes. No bytes are silently truncated, folded, or
/// substituted, and every ASCII character gets its own glyph cell.
/// This only checks the in-process pixel buffer, not a hardware mapping.
pub fn draw_request(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    pitch: usize,
    input: &str,
    colour: u32,
) -> Result<(), DrawError> {
    if !can_show_editor_input(input) {
        return Err(DrawError::UnrenderableInput);
    }
    let required = height.checked_mul(pitch).ok_or(DrawError::InsufficientSurface)?;
    if pitch < width || pixels.len() < required {
        return Err(DrawError::InsufficientSurface);
    }
    let right = EDITOR_X
        .checked_add(EDITOR_COLS * CELL_WIDTH)
        .ok_or(DrawError::InsufficientEditor)?;
    let bottom = EDITOR_Y
        .checked_add(EDITOR_ROWS * CELL_HEIGHT)
        .ok_or(DrawError::InsufficientEditor)?;
    if width < right || height < bottom {
        return Err(DrawError::InsufficientEditor);
    }

    for (i, byte) in input.bytes().enumerate() {
        let glyph = &kernel_font::GLYPHS[(byte - kernel_font::FIRST) as usize];
        let gx = EDITOR_X + (i % EDITOR_COLS) * CELL_WIDTH;
        let gy = EDITOR_Y + (i / EDITOR_COLS) * CELL_HEIGHT;
        for row in 0..CELL_HEIGHT {
            for col in 0..CELL_WIDTH {
                let coverage = glyph[row * CELL_WIDTH + col] as u32;
                if coverage != 0 {
                    let idx = (gy + row) * pitch + gx + col;
                    pixels[idx] = blend(pixels[idx], colour, coverage);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, BackendEvent, Intent, Mode, Model, Ui};
    use std::string::String;
    use std::vec;

    const W: usize = 960;
    const H: usize = 540;
    const BACK: u32 = 0x10283f;
    const FORE: u32 = 0xf2f7fc;

    fn raster(input: &str) -> vec::Vec<u32> {
        let mut pixels = vec![BACK; W * H];
        draw_request(&mut pixels, W, H, W, input, FORE).unwrap();
        pixels
    }

    fn cell_has_ink(pixels: &[u32], i: usize) -> bool {
        let x = EDITOR_X + (i % EDITOR_COLS) * CELL_WIDTH;
        let y = EDITOR_Y + (i / EDITOR_COLS) * CELL_HEIGHT;
        (0..CELL_HEIGHT).any(|dy| {
            (0..CELL_WIDTH).any(|dx| pixels[(y + dy) * W + x + dx] != BACK)
        })
    }

    #[test]
    fn lower_and_upper_case_paths_render_differently() {
        let a = raster("read /home/a.txt");
        let b = raster("read /home/A.txt");
        assert_ne!(a, b);
        assert!(cell_has_ink(&a, 11));
        assert!(cell_has_ink(&b, 11));
    }

    #[test]
    fn all_printable_ascii_glyphs_except_space_have_ink() {
        for ch in 33u8..=126 {
            let glyph = &kernel_font::GLYPHS[(ch - kernel_font::FIRST) as usize];
            assert!(glyph.iter().any(|v| *v != 0), "invisible supported ASCII {}", ch);
        }
    }

    #[test]
    fn all_submittable_bytes_at_92_93_and_120_are_drawn() {
        for len in [92, 93, 120] {
            let prompt = "x".repeat(len - 1) + "Z";
            let pixels = raster(&prompt);
            assert!(cell_has_ink(&pixels, len - 1), "last byte invisible at len {}", len);
            assert_eq!(EDITOR_COLS, 78);
            assert!(len <= EDITOR_CAPACITY);
            let mut u = Ui::new();
            assert!(u.observe(BackendEvent::Service {
                connected: true, model: Model::Ready, saved_mode: Mode::Single,
            }));
            for ch in prompt.bytes() {
                assert!(u.input.push_ascii(ch));
            }
            assert_eq!(u.input.as_str(), prompt);
            match u.intent(Action::Submit) {
                Some(Intent::Submit { prompt: delivered, mode: Mode::Single }) => {
                    assert_eq!(delivered.as_str(), prompt);
                }
                _ => panic!("expected exact UI request intent"),
            }
        }
    }

    #[test]
    fn punctuation_is_distinct_and_not_silently_deleted() {
        let a = raster("mkdir /home/a_b:{c}[d]?!");
        let b = raster("mkdir /home/a-b:{c}[d]?!");
        assert_ne!(a, b);
        let cases = ["_", "\\", ":", "{", "}", "[", "]", "?", "!", "~", "@", "#", "'",
                     "\""];
        for s in cases {
            assert!(cell_has_ink(&raster(s), 0), "missing punctuation {}", s);
        }
    }

    #[test]
    fn invalid_or_unfittable_requests_leave_framebuffer_unchanged() {
        for value in [
            "a".repeat(EDITOR_CAPACITY + 1),
            String::from("abc\nhidden"),
            String::from("á"),
        ] {
            let mut px = vec![BACK; W * H];
            assert_eq!(draw_request(&mut px, W, H, W, &value, FORE),
                       Err(DrawError::UnrenderableInput));
            assert!(px.iter().all(|c| *c == BACK));
        }

        let mut narrow = vec![BACK; 919 * H];
        assert_eq!(draw_request(&mut narrow, 919, H, 919, "A", FORE),
                   Err(DrawError::InsufficientEditor));
        assert!(narrow.iter().all(|c| *c == BACK));

        let mut small = vec![BACK; 20];
        assert_eq!(draw_request(&mut small, W, H, W, "A", FORE),
                   Err(DrawError::InsufficientSurface));
        assert!(small.iter().all(|c| *c == BACK));
    }
}
