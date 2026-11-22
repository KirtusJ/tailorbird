//! Qualification guest for the SSE2 worker target. Uses the pinned decoder's tiny reference models.
#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(not(target_os = "none"))]
fn main() {}

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
#[path = "../../pcp-os/store/apps/parrots-all-the-way-down/guest/src/heap.rs"]
mod heap;

#[cfg(target_os = "none")]
mod guest {
    use pcp_game_guest::{exit, log, sleep_us, time_us};
    use pcpos_gguf::Index;
    use pcpos_qwen::forward::{Decoder, Error, PrefillEvent};
    use tailorbird_ui::{model_reading::ReadingProgress, wire::{self, Kind, Reader}};
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
        // Exercise the real decoder notifications and the worker's rate limiter
        // with a deterministic clock. Cancel at a reported boundary and retry.
        for (wanted, stop_at) in [(false, None), (true, Some(3)), (true, None)] {
            decoder.reset();
            let mut progress = ReadingProgress::new(wanted, 4, 9);
            let mut reports = 0;
            let result = decoder.prefill_with_progress(&prompt, |event| {
                if let PrefillEvent::TokenProcessed { completed, total } = event {
                    let elapsed = || { assert!(wanted); completed as u64 * 250_000 };
                    if let Some(packet) = progress.token_processed(completed, total, elapsed) {
                        let mut reader = Reader::new();
                        for &b in packet.bytes() { reader.take(b); }
                        let r = reader.record().unwrap();
                        reports += 1;
                        assert_eq!((r.kind, r.conversation, r.reply), (Kind::Reading, 4, 9));
                        assert_eq!(wire::progress(r.body), Some((reports, 4)));
                    }
                    return Some(completed) != stop_at;
                }
                true
            });
            if stop_at.is_some() {
                assert_eq!(result, Err(Error::Cancelled));
                assert_eq!(decoder.position(), 0);
                assert_eq!(reports, 3);
            } else {
                assert_eq!(result.unwrap(), final_scores);
                assert_eq!(reports, if wanted { 4 } else { 0 });
            }
        }
        log(b"Qwen worker prefill progress passed opt-in, Stop and retry\n");
        log(alloc::format!("Qwen four-step fixture compute_us={compute_us}\n").as_bytes());
    }

    // Real release-model block layouts, including both halves of each packed byte.
    fn verify_quantized() {
        use pcpos_model_tensor::{Error, Projection, Row, TensorType};
        let projection = Projection::detect();
        let values: [f32; 512] = core::array::from_fn(|i| ((i * 31 % 251) as f32 - 125.0) / 64.0);
        for kind in [TensorType::Q4K, TensorType::Q6K] {
            let size = kind.block_layout().1 as usize;
            let mut encoded = alloc::vec![0; 2 * size];
            for (i, byte) in encoded.iter_mut().enumerate() {
                *byte = i.wrapping_mul(37).wrapping_add(i / 7) as u8;
            }
            for block in encoded.chunks_exact_mut(size) {
                if kind == TensorType::Q4K {
                    block[..4].copy_from_slice(&[0, 0x38, 0, 0x34]);
                } else {
                    block[208..].copy_from_slice(&[0, 0x38]);
                }
            }
            let row = Row::new(kind, values.len(), &encoded).unwrap();
            let expected = row.dot(&values, || true).unwrap();
            assert!(expected.is_finite() && expected != 0.0);
            let mut checks = 0;
            assert_eq!(projection.dot(&row, &values, || {
                checks += 1;
                checks < 2
            }), Err(Error::Cancelled));
            assert_eq!(checks, 2);
            assert_eq!(projection.dot(&row, &values, || true).unwrap().to_bits(), expected.to_bits());
        }
        log(alloc::format!("Qwen Q4_K/Q6_K projections passed with {}\n", projection.name()).as_bytes());
    }

    #[no_mangle]
    pub extern "C" fn probe_main() -> ! {
        verify_quantized();
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
