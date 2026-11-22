//! Qualification guest for the SSE2 worker target. Uses the pinned decoder's tiny reference models.
#![cfg_attr(any(target_os = "none", all(target_os = "linux", feature = "stable-model-worker")), no_std)]
#![cfg_attr(any(target_os = "none", all(target_os = "linux", feature = "stable-model-worker")), no_main)]

#[cfg(not(any(target_os = "none", all(target_os = "linux", feature = "stable-model-worker"))))]
fn main() {}

#[cfg(any(target_os = "none", all(target_os = "linux", feature = "stable-model-worker")))]
extern crate alloc;

#[cfg(any(target_os = "none", all(target_os = "linux", feature = "stable-model-worker")))]
#[path = "../../pcp-os/store/apps/parrots-all-the-way-down/guest/src/heap.rs"]
mod heap;

#[cfg(all(target_os = "linux", feature = "stable-model-worker"))]
#[path = "../src/stable_compare.rs"]
mod stable_compare;

#[cfg(any(target_os = "none", all(target_os = "linux", feature = "stable-model-worker")))]
mod guest {
    use pcp_game_guest::{exit, log, sleep_us, time_us};
    use pcpos_gguf::Index;
    use pcpos_qwen::forward::{Decoder, Error};
    use pcpos_qwen::Model;

    pcp_game_guest::entry!(probe_main);

    fn verify(bytes: &[u8], reference: &str) {
        let index = Index::parse(bytes, bytes.len() as u64).unwrap();
        let model = Model::admit(&index).unwrap();
        let mut decoder = Decoder::new(&model, bytes, 32, 1 << 20, || true).unwrap();
        let mut positions = 0;
        let mut compute_us = 0;
        let mut prompt = [0; 4];
        let mut final_scores = [0.0; 16];
        for line in reference.lines().skip(1) {
            let mut fields = line.split_ascii_whitespace();
            let token: u32 = fields.next().unwrap().parse().unwrap();
            prompt[positions] = token;
            let start = time_us();
            let scores = decoder.step(token, || true).unwrap();
            compute_us += time_us().checked_sub(start).unwrap();
            assert_eq!(scores.len(), 16);
            final_scores.copy_from_slice(scores);
            for &score in scores {
                let expected = f32::from_bits(u32::from_str_radix(fields.next().unwrap(), 16).unwrap());
                assert!((score - expected).abs() <= 2e-5 + 2e-5 * expected.abs());
            }
            assert!(fields.next().is_none());
            positions += 1;
            sleep_us(1000);
        }
        assert_eq!(positions, 4);
        decoder.reset();
        let mut checks = 0;
        let scores = decoder.prefill(&prompt, || {
            checks += 1;
            true
        }).unwrap();
        assert_eq!(scores, final_scores);
        assert!(checks > 2);
        decoder.reset();
        let mut remaining = checks / 2;
        let interrupted = decoder.prefill(&prompt, || {
            remaining -= 1;
            remaining > 0
        });
        assert_eq!(interrupted, Err(Error::Cancelled));
        assert_eq!(decoder.position(), 0);
        assert_eq!(decoder.prefill(&prompt, || true).unwrap(), final_scores);
        log(alloc::format!("Qwen four-step fixture compute_us={compute_us}\n").as_bytes());
    }

    #[no_mangle]
    pub extern "C" fn probe_main() -> ! {
        #[cfg(all(target_os = "linux", feature = "stable-model-worker"))]
        unsafe {
            use super::stable_compare::{bcmp, memcmp};
            let a = [7, 0, 0x80];
            let b = [7, 0, 0x7f];
            assert_eq!(memcmp(core::ptr::null(), core::ptr::null(), 0), 0);
            assert_eq!(bcmp(a.as_ptr(), b.as_ptr(), 2), 0);
            assert!(memcmp(a.as_ptr(), b.as_ptr(), 3) > 0);
            assert!(memcmp(b.as_ptr(), a.as_ptr(), 3) < 0);
            assert_ne!(bcmp(a.as_ptr(), b.as_ptr(), 3), 0);
        }
        verify(
            include_bytes!(concat!(env!("QWEN_FIXTURE_DIR"), "/qwen2.gguf")),
            include_str!("../../pcp-os/PCP_OS_Rust/crates/pcpos-qwen/tests/data/qwen2-forward.txt"),
        );
        verify(
            include_bytes!(concat!(env!("QWEN_FIXTURE_DIR"), "/qwen3.gguf")),
            include_str!("../../pcp-os/PCP_OS_Rust/crates/pcpos-qwen/tests/data/qwen3-forward.txt"),
        );
        log(b"Qwen decoder passed 128 reference logits\n");
        exit(0)
    }
}
