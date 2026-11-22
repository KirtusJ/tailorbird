//! The actual worker Link with an in-memory stand-in for the kernel's nonblocking descriptor.
use crate::wire::{self, Kind, Record};
use std::{cell::RefCell, vec::Vec};

#[allow(dead_code)]
#[path = "model_worker_io.rs"]
mod native;

#[derive(Default)]
struct Device {
    bytes: Vec<u8>,
    remaining: Option<usize>,
    stalled: bool,
    eof: bool,
    sleeps: usize,
    writes: usize,
}

std::thread_local! { static DEVICE: RefCell<Device> = RefCell::new(Device::default()); }

fn open(path: &[u8], flags: usize) -> isize {
    assert_eq!(path, b"/dev/talk\0");
    assert_eq!(flags, 2 | 0x800);
    7
}

fn call(number: usize, fd: usize, address: usize, length: usize, _: usize, _: usize, _: usize) -> isize {
    DEVICE.with_borrow_mut(|d| match number {
        0 => if d.eof { 0 } else { -11 },
        1 => {
            assert_eq!(fd, 7);
            d.writes += 1;
            let n = d.remaining.unwrap_or(length).min(length);
            if n == 0 { return if d.stalled { -11 } else { -5 }; }
            // The caller's write buffer is borrowed only for this synchronous call.
            d.bytes.extend_from_slice(unsafe { std::slice::from_raw_parts(address as *const u8, n) });
            if let Some(left) = d.remaining.as_mut() { *left -= n; }
            n as isize
        }
        3 => 0,
        35 => { d.sleeps += 1; 0 }
        _ => panic!("unexpected syscall {number}"),
    })
}

fn fresh() -> native::Link {
    DEVICE.with_borrow_mut(|device| *device = Device::default());
    native::Link::open().unwrap()
}

fn frame(kind: Kind, ids: Option<(u64, u64)>, body: &[u8]) -> Vec<u8> {
    let (conversation, reply) = ids.unwrap_or((0, 0));
    wire::pack(Record {kind, history:false, conversation, reply, body}).unwrap().bytes().to_vec()
}

fn assert_terminal(link: &mut native::Link) {
    let before = DEVICE.with_borrow(|d| (d.writes, d.bytes.clone()));
    DEVICE.with_borrow_mut(|d| { d.eof = false; d.remaining = None; });
    link.finish_with_error();
    link.finish_with_error();
    assert!(link.failed());
    assert!(link.poll(None).is_err());
    assert!(link.send(Kind::Ready, None, b"").is_err());
    assert!(!link.report_reading((1,1), wire::reading(1,1,1,2).unwrap()));
    DEVICE.with_borrow(|d| assert_eq!((d.writes, &d.bytes), (before.0, &before.1)));
}

#[test]
fn a_model_refused_before_ready_can_send_one_startup_notice_on_an_intact_stream() {
    let mut link = fresh();
    link.send(Kind::Loading, None, b"").unwrap();
    link.finish_with_error();
    let expected = [frame(Kind::Loading, None, b""), frame(Kind::NotStarted, None, b"")].concat();
    DEVICE.with_borrow(|d| assert_eq!(d.bytes, expected));
    assert_terminal(&mut link);
}

#[test]
fn a_local_error_after_ready_closes_without_sending_a_startup_notice() {
    let mut link = fresh();
    link.send(Kind::Ready, None, b"").unwrap();
    assert_terminal(&mut link);
    DEVICE.with_borrow(|d| assert_eq!(d.bytes, frame(Kind::Ready, None, b"")));
}

#[test]
fn every_incomplete_ready_or_answer_remains_the_last_bytes_even_if_capacity_returns() {
    for (kind, ids, body) in [(Kind::Ready, None, &b""[..]), (Kind::Chunk, Some((1,1)), &b"Hello!"[..])] {
        let bytes = frame(kind, ids, body);
        for split in 0..bytes.len() {
            let mut link = fresh();
            if ids.is_some() { link.send(Kind::Ready, None, b"").unwrap(); }
            let prefix = DEVICE.with_borrow(|d| d.bytes.clone());
            DEVICE.with_borrow_mut(|d| d.remaining = Some(split));
            assert!(link.send(kind, ids, body).is_err());
            assert!(link.failed());
            assert_terminal(&mut link);
            DEVICE.with_borrow(|d| assert_eq!(d.bytes, [prefix.as_slice(), &bytes[..split]].concat()));
        }
    }
}

#[test]
fn an_input_disconnect_and_a_timed_out_progress_tail_both_close_the_stream() {
    let mut link = fresh();
    DEVICE.with_borrow_mut(|d| d.eof = true);
    assert!(link.poll(None).is_err());
    assert!(link.failed());
    assert_terminal(&mut link);

    let mut link = fresh();
    link.send(Kind::Ready, None, b"").unwrap();
    let ready = frame(Kind::Ready, None, b"");
    DEVICE.with_borrow_mut(|d| { d.remaining=Some(9); d.stalled=true; });
    let progress = wire::reading(1,1,1,8).unwrap();
    assert!(link.report_reading((1,1),progress));
    assert!(link.send(Kind::Done,Some((1,1)),b"").is_err());
    assert!(link.failed());
    DEVICE.with_borrow(|d| {
        assert_eq!(d.sleeps,200);
        assert_eq!(d.bytes,[&ready[..],&progress.bytes()[..9]].concat());
    });
    assert_terminal(&mut link);
}

#[test]
fn a_write_error_while_pumping_optional_progress_is_terminal_too() {
    let mut link = fresh();
    link.send(Kind::Ready, None, b"").unwrap();
    DEVICE.with_borrow_mut(|d| d.remaining = Some(1));
    assert!(link.report_reading((1,1), wire::reading(1,1,1,8).unwrap()));
    assert!(!link.report_reading((1,1), wire::reading(1,1,2,8).unwrap()));
    assert!(link.failed());
    assert_terminal(&mut link);
}
