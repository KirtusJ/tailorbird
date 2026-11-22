# Native model worker: qualification build

This opt-in companion joins the existing Qwen decoder/tokenizer to Tailorbird's
actual `wire` module. It is not a release image or a qualified-speed model choice.
Ordinary UI builds do not enable or link this feature. It has no tool execution,
filesystem grant or Heliograph dependency. Effects still require the kernel's
separate proposal/consent/receipt path.

## Build and launch binding

Use Rust 1.97.0 and the checked-in PCP OS submodule. The kernel must include the
reviewed companion and normal `model-memory` implementation; do not use
`model-memory-fixture` in an image. The source pin alone does not select that
kernel or supply a production EFI preloader.

```sh
PCP_MODEL_MANIFEST_SHA256=bbdaa8cd263d9454159f12b55ec32613d18a4b7b4d75d284ad0cadc932bc1f4b \
  cargo build --locked --release --target x86_64-unknown-none \
  --features model-worker --bin model-worker
```

That pin is the packed, verified 0.5B **reference**, not a release model selection.
The nominated Qwen3-8B media manifest pin is
`2c02ff6f00a2ee802ce09648dfbc56dda38a64b009635f0aeb383f09e1af67dc`.
Set it at compile time for that separate worker build. An absent, malformed or
different pin refuses before Ready. Do not reuse a worker built for one manifest
with another. Record the compiler, source tree, feature list, pin and ELF hash
with each boot. Decoder/tensor crates use opt-level 3; the UI keeps its size profile.

The companion opens only its granted `/dev/model` and `/dev/talk`. It checks the
48-byte descriptor against the compiled pin, reads the exact bounded manifest,
maps exactly the logical model length read-only, then verifies every borrowed
shard and the complete bytes on those same sealed pages. The native mapping is
the unsafe boundary; its slice borrows the mapping owner, which unmaps on Drop.
It relies on the kernel's loader-owned, read-only, NX, sealed and allocator-excluded
page contract. No weights are copied to a second model-sized allocation.

Loading progress names actual bytes checked, with throttled records. Ready follows
successful weight verification, GGUF/model/tokenizer admission, an exact known
chat-template check, fallible buffer allocation, and one actual forward preflight.
The allocator is the existing Parrots guest page allocator, reused by path.

## Worker with hardware floating point

For native speed qualification, the worker has its own optional x86-64 target.
It uses the System V floating-point ABI and baseline SSE2. The target keeps
the PCP OS guest entry point, static large-address layout and disabled red zone.
Build from the Tailorbird repository root with its pinned submodule:

```sh
rustup toolchain install nightly-2026-05-01 --profile minimal --component rust-src
PCP_MODEL_MANIFEST_SHA256=bbdaa8cd263d9454159f12b55ec32613d18a4b7b4d75d284ad0cadc932bc1f4b \
  cargo +nightly-2026-05-01 build --locked --release -Zjson-target-spec \
  -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem \
  --target guest/targets/x86_64-pcpos-worker.json --features model-worker --bin model-worker
```

The output is `target/x86_64-pcpos-worker/release/model-worker`. This path
uses a dated nightly because the custom target schema and Cargo's build-std
interface are unstable. It compiles core, alloc, compiler builtins and worker
dependencies for the same ABI. The kernel, loader and foreground application
use their existing toolchains. Record the worker digest from the actual build
in its image receipt. The manifest in the command selects the 0.5B reference.

The SSE2 qualification workflow produces `model-worker` for the 0.5B reference
and `model-worker-qwen3-8b` for the verified 8B package. The artifact records both
manifest pins and executable hashes. The workflow checks the reference worker's
instructions and boots two tiny decoder probes twice, reversing their order for
the second trial, on the same QEMU CPU and PCP OS kernel. Each
compares four steps for each of Qwen2 and Qwen3 against the pinned reference
scores. Each also interrupts prompt processing, checks the rollback, and retries
the same prompt. The serial output gives guest-clock compute time for each model.
Full-model time to first visible text, named Stop, and floating-point state
across competing processes require the integrated worker trial.

## Conversation behavior and bounds

The worker uses the shared 24-byte TB records, including canonical service IDs,
Stop/Forget identities and 16-byte progress. A host-tested inbox handles partial
input and cancels a queued request if Stop arrives before inference begins. At
most one subsequent SAY is queued. Wrong-direction/malformed records and queue
overflow end the service explicitly.

Qwen2 uses its chat-only ChatML branch. Qwen3 uses the official
`enable_thinking=false` generation prefix. Only the two checksum-pinned template
strings from the nominated files are accepted; arbitrary Jinja is never run in
the guest. User content stays literal; role/tool/reasoning control IDs are inserted
only by the template. The initial sampling policy is deterministic greedy,
lowest-ID ties, with unused and unrelated control tokens excluded. This policy
is for qualification and is not a selected release-quality configuration.

