//! Draw Tailorbird's logical 960 by 540 layout on the current guest scanout.
#[path = "../../../../PCP_OS_Rust/crates/pcpos-kernel/src/font.rs"]
mod font;
const DOUBLE: &[u8; 95 * 20 * 40] =
    include_bytes!("../../../../PCP_OS_Rust/crates/pcpos-kernel/src/font-2x.bin");

pub struct Frame<'a> {
    pixels: &'a mut [u32],
    pitch: usize,
    width: usize,
    height: usize,
    left: usize,
    top: usize,
    scale: usize,
    divisor: usize,
}

impl<'a> Frame<'a> {
    pub fn new(
        pixels: &'a mut [u32],
        width: usize,
        height: usize,
        pitch: usize,
        ground: u32,
        compact: bool,
    ) -> Self {
        assert!(width > 0 && height > 0 && width <= pitch && height <= pixels.len() / pitch);
        let whole = (width / 960).min(height / 540);
        let (scale, divisor) = if whole > 0 {
            (if compact { 1 } else { whole }, 1)
        } else if width * 540 < height * 960 {
            (width, 960)
        } else {
            (height, 540)
        };
        let left = (width - 960 * scale / divisor) / 2;
        let top = (height - 540 * scale / divisor) / 2;
        for row in pixels.chunks_exact_mut(pitch).take(height) {
            row[..width].fill(ground);
        }
        Self {
            pixels,
            pitch,
            width,
            height,
            left,
            top,
            scale,
            divisor,
        }
    }

    fn x(&self, x: usize) -> usize {
        (self.left + x * self.scale / self.divisor).min(self.width)
    }
    fn y(&self, y: usize) -> usize {
        (self.top + y * self.scale / self.divisor).min(self.height)
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        if w <= 0 || h <= 0 {
            return;
        }
        let left = self.x(x.max(0) as usize);
        let right = self.x((x + w).max(0) as usize);
        for row in self.y(y.max(0) as usize)..self.y((y + h).max(0) as usize) {
            let offset = row * self.pitch;
            self.pixels[offset + left..offset + right].fill(color);
        }
    }

    /// Sample coverage from the typeface at the destination size. The larger
    /// face is independently rasterized from the original DejaVu outlines.
    pub fn glyph(&mut self, x: usize, y: usize, w: usize, h: usize, byte: u8, color: u32) {
        let Some(index) = byte.checked_sub(font::FIRST).filter(|&i| i < 95) else {
            return;
        };
        let left = self.x(x);
        let top = self.y(y);
        let dest_w = self.x(x + w) - left;
        let dest_h = self.y(y + h) - top;
        if dest_w == 0 || dest_h == 0 {
            return;
        }
        let (mut sw, mut sh) = (font::CELL_WIDTH, font::CELL_HEIGHT);
        let mask = if dest_w > sw || dest_h > sh {
            sw *= 2;
            sh *= 2;
            &DOUBLE[usize::from(index) * sw * sh..(usize::from(index) + 1) * sw * sh]
        } else {
            &font::GLYPHS[usize::from(index)][..]
        };
        for dy in 0..dest_h {
            let sy = dy * sh / dest_h;
            let end_y = ((dy + 1) * sh / dest_h).max(sy + 1);
            for dx in 0..dest_w {
                let sx = dx * sw / dest_w;
                let end_x = ((dx + 1) * sw / dest_w).max(sx + 1);
                let mut sum = 0u32;
                for row in sy..end_y {
                    sum += mask[row * sw + sx..row * sw + end_x]
                        .iter()
                        .map(|&a| u32::from(a))
                        .sum::<u32>();
                }
                let alpha = sum / ((end_x - sx) * (end_y - sy)) as u32;
                if alpha == 0 {
                    continue;
                }
                let pixel = &mut self.pixels[(top + dy) * self.pitch + left + dx];
                let blend = |s: u32| {
                    (((*pixel >> s & 255) * (255 - alpha) + (color >> s & 255) * alpha + 127) / 255)
                        << s
                };
                *pixel = blend(16) | blend(8) | blend(0);
            }
        }
    }

