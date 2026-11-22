# Qwen Coder reference evidence

Recorded by C3 on **21 November 2026, America/Vancouver**. These are read-only
Windows CPU trials for the fresh Tailorbird backend. No model output was executed,
no PCP OS model was admitted or loaded, and no file-creation authority was tested.
The ordinary Tailorbird application remains disconnected.

The practical finding is that this candidate can draft text, but neither a JSON
grammar nor this small prompt revision makes exact copying reliable. Keep generated
content visibly proposed, let the user inspect it, and show creation only after a
matching kernel receipt. The initial typed-content path can preserve the user's
bytes directly while model support is being built.

## Reproduce the evidence review

From the repository root, Python 3.11 or later, standard library only:

```sh
python3 qa/model-reference/replay.py
```

This checks the frozen file digests, re-scores all **32** recorded outputs, checks
the exact-input inference prompts against vocabulary-only token runs, and exercises
11 adverse scoring cases. A passing check means the recorded successes **and
failures** are reproduced. It does not mean the model passed the tasks. It performs
no inference and downloads nothing. The workflow has the same scope on Linux and
Windows. This directory disables Git line-ending conversion so evidence hashes
refer to the same bytes on both platforms; the first Windows Git checkout exposed
that portability issue, and this rule fixes it without relaxing the digest checks.

`reference.json` contains exact prompt text, raw stdout encoded as base64, complete
stderr, commands, expected results, timing/memory observations and every outcome.
`token-reference.json` has 18 vocabulary-only reference cases, including line
endings, literal escapes, Unicode, special-token-looking text, the chat wrapper,
and all 11 distinct corrected inference prompts. These are future Rust parity
fixtures, not proof that a native tokenizer exists.

## Input correction and results

The first 16 trials used llama-completion's `-f` option. Its source removes the last
LF from the file, and default escape processing transforms literal backslash
sequences. That changed the intended assistant prefix and the revised prompt's
examples. Those runs remain in the evidence as `historical-*`; do not use them as
the final baseline. The original scripts are retained in `historical/`.

