#![no_std]
#![no_main]
//! A TEST WORKER. NOT A MODEL. It stands where a local model service will stand, beside Tailorbird
//! on PCP OS's channel, so that the channel, the records and the screen can be checked end to end
//! before any model exists. It understands nothing: it says so and repeats what it was sent.
//! It is built only when asked for (feature `talk-test-worker`) and belongs on no release image.
//!
//! What it does with a message:
//!  `fail`   answers that the reply failed;
//!  `leave`  says a few bytes and ends, as a service that stops would;
//!  anything else: answers in pieces of eight bytes, a pause between them, and stops at once if told to.
use pcp_game_guest::{call, exit, log, open, sleep_us};
use tailorbird_ui::wire::{pack, Kind, Reader, Record, Step};

fn send(fd: isize, kind: Kind, conversation: u64, reply: u64, body: &[u8]) {
    let Some(packed) = pack(Record { kind, history: false, conversation, reply, body }) else { exit(4) };
    let mut bytes = packed.bytes();
    while !bytes.is_empty() {
        let wrote = call(1, fd as usize, bytes.as_ptr() as usize, bytes.len(), 0, 0, 0);
        if wrote <= 0 {
            log(b"test worker: the other end is gone\n");
            exit(0);
        }
        bytes = &bytes[wrote as usize..];
    }
}

/// Wait about so many milliseconds. One sleep of the guest library is at most 50 ms.
fn pause(milliseconds: u64) {
    for _ in 0..milliseconds / 50 {
        sleep_us(50_000);
    }
}

/// What one whole record was, copied out of the reader.
struct Got {
    kind: Kind,
    conversation: u64,
    reply: u64,
    body: [u8; 480],
    length: usize,
}

/// Read until one record is whole. `waiting`: wait for bytes, or return None when there are none now.
fn next(fd: isize, reader: &mut Reader, waiting: bool) -> Option<Got> {
    loop {
        let mut byte = [0u8; 1];
        let got = call(0, fd as usize, byte.as_mut_ptr() as usize, 1, 0, 0, 0);
        if got == -11 && !waiting {
            return None;
        }
        if got <= 0 {
            log(b"test worker: the other end is gone\n");
            exit(0);
        }
        match reader.take(byte[0]) {
            Step::More => {}
            Step::Broken => {
                log(b"test worker: what arrived is not a record\n");
                exit(3);
            }
            Step::Whole => {
                let Some(record) = reader.record() else { exit(3) };
                let mut body = [0u8; 480];
                body[..record.body.len()].copy_from_slice(record.body);
                return Some(Got { kind: record.kind, conversation: record.conversation, reply: record.reply, body, length: record.body.len() });
            }
        }
    }
}

pcp_game_guest::entry!(worker_main);
#[no_mangle]
pub extern "C" fn worker_main() -> ! {
    let fd = open(b"/dev/talk\0", 2);
    let glance = open(b"/dev/talk\0", 2 | 0x800);
    if fd < 0 || glance < 0 {
        log(b"test worker: no channel\n");
        exit(2);
    }
    log(b"test worker: NOT A MODEL; started\n");
    // As a service checking a model would: loading, then how far the check has got, twice, the
    // second time all of it; and only after that, ready. The numbers are made up: nothing is checked.
    send(fd, Kind::Loading, 0, 0, b"");
    pause(400);
    for (checked, wait) in [(2u64 << 20, 3000), (5u64 << 20, 1000)] {
        let mut body = [0u8; 16];
        body[..8].copy_from_slice(&checked.to_le_bytes());
        body[8..].copy_from_slice(&(5u64 << 20).to_le_bytes());
        send(fd, Kind::Loading, 0, 0, &body);
        log(if checked == 5 << 20 { b"test worker: says it has checked 5 of 5 MiB (made-up numbers)\n" } else { b"test worker: says it has checked 2 of 5 MiB (made-up numbers)\n" });
        pause(wait);
    }
    send(fd, Kind::Ready, 0, 0, b"");
    log(b"test worker: said it is ready\n");
    let mut reader = Reader::new();
    loop {
        let Some(got) = next(fd, &mut reader, true) else { continue };
        if got.kind == Kind::Forget {
            // It keeps nothing of any conversation, so there is nothing to drop; it says it was told.
            log(b"test worker: told to forget a conversation\n");
            continue;
        }
        if got.kind != Kind::Say {
            // A Stop for a reply already finished: nothing to do.
            continue;
        }
        let said = &got.body[..got.length];
        log(b"test worker: was sent a message\n");
        if said == b"fail" {
            send(fd, Kind::Failed, got.conversation, got.reply, b"");
            continue;
        }
        let mut answer = [0u8; 480];
        let opening = b"This is the test worker, not a model. You said: ";
        answer[..opening.len()].copy_from_slice(opening);
        let length = (opening.len() + said.len()).min(answer.len());
        answer[opening.len()..length].copy_from_slice(&said[..length - opening.len()]);
        let mut stopped = false;
        for (index, piece) in answer[..length].chunks(8).enumerate() {
            if let Some(other) = next(glance, &mut reader, false) {
                if other.kind == Kind::Stop && other.conversation == got.conversation && other.reply == got.reply {
                    log(b"test worker: told to stop, and stopped\n");
                    stopped = true;
                    break;
                }
            }
            send(fd, Kind::Chunk, got.conversation, got.reply, piece);
            if said == b"leave" && index == 1 {
                log(b"test worker: leaving in the middle of an answer\n");
                exit(0);
            }
            pause(300);
        }
        if !stopped {
            send(fd, Kind::Done, got.conversation, got.reply, b"");
            log(b"test worker: answer complete\n");
        }
    }
}