    /// The event message is printable ASCII already retained by the reducer.
    /// Starting at 422 leaves room in the panel for all 84 six-unit cells.
    /// For OUTPUT/TOOL rows the UI-owned "NNB" at x386..404 reports the
    /// *retained preview* byte length, so a trailing space remains observable.
    /// The separate (!) glyph at x=404 is *derived* from reducer metadata,
    /// not from service-supplied content (which starts only at x=422).
    /// Neither the count nor loss flag is a trusted service/effect receipt.
    pub fn task_output(&mut self, y: i32, kind: &str, message: &str,
                       preview_lossy: bool, kind_color: u32, text_color: u32) {
        self.text(336, y, kind, 1, kind_color);
        if kind == "OUTPUT" || kind == "TOOL" {
            // Ui::log retains <= 84 printable ASCII bytes. Do not silently
            // display a false numeric length if a different caller violates
            // the contract. Metadata is outside the model-controlled text.
            let n = message.len();
            let (tens, units) = if n <= 99 {
                (b'0' + (n / 10) as u8, b'0' + (n % 10) as u8)
            } else {
                (b'?', b'?')
            };
            self.glyph(386, y.max(0) as usize, 6, 10, tens, kind_color);
            self.glyph(392, y.max(0) as usize, 6, 10, units, kind_color);
            self.glyph(398, y.max(0) as usize, 6, 10, b'B', kind_color);
        }
        if preview_lossy {
            self.glyph(404, y.max(0) as usize, 6, 10, b'!', kind_color);
        }
        self.text(422, y, message, 1, text_color);
    }

