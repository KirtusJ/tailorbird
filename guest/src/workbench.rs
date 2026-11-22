//! A small lit object on Taylor's workbench, drawn by Heliograph (PCP OS's own 3D renderer) into a
//! picture this program owns. A trial of the renderer inside Tailorbird: it shows that something was
//! made and says nothing else about it; the words beside it carry every fact.
use heliograph::assets::{LODS, MATERIALS};
use heliograph::raster::{Cached, Counts, Instance, Target};

pub const WIDTH: usize = 140;
pub const HEIGHT: usize = 110;
const VERTICES: usize = 128 * 24;

pub struct Workbench {
    pub pixels: [u32; WIDTH * HEIGHT],
    depth: [i32; WIDTH * HEIGHT],
    cache: [Cached; VERTICES],
}

impl Workbench {
    pub const fn new() -> Self {
        Self { pixels: [0; WIDTH * HEIGHT], depth: [0; WIDTH * HEIGHT], cache: [Cached::ZERO; VERTICES] }
    }

    /// Draw the object turned by `angle` (1024 to a full turn) in `material` (0, 1 or 2) on `ground`.
    /// Returns how many triangles reached the picture.
    pub fn draw(&mut self, angle: i32, material: usize, ground: u32) -> u32 {
        self.pixels.fill(ground);
        self.depth.fill(0);
        let mut counts = Counts::default();
        let mut target = Target::new(&mut self.pixels, &mut self.depth, WIDTH as i32, HEIGHT as i32, 130);
        target.draw(&LODS[1], Instance { pos: [0, 0, 5600], yaw: angle, tilt: 76, scale: 1024 }, &MATERIALS[material % 3],
                    &mut self.cache, &mut counts);
        counts.rasterized
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn the_object_is_drawn_inside_its_picture_and_the_same_each_time() {
        let mut bench = std::boxed::Box::new(Workbench::new());
        let drawn = bench.draw(100, 1, 0x18364F);
        assert!(drawn > 0);
        let lit = bench.pixels.iter().filter(|p| **p != 0x18364F).count();
        // Something is there, and it does not fill the picture or touch its edge rows.
        assert!(lit > WIDTH * HEIGHT / 20 && lit < WIDTH * HEIGHT * 3 / 4, "{lit} pixels lit");
        assert!(bench.pixels[..WIDTH].iter().all(|p| *p == 0x18364F));
        assert!(bench.pixels[WIDTH * (HEIGHT - 1)..].iter().all(|p| *p == 0x18364F));
        let first = bench.pixels;
        bench.draw(100, 1, 0x18364F);
        assert!(first == bench.pixels);
        bench.draw(400, 1, 0x18364F);
        assert!(first != bench.pixels);
        if let Ok(folder) = std::env::var("TAILORBIRD_SHOT") {
            let mut out: std::vec::Vec<u8> = std::format!("P6\n{} {}\n255\n", WIDTH, HEIGHT).into_bytes();
            for p in &first {
                out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
            }
            std::fs::write(std::format!("{}/workbench.ppm", folder), out).unwrap();
        }
    }
}