One conversation retains up to 32 messages / 8192 content bytes. Single mode
answers without reading older turns, while retaining the conversation for a
later continuous request. Sent partial replies survive Stop; an active Forget
discards the pending turn and the named retained conversation. KV is reset and
the chosen conversation is processed again per request. There is no silent
history eviction: a full conversation produces a visible refusal. These are
logical resets, not a secure-erasure promise.

Prompt processing now uses atomic `Decoder::prefill`, computing the vocabulary
projection only for the final prompt token. All transformer and KV steps remain.
Cancellation/error rolls the prefill position back; the worker still resets its
sequence after the turn. This build retains the sequential f64 dot reference;
the library's optional reassociated reduction is not selected here.

Limits: 2048 model positions; 128 generated tokens / 4096 reply bytes; 16 MiB
tokenizer-table budget and 768 MiB decoder-buffer budget. Page rounding, metadata,
history and stacks add overhead. Each CHUNK is at most 480 bytes and ends on a
whole UTF-8 character. Incomplete bytes remain private until complete. Tests put
2-, 3- and 4-byte characters across that boundary and compare reassembled wire
bytes. Output limits/errors produce an explicit failed partial response; EOS
alone produces Done. No model text is interpreted as a protocol record.

During inference/tokenization the native channel is polled at most every 4096
library cancellation callbacks, plus between tokens and outgoing records. A
partially written record finishes to retain framing; no further reply record is
started after observed Stop. A persistently full output channel ends after 200
bounded sleep attempts instead of hanging forever. Wall-clock Stop latency and
the exceptional channel-failure behavior require native measurement; this is
not a timing guarantee. Whole-process Stop remains the kernel's responsibility.

## Evidence and open acceptance

The opt-in library tests cover retained partial turns, named Forget, memory limits,
queued/active cancellation, protocol direction and UTF-8 record boundaries.
`model_prompt` runs the exact template construction on real metadata without
weights. `tests/model_prompt_reference.py` compares its first/continuous/Single
prompts against each GGUF's actual Jinja template (host-only Jinja2 3.1.6) and the
pinned llama.cpp tokenizer. Both candidate models passed all three comparisons;
literal delimiter injection and a changed template are refused by the Rust check.
`model_chat` is a <=1 GiB host smoke helper restricted to the verified 0.5B file;
it joins these prompts, forward execution, sampling and decoded text for up to
16 tokens. It is not the native device adapter.

Before atomic prefill, on the shared Windows host, the verified 0.5B file answered `Say hello in one
short sentence.` with `Hello!` (IDs 9707, 0, then EOS). Its 41-token prefill took
61.697508 seconds, first decoded text appeared at 61.697514 seconds, and the
helper finished at 63.944222 seconds. This uncontrolled smoke proves text wiring,
not acceptable speed. It does not qualify the 8B candidate or target hardware.

The decoder's separate 37-token, eight-step comparison at PCP OS `5a87b5e7`
used one executable in step/prefill/prefill/step order. Sequential prompt processing
took 97.21/84.12 seconds with repeated step and 58.32/50.21 with atomic prefill.
All 1,215,488 saved vocabulary scores and eight token IDs matched byte for byte.
The observed mean reduction was 40.1%; shared-host timing varied and does not
predict native speed. The stock native target uses software floating arithmetic.
The optional SSE2 worker above needs separate state-switch and performance measurements.

Native linking is not a boot. Required next evidence: real verified weights through
the actual worker and Tailorbird, loading/refusal controls, Stop during prefill and
decode, queued requests, Single/continuous/New Chat, full memory recovery, and
the exact source/ELF/image identities. B3's separate native scalar and tokenizer
fixtures do not establish this worker's complete behavior. B3 has supplied a
tested consent-scheduling correction; its integrated acceptance and the production
EFI preloader handoff remain independent gates.

B3 subsequently booted the actual pre-prefill worker at Tailorbird `ffffc081`
with the verified 0.5B model: forward preflight reached Ready, Stop interrupted
prefill, New Chat started a second request, and system Stop returned every frame
to the 1920x1080 desktop. Missing-model refusal also passed. These received
observations do not establish a completed answer or acceptance of this updated
worker. See [the exact native observation](https://github.com/KirtusJ/tailorbird/pull/20#issuecomment-7078147192).

The current UI still displays ASCII, replaces other bytes with `?`, flattens
newlines and retains 480 bytes with a cut indicator. The service preserves full
UTF-8; this document does not claim multilingual display acceptance. Target-laptop
latency/throughput/quality, a real approved file task, physical Rufus writing and
USB boot are still required before calling the preloaded ISO ready.
