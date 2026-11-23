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
//!  `cut N`  (N in digits, at most 480) sends an answer of exactly N bytes, then says the reply failed;
//!  `raw`    replies with a genuine invalid UTF-8 CHUNK for a native display test;\n//!  `sequence` sends inconsistent reading counts, then waits for Stop; `again` starts a new total;
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

/// Sixteen bytes that say how far: so many, of so many.
fn progress(so_far: u64, of: u64) -> [u8; 16] {
    let mut body = [0u8; 16];
    body[..8].copy_from_slice(&so_far.to_le_bytes());
    body[8..].copy_from_slice(&of.to_le_bytes());
    body
}

/// What one whole record was, copied out of the reader.
struct Got {
    kind: Kind,
    conversation: u64,
    reply: u64,
    body: [u8; 480],
    length: usize,
    /// For a SAY: whether it asks to be told how far the reading has got.
    progress: bool,
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
                return Some(Got { kind: record.kind, conversation: record.conversation, reply: record.reply, body, length: record.body.len(), progress: reader.progress_wanted() });
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
        if said == b"sequence" && got.progress {
            for (read, total, marker) in [
                (3, 7, &b"test worker: sequence first 3/7\n"[..]),
                (2, 7, &b"test worker: sequence regression 2/7\n"[..]),
                (2, 9, &b"test worker: sequence changed total 2/9\n"[..]),
                (4, 7, &b"test worker: sequence advanced 4/7\n"[..]),
            ] {
                send(fd, Kind::Reading, got.conversation, got.reply, &progress(read, total));
                log(marker);
                if read != 4 { pause(4000); }
            }
            let stop = next(fd, &mut reader, true).unwrap_or_else(|| exit(3));
            if stop.kind != Kind::Stop || stop.conversation != got.conversation || stop.reply != got.reply {
                exit(3);
            }
            log(b"test worker: sequence received matching Stop\n");
            continue;
        }
        if said == b"again" && got.progress {
            send(fd, Kind::Reading, got.conversation, got.reply, &progress(1, 5));
            log(b"test worker: replacement read 1/5\n");
            pause(4000);
            send(fd, Kind::Chunk, got.conversation, got.reply, b"ok");
            send(fd, Kind::Done, got.conversation, got.reply, b"");
            continue;
        }
        // Dedicated, model-free end-to-end probe: byte FF must cross the real
        // /dev/talk channel. It is deliberately not valid UTF-8 and is never
        // used by the production model worker or any release image.
        if said == b"raw" {
            send(fd, Kind::Chunk, got.conversation, got.reply, b"go\xff?");
            send(fd, Kind::Done, got.conversation, got.reply, b"");
            log(b"test worker: raw invalid CHUNK sent (67 6f ff 3f)\n");
            continue;
        }
        if said == b"fail" {
            send(fd, Kind::Failed, got.conversation, got.reply, b"");
            continue;
        }
        // `read`: before answering, say how far the reading has got, twice, as a model would while
        // it reads; only if the message asked for that. The numbers are made up: nothing is read.
        if said == b"read" {
            if got.progress {
                send(fd, Kind::Reading, got.conversation, got.reply, &progress(3, 7));
                log(b"test worker: says it has read 3 of 7 tokens (made-up numbers)\n");
                pause(5000);
                send(fd, Kind::Reading, got.conversation, got.reply, &progress(7, 7));
            } else {
                log(b"test worker: was not asked how far the reading has got, and does not say\n");
            }
        }
        let mut answer = [b'W'; 480];
        let opening = b"This is the test worker, not a model. You said: ";
        answer[..opening.len()].copy_from_slice(opening);
        let length = (opening.len() + said.len()).min(answer.len());
        answer[opening.len()..length].copy_from_slice(&said[..length - opening.len()]);
        // `cut N`: exactly N bytes (the answer, cut short or filled out with W), then a failure.
        if let Some(digits) = said.strip_prefix(b"cut ").filter(|d| !d.is_empty() && d.len() <= 3 && d.iter().all(u8::is_ascii_digit)) {
            let exact = digits.iter().fold(0usize, |n, d| n * 10 + (d - b'0') as usize).min(answer.len());
            for piece in answer[..exact].chunks(64) {
                send(fd, Kind::Chunk, got.conversation, got.reply, piece);
            }
            send(fd, Kind::Failed, got.conversation, got.reply, b"");
            log(b"test worker: sent an answer of an exact length and said it failed\n");
            continue;
        }
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