    pub fn text(&mut self, x: i32, y: i32, text: &str, size: i32, color: u32) {
        if x < 0 || y < 0 || size <= 0 {
            return;
        }
        let size = size as usize;
        for (i, byte) in text.bytes().enumerate() {
            self.glyph(
                x as usize + i * 6 * size,
                y as usize,
                6 * size,
                10 * size,
                byte,
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::{format, vec};
    #[test]
    fn compact_view_fits_the_screen_and_preserves_visible_bounds() {
        for (w, h, geometry) in [
            (800, 600, (0, 75, 800, 525)),
            (1024, 768, (32, 114, 992, 654)),
            (1920, 1080, (480, 270, 1440, 810)),
            (2560, 1440, (800, 450, 1760, 990)),
        ] {
            let mut pixels = vec![0; w * h];
            let f = Frame::new(&mut pixels, w, h, w, 0, true);
            assert_eq!((f.x(0), f.y(0), f.x(960), f.y(540)), geometry);
        }
    }

    #[test]
    fn clips_at_visible_edges_and_leaves_row_padding_untouched() {
        for (w, h, left, top) in [
            (800, 600, 0, 75),
            (1024, 768, 32, 114),
            (1920, 1080, 0, 0),
            (2560, 1440, 320, 180),
        ] {
            let pitch = w + 9;
            let mut pixels = vec![0xdeadbeef; pitch * h + 2];
            {
                let mut f = Frame::new(&mut pixels[1..1 + pitch * h], w, h, pitch, 0x10283f, false);
                assert_eq!((f.left, f.top), (left, top));
                f.rect(-10, -10, 990, 565, 0xabcdef);
                f.text(22, 16, "TAILORBIRD", 3, 0xff01);
                f.glyph(140, 460, 10, 20, b'a', 0xffffff);
                f.glyph(960, 540, 10, 20, b'a', 0xffffff);
            }
            assert_eq!(pixels[0], 0xdeadbeef);
            assert_eq!(pixels[pitch * h + 1], 0xdeadbeef);
            assert!(pixels[1..=pitch * h]
                .chunks_exact(pitch)
                .all(|r| r[w..].iter().all(|v| *v == 0xdeadbeef)));
        }
    }
    #[test]
    fn task_output_preserves_case_and_the_final_cell_inside_the_panel() {
        // Model two service messages that differ only in the last path character.
        let prefix = " ".repeat(82);
        let lower = prefix.clone() + "_a";
        let upper = prefix + "_A";
        let render = |message: &str| {
            let mut pixels = vec![0; 960 * 540];
            let mut f = Frame::new(&mut pixels, 960, 540, 960, 0, false);
            f.task_output(190, "OUTPUT", message, false, 0xff00, 0xffffff);
            pixels
        };
        let a = render(&lower);
        let b = render(&upper);
        let mut reference = vec![0; 960 * 540];
        let mut f = Frame::new(&mut reference, 960, 540, 960, 0, false);
        f.glyph(914, 190, 6, 10, b'_', 0xffffff);
        f.glyph(920, 190, 6, 10, b'a', 0xffffff);
        let mut case_difference = false;
        for y in 190..200 {
            let offset = y * 960;
            assert_eq!(&a[offset+914..offset+926], &reference[offset+914..offset+926]);
            assert!(a[offset+926..offset+960].iter().all(|&p| p == 0));
            case_difference |= a[offset+920..offset+926] != b[offset+920..offset+926];
        }
        assert!(case_difference);
    }

    #[test]
    fn output_and_tool_trailing_space_length_is_visible_at_native_scales() {
        // The retained preview differs, but an ASCII space has no ink.
        // The byte count must distinguish both cases without modifying
        // model-controlled message pixels or the independent lossy flag.
        let prefix = "Z".repeat(83);
        let trailing = format!("{prefix} ");
        let period = format!("{prefix}.");
        for kind in ["OUTPUT", "TOOL"] {
            for (width, height) in [(960usize, 540usize), (800, 600), (1024, 768)] {
                let render = |message: &str| {
                    let mut pixels = vec![0x18364f; width * height];
                    let mut f = Frame::new(
                        &mut pixels, width, height, width, 0x18364f, false,
                    );
                    f.task_output(190, kind, message, false, 0x00ff00, 0xffffff);
                    pixels
                };
                let a = render(&prefix);
                let b = render(&trailing);
                let c = render(&period);
                let mut scratch = vec![0u32; width * height];
                let f = Frame::new(
                    &mut scratch, width, height, width, 0x18364f, false,
                );
                let (left, right) = (f.x(386), f.x(404));
                let (body_left, body_right) = (f.x(422), f.x(926));
                let (top, bottom) = (f.y(190), f.y(200));
                let mut count_difference = 0usize;
                let mut printable_control_difference = 0usize;
                for y in 0..height {
                    for x in 0..width {
                        let i = y * width + x;
                        if a[i] != b[i] {
                            assert!(x >= left && x < right && y >= top && y < bottom,
                                "retained count must not alter message/body or other UI");
                            count_difference += 1;
                        }
                        if b[i] != c[i] {
                            assert!(x >= body_left && x < body_right
                                && y >= top && y < bottom,
                                "equal-length content control changed metadata");
                            printable_control_difference += 1;
                        }
                    }
                }
                assert!(count_difference > 0,
                    "83 vs 84 bytes invisible at {width}x{height}, {kind}");
                assert!(printable_control_difference > 0,
                    "space vs period glyph invisible at {width}x{height}, {kind}");
            }
        }
    }

    #[test]
    fn loss_indicator_is_separate_from_model_or_tool_text() {
        // Even if a service returns the literal "[LOSSY] go??" text,
        // only the UI-computed loss bit can cause the dedicated status cell
        // to be lit. No model bytes can write into x=404 through this API.
        let render = |lossy: bool| {
            let mut pixels = vec![0; 960 * 540];
            let mut f = Frame::new(&mut pixels, 960, 540, 960, 0, false);
            f.task_output(190, "OUTPUT", "[LOSSY] go??", lossy, 0xffffff, 0xffffff);
            pixels
        };
        let exact = render(false);
        let altered = render(true);
        assert_ne!(exact, altered);
        let mut ink = false;
        for y in 190..200 {
            for x in 404..410 {
                assert_eq!(exact[y * 960 + x], 0, "source text escaped its allocated lane");
                ink |= altered[y * 960 + x] != 0;
            }
        }
        assert!(ink, "UI provenance indicator did not paint");
        // The service text area must be byte-for-byte visually identical.
        for y in 190..200 {
            assert_eq!(&exact[y * 960 + 422..y * 960 + 926],
                       &altered[y * 960 + 422..y * 960 + 926]);
        }
    }

    #[test]
    fn exact_source_glyphs_are_rasterized_at_the_selected_scale() {
        for k in [1, 2] {
            let w = 960 * k;
            let h = 540 * k;
            let mut pixels = vec![0; w * h];
            let mut f = Frame::new(&mut pixels, w, h, w, 0, false);
            f.glyph(140, 460, 10, 20, b'a', 0xffffff);
            let glyph = usize::from(b'a' - font::FIRST);
            let len = 200 * k * k;
            let expected = if k == 1 {
                &font::GLYPHS[glyph][..]
            } else {
                &DOUBLE[glyph * len..(glyph + 1) * len]
            };
            for (y, row) in expected.chunks_exact(10 * k).enumerate() {
                for (x, &coverage) in row.iter().enumerate() {
                    assert_eq!(
                        pixels[(460 * k + y) * w + 140 * k + x],
                        u32::from(coverage) * 0x010101
                    );
                }
            }
        }
    }
    #[test]
    fn stop_control_ink_is_readable_on_the_real_framebuffer_at_both_scales() {
        use crate::palette::{stop_colors, PANEL};

        // WCAG relative luminance and contrast ratio, computed from the
        // exact sRGB colors passed to the pixel renderer.
        fn luminance(color: u32) -> f64 {
            let channel = |shift| {
                let v = ((color >> shift) & 255u32) as f64 / 255.0;
                if v <= 0.04045 { v / 12.92 }
                else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
        }
        fn contrast(a: u32, b: u32) -> f64 {
            let (high, low) = (luminance(a).max(luminance(b)),
                               luminance(a).min(luminance(b)));
            (high + 0.05) / (low + 0.05)
        }

        for enabled in [false, true] {
            let (fill, heading, caption) = stop_colors(enabled);
            assert!(contrast(heading, fill) >= 4.5,
                    "Stop title must meet normal-text contrast when enabled={enabled}");
            assert!(contrast(caption, fill) >= 4.5,
                    "Stop task state must meet normal-text contrast when enabled={enabled}");
            if enabled {
                assert!(contrast(fill, PANEL) >= 3.0,
                        "enabled Stop fill must be distinguishable from its panel");
            }

            for k in [1usize, 2] {
                let (width, height) = (960 * k, 540 * k);
                let pitch = width + 11;
                let mut guarded = vec![0xdeadbeef; pitch * height + 2];
                {
                    let pixels = &mut guarded[1..1 + pitch * height];
                    let mut f = Frame::new(pixels, width, height, pitch, PANEL, false);
                    f.rect(20, 94, 280, 352, PANEL);
                    let rect = crate::pointer::STOP_RECT;
                    f.rect(rect.x, rect.y, rect.width, rect.height, fill);
                    f.text(48, 331, "STOP", 3, heading);
                    f.text(48, 361, if enabled { "RUNNING" } else { "NO TASK" },
                           1, caption);
                }
                let px = |x: usize, y: usize| guarded[1 + y * pitch + x];
                assert_eq!(px(35 * k, 319 * k), fill);
                for (x, y, w, h) in [(48, 331, 72, 30), (48, 361, 70, 10)] {
                    let mut ink = 0usize;
                    for row in y * k..(y + h) * k {
                        for col in x * k..(x + w) * k {
                            ink += usize::from(px(col, row) != fill);
                        }
                    }
                    assert!(ink > 0, "Stop title/state raster missing at scale {k}");
                }
                assert_eq!(guarded[0], 0xdeadbeef);
                assert_eq!(guarded[pitch * height + 1], 0xdeadbeef);
                assert!(guarded[1..1 + pitch * height]
                    .chunks_exact(pitch)
                    .all(|row| row[width..].iter().all(|&v| v == 0xdeadbeef)));
            }
        }
    }

}
