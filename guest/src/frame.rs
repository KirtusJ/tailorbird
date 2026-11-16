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
    ) -> Self {
        assert!(width > 0 && height > 0 && width <= pitch && height <= pixels.len() / pitch);
        let whole = (width / 960).min(height / 540);
        let (scale, divisor) = if whole > 0 {
            (whole, 1)
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
    use std::vec;
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
                let mut f = Frame::new(&mut pixels[1..1 + pitch * h], w, h, pitch, 0x10283f);
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
    fn exact_source_glyphs_are_rasterized_at_the_selected_scale() {
        for k in [1, 2] {
            let w = 960 * k;
            let h = 540 * k;
            let mut pixels = vec![0; w * h];
            let mut f = Frame::new(&mut pixels, w, h, w, 0);
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
}
