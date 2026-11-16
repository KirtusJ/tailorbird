//! Test the guest framebuffer client with a host-backed mapping and recorded page flips.
#![allow(dead_code)]
use std::cell::RefCell;

#[path = "../../../../store/native/guest/src/device.rs"]
mod device;
fn paint_reference(pixels: &mut [u32], w: usize, h: usize, pitch: usize, color: u32) {
    // Draw edge pixels on every visible row, leaving the interior and padding intact.
    for row in pixels.chunks_exact_mut(pitch).take(h) {
        row[0] = color;
        row[w - 1] = color;
    }
}

const GUARD: u32 = 0x1827a4da;
struct Fixture {
    pixels: Vec<u32>,
    width: usize,
    height: usize,
    pitch: usize,
    pages: usize,
    shown: usize,
    flips: Vec<usize>,
    maps: Vec<usize>,
}
thread_local! { static FB: RefCell<Option<Fixture>> = const { RefCell::new(None) }; }
fn fixture<T>(f: impl FnOnce(&mut Fixture) -> T) -> T {
    FB.with_borrow_mut(|opt| f(opt.as_mut().unwrap()))
}
fn open(path: &[u8], _: usize) -> isize {
    match path {
        b"/dev/fb0\0" => 42,
        b"/dev/kbd\0" => 43,
        _ => panic!("unexpected device"),
    }
}
fn log(_: &[u8]) {}
fn exit(code: usize) -> ! {
    panic!("unexpected exit {code}")
}
fn ioctl(fd: isize, op: usize, arg: usize) -> isize {
    assert_eq!(fd, 42);
    fixture(|f| match op {
        0x5001 => {
            let info = [
                f.width as u32,
                f.height as u32,
                (f.pitch * 4) as u32,
                f.pages as u32,
                f.shown as u32,
                0,
            ];
            unsafe {
                std::ptr::copy_nonoverlapping(info.as_ptr(), arg as *mut u32, 6);
            }
            0
        }
        0x5003 => {
            assert_eq!(f.pages, 2);
            assert_eq!(arg, 1 - f.shown);
            f.shown = arg;
            f.flips.push(arg);
            0
        }
        _ => panic!("unexpected display ioctl {op:x}"),
    })
}
fn call(
    op: usize,
    addr: usize,
    bytes: usize,
    prot: usize,
    flags: usize,
    fd: usize,
    off: usize,
) -> isize {
    assert_eq!((op, addr, prot, flags, fd, off), (9, 0, 3, 1, 42, 0));
    fixture(|f| {
        assert_eq!(bytes, 4 * f.pitch * f.height * f.pages);
        f.maps.push(bytes);
        unsafe { f.pixels.as_mut_ptr().add(8) as isize }
    })
}
fn exercise(w: usize, h: usize, pitch: usize, pages: usize, initial: usize) {
    let page = pitch * h;
    let count = page * pages + 16;
    FB.set(Some(Fixture {
        pixels: vec![GUARD; count],
        width: w,
        height: h,
        pitch,
        pages,
        shown: initial,
        flips: Vec::new(),
        maps: Vec::new(),
    }));
    let mut display = device::Display::current();
    let mut expected = vec![GUARD; count];
    let mut shown = initial;
    let mut expected_flips = Vec::new();
    for index in 0..4 {
        shown = if pages == 2 { 1 - shown } else { 0 };
        if pages == 2 {
            expected_flips.push(shown);
        }
        let start = 8 + shown * page;
        let color = 0xa0b349 + index;
        if index == 1 || index == 3 {
            let packed = vec![color; w * h];
            display.show(&packed);
            for y in 0..h {
                expected[start + y * pitch..start + y * pitch + w].fill(color);
            }
        } else {
            display.draw(|px, width, height, stride| {
                assert_eq!((width, height, stride, px.len()), (w, h, pitch, page));
                if index == 0 {
                    paint_reference(px, w, h, pitch, color);
                } else {
                    px[(h - 1) * pitch + w - 1] = color;
                }
            });
            if index == 0 {
                for y in 0..h {
                    expected[start + y * pitch] = color;
                    expected[start + y * pitch + w - 1] = color;
                }
            } else {
                expected[start + (h - 1) * pitch + w - 1] = color;
            }
        }
        fixture(|f| {
            assert!(f.pixels == expected, "page/guard mismatch on pass {index}");
            assert_eq!(f.flips, expected_flips);
            assert_eq!(f.maps, [page * pages * 4]);
        });
    }
    println!(
        "{w}x{h} pitch={} pages={pages} initial={initial} flips={expected_flips:?} verified",
        pitch * 4
    );
}
#[test]
fn preserves_padding_and_flips_only_the_drawn_page() {
    for (w, h, pitch) in [(800, 600, 808), (1024, 768, 1024), (1920, 1080, 2048)] {
        for (pages, initial) in [(1, 0), (2, 0), (2, 1)] {
            exercise(w, h, pitch, pages, initial);
        }
    }
}
