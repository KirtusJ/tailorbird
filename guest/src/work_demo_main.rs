#![no_std]
#![no_main]
use pcp_game_guest::{call, exit, log, open, sleep_us, Display};
use tailorbird_ui::{frame::Frame, live_demo::Demo, live_view, palette::GROUND};

pcp_game_guest::entry!(work_demo_main);
#[no_mangle]
pub extern "C" fn work_demo_main() -> ! {
    let mut display = Display::current();
    let keyboard = open(b"/dev/kbd\0", 0x800);
    if keyboard < 0 { exit(2); }
    let mut demo = Demo::new(); let mut down = [false; 128];
    let mut dirty = true;
    loop {
        if dirty {
            display.draw(|pixels, width, height, pitch| {
                live_view::paint(&mut Frame::new(pixels, width, height, pitch, GROUND, false), &demo.work);
            });
            log(b"Live work demo frame: "); log(demo.work.phase().label().as_bytes()); log(b"\n");
            dirty = false;
        }
        let mut event = [0u8; 2];
        while call(0, keyboard as usize, event.as_mut_ptr() as usize, 2, 0, 0, 0) == 2 {
            let code = (event[0] & 127) as usize; let pressed = event[1] & 1 == 0;
            let fresh = pressed && !down[code]; down[code] = pressed;
            if !fresh || event[1] & 2 != 0 { continue; }
            if code == 1 { exit(0); }
            dirty |= demo.key(code);
        }
        sleep_us(12000);
    }
}
