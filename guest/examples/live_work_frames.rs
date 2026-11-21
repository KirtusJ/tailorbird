//! Render the actual Rust UI at several sizes for review and native comparison.
use std::{env, fs, io::Write, path::Path};
use tailorbird_ui::{frame::Frame, live_demo::Demo, live_view, palette::GROUND};

const CASES: &[(&str, &[usize])] = &[
    ("idle", &[]), ("started", &[0x39]), ("draft", &[0x39,0x39]),
    ("preview", &[0x39,0x39,0x39]), ("built", &[0x39,0x39,0x39,0x39]),
    ("waiting", &[0x39,0x39,0x39,0x39,0x11]),
    ("stop_requested", &[0x39,0x39,0x39,0x39,0x1f]),
    ("stopped", &[0x39,0x39,0x39,0x39,0x1f,0x2e]),
    ("disconnected", &[0x39,0x39,0x39,0x39,0x20]),
    ("late_after_disconnect", &[0x39,0x39,0x39,0x39,0x20,0x39]),
    ("failed", &[0x39,0x39,0x39,0x39,0x21]),
    ("completed", &[0x39,0x39,0x39,0x39,0x39,0x39,0x39]),
];
fn main() {
    let root = env::args().nth(1).expect("output directory");
    for (width,height) in [(640,480),(800,600),(1024,768),(1920,1080)] {
        let folder = Path::new(&root).join(format!("{width}x{height}")); fs::create_dir_all(&folder).unwrap();
        for (name, keys) in CASES {
            let mut demo = Demo::new(); for key in *keys { demo.key(*key); }
            let mut pixels = vec![GROUND; width*height];
            live_view::paint(&mut Frame::new(&mut pixels,width,height,width,GROUND,false), &demo.work);
            let mut f = fs::File::create(folder.join(format!("{name}.ppm"))).unwrap();
            write!(f,"P6\n{width} {height}\n255\n").unwrap();
            let rgb: Vec<u8> = pixels.iter().flat_map(|p| [(p>>16) as u8,(p>>8) as u8,*p as u8]).collect();
            f.write_all(&rgb).unwrap();
        }
    }
    let rows: Vec<String> = CASES.iter().map(|(name, keys)| format!("{{\"name\":\"{name}\",\"keys\":{keys:?}}}")).collect();
    fs::write(Path::new(&root).join("cases.json"), format!("[{}]\n",rows.join(","))).unwrap();
}