The corrected 16 trials use `--binary-file --no-escape`, explicit CPU/F16-cache
settings, no prompt cache, and offline mode. Verbose inference token IDs match
separate vocabulary-only runs, including the final assistant newline. This fixes
the harness without repairing or normalizing generated text. The upstream behavior
is visible in [the pinned argument parser](https://github.com/ggml-org/llama.cpp/blob/a956a852c/common/arg.cpp)
and [tokenize tool](https://github.com/ggml-org/llama.cpp/blob/a956a852c/tools/tokenize/tokenize.cpp).

The exact-input harness used for those frozen trials is retained in
`historical/run_reference_exact.py`. The current runner and replay share
`scoring.py`: both reject duplicate JSON keys and invalid UTF-8. The runner records
malformed stdout bytes without repair, writes the failed trial's result and
continues the next case. `test_runner_output.py` exercises that sequence on Windows
with controlled process output and no model. The frozen outcomes are unchanged;
no inference was rerun for this recording correction.

| Corrected trial set | Valid JSON | Exact requested result |
|---|---:|---:|
| Initial prompt, unconstrained | 4 / 5 | 0 / 5 |
| Initial prompt, JSON schema | 5 / 5 | 1 / 5 |
| Revised prompt, JSON schema, different requests | 6 / 6 | 3 / 6 |

Each set includes one unsupported-action request. The exact-result score for it
checks classification and empty filename, not the quality of its explanation.
Of the text-drafting requests alone, the constrained initial set passed **0 / 4**
and the revised set **2 / 5**. These are small exploratory sets, not a representative
benchmark, statistical comparison, security evaluation or model-selection verdict.

Concrete failures after the input correction:

- `Hello, world!` and the two-line reminder lost the requested final LF.
- `Café` became `Cafe` in the initial prompt.
- Quoted words about granting permission were classified as an unsupported action
  instead of copied as data. The revised prompt got its different quoted example right.
- A requested blank line became the literal words `an empty line`.
- `Wait.` with no newline became `Wait` followed by a newline.
- The revised Unicode example retained its accents but added an unwanted LF.

The scorer strips only outer transport whitespace and the tool's final
`[end of text]` marker. It does not remove Markdown fences, alter the JSON text
field, repair missing newlines or normalize Unicode. Valid JSON is a format result,
not content correctness and never permission.

## Resource observations and native work

The candidate is the official
[Qwen2.5-Coder-0.5B-Instruct-GGUF](https://huggingface.co/Qwen/Qwen2.5-Coder-0.5B-Instruct-GGUF/tree/ebb2015119c907b064c512bf053e945850b5875f),
revision `ebb2015119c907b064c512bf053e945850b5875f`, Q4_K_M file, **491,400,064 bytes**,
SHA-256 `1d9614638d18024d0fbb36575a15f1302a3adf044df10345688ec4f6e1c4ff32`.
It differs from the older general Instruct example. Model and runner identities,
the upstream archive hash and runtime DLL hashes are in `identity.json`. Weights
and executable binaries are deliberately absent from this repository.

The verified GGUF has 291 tensors: F32 (121), Q5_0 (133), Q8_0 (13), Q4_K (12),
and Q6_K (12). `tensor-layout.json` checks block sizes, nonoverlap and ends against
the file size using the pinned [GGML definitions](https://github.com/ggml-org/llama.cpp/blob/a956a852c/ggml/src/ggml-common.h).
Q4_K_M is not a promise that every tensor uses one format. The metadata includes
24 layers, width 896, 14 attention heads, 2 KV heads, RMS epsilon approximately
1e-6, QKV biases, and a 151,936-token vocabulary with Qwen2 pretokenization.

For a proposed scalar F32 KV cache, the **calculated** cost is 24,576 bytes per
token: 12 / 24 / 48 MiB at context lengths 512 / 1,024 / 2,048. This excludes
weights, activations, tokenizer tables, allocator overhead, UI, kernel reserve and
receipts. It is not a native memory measurement. The host trials use **F16** KV.

On this i5-10210U Windows host (8 logical processors, 16,951,238,656 bytes RAM),
using one inference thread, context 1,024, batch/microbatch 64 and at most 160 output
tokens, the corrected trials observed:

- Total process time: 5.404–10.478 seconds.
- First non-whitespace stdout: 3.618–7.787 seconds. This includes loading/prefill
  and buffering; it is not a native input-to-frame measurement.
- Reported decode speed: 12.51–22.21 tokens/second.
- Sampled process peak working set: 507,858,944–531,415,040 bytes.
- Sampled maximum private commit: 193,712,128–212,287,488 bytes.

Memory sampling uses [Windows process counters](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex)
approximately every 50 ms. Working set and private commit describe different
things and must not be added. The host has paging, mapped weights and an optimized
AVX2-capable runtime; the native scalar implementation has none of that established.
The CPU was not isolated and runs were not randomized. These observations do not
admit a model on a 1 GiB PCP OS machine or predict its native speed.

Remaining C3 implementation work: bounded Rust model reads and all five tensor
formats; tokenizer parity; numerical parity for Qwen2 layers; loading/prefill/layer
work that yields to input and drawing; full native memory and Stop measurements;
then connection to the agreed kernel proposal/receipt interface. A3 owns media
admission, B3 native resource/Stop evidence, and Z3 the kernel boundary and theme.
llama.cpp is an external QA reference, not a shipping runtime dependency.

## User-facing consequences

- Show the full desktop/app context, with an explicitly named artifact preview.
  A model's generated sentence cannot set an applied state.
- Show real activity immediately during loading and prompt processing, keep Stop
  reachable, and use measured counts rather than invented completion percentages.
  This follows [GNOME progress guidance](https://developer.gnome.org/hig/patterns/feedback/progress-bars.html)
  and [Microsoft progress guidance](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/progress-controls).
- Keep the result readable in the app and make returning to background work
  discoverable. Individual community reports want both
  [foreground progress near the work](https://discuss.kde.org/t/file-operations-in-foreground/39661)
  (12 September 2025) and [compact background progress](https://discuss.kde.org/t/played-around-with-settings-i-didnt-understand-dolphin-file-copy-popup-dialogue-box/47004)
  (14 May 2026); these are different preferences, not a consensus or a PCP OS user study.
- A first ASCII-only trusted screen must refuse unsupported characters explicitly;
  silently replacing an accent is not an acceptable workaround. Show session-only
  durability plainly. Use controlled plain-language status/error copy instead of
  a model's invented saved-file or administrator claims.
- Pixel rendering alone does not supply assistive-technology status semantics;
  [status-message accessibility guidance](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html)
  remains an implementation obligation, not a conformance claim here.

## Rerun inference on Windows

Obtain and checksum the exact upstream model and b12387 Windows CPU archive from
the links above. Keep downloads, extraction, caches and output in `D:\C3`. With
Python 3.11 or later and a **new** output directory:

```powershell
$env:PYTHONPYCACHEPREFIX = 'D:\C3\pycache'
python qa/model-reference/run_reference.py --model D:\C3\model-workbench\qwen2.5-coder-0.5b-instruct-q4_k_m.gguf --runner D:\C3\tools\llama-b12387\bin\llama-completion.exe --output D:\C3\model-workbench\rerun-01
```

The runner verifies model and executable hashes, preserves each input/output,
uses no shell to launch the process, bounds each trial to 120 seconds and never
executes generated content. Match the archive/DLL hashes as well for a comparable
runtime. The inspection and vocabulary-collection scripts retain their recorded
local paths; adjust those constants for another reference workspace. Hashes
establish artifact identity, not trust or a safe model-admission decision.
