# Spike candidates: recursive self-improvement and cheap-GPU inference

Written for: the project owner, choosing the next spikes. Date: 2026-10-25. Nothing here is in
`ROADMAP.md`; the owner picks.

Goal as stated: recursive self-improvement and fully autonomous use of this harness, on consumer
hardware (RTX 5070 Ti or an AMD card in the same class). Constraint from the project: one person plus
agents, no deadline ([.claude/CLAUDE.md](../../../.claude/CLAUDE.md)).

## 1. Source grades

Paper claims cite PDF pages from the two ingested papers, `(recursivemas p. N)` and `(aide2 p. N)`; the
wiki pages [[recursivemas]] and [[aide2]] hold the detail. Web claims cite a URL and a grade:

- **P**: primary (vendor page, project docs, the paper itself), fetched 2026-10-25.
- **S**: secondary (blog, forum, review). Indicative only.
- **B**: background knowledge, not fetched this session. Re-check before relying on it.

Thresholds below marked *(proposed)* are assumptions with no baseline yet. Each spike's first step is
to measure that baseline; the threshold is then fixed before the experiment, not after.

## 2. What the papers imply

**Recursive self-improvement is shown, but not shown to ignite.** AIDE2 accepted 7 of 99 proposed
rewrites in 8 days (aide2 pp. 5-6). The one test of a discovered agent as its own editor ended at 0.780
against 0.782 for the human-built editor, 3 seeds, and the authors call it inconclusive (aide2
pp. 10-11). Seed noise can accept a rewrite falsely (aide2 p. 13). So "recursive self-improvement"
is the goal, and the evidence base is one fixed-editor run. The spikes below measure it at our scale
before building on it.

**The loop's own gains include prompt-size cuts.** The discovered agent shrank median prompts 7x to
about 50x (aide2 p. 9) and removed context-window failures that ended 5 FML-Bench and 48 ALE-Bench
runs for the starting agent (aide2 p. 27). Token efficiency is therefore something the loop can
discover, which makes the token audit a good first target (S1).

**The paper describes no containment.** No sandbox, threat-model or safety section; the only
enforcement-like items are cost, step and time caps and container setups for the held-out runs (aide2
pp. 3-4, 24). The grader-patch passage cannot be read for write scope (aide2 p. 10). Hence Spike 0.

**Latent agent loops need local open-weight models.** RecursiveMAS reads hidden states and input
embeddings of every agent and trains only the links, with agents frozen (recursivemas pp. 5, 7, 9).
Hosted APIs do not expose that, so this line of work and the GPU question are the same question.
Reported training peak is 15.29 GB per agent against 21.67 GB for LoRA and 41.40 GB for full SFT, run
on H100 and A100 GPUs (recursivemas pp. 13, 25). 15.29 GB leaves under 0.7 GB headroom on a 16 GB
card before the desktop, CUDA context and batch growth, so "fits" is untested.

**Latent steps are not metered by tokens.** Light Math500 used 523, 495 and 519 tokens at 1, 2 and 3
rounds against 1185, 2117 and 3059 for text recursion, while wall-clock time rose (recursivemas
p. 9). The paper does not say whether latent steps count as tokens. Any budget for an autonomous loop
must charge compute, not tokens ([[agent-recursion-os-implications]]).

## 3. Hardware envelope

| | RTX 5070 Ti | RX 9070 XT |
|---|---|---|
| Memory | 16 GB GDDR7, 256-bit (P) | 16 GB GDDR6, 256-bit (S) |
| Bandwidth | 896 GB/s (S; consistent with 28 Gbps x 256 bit / 8) | about 640 GB/s (S) |
| Board power | 300 W (P) | 304 W (S) |
| Compute | 8960 CUDA cores, 1406 AI TOPS (P) | 64 CUs (S) |

NVIDIA: [product page](https://www.nvidia.com/en-us/geforce/graphics-cards/50-series/rtx-5070-family/)
(P); bandwidth from [pre-launch coverage](https://wccftech.com/nvidia-geforce-rtx-5070-ti-16-gb-gddr7-gb203-300-gpu-350w-tbp/amp/)
(S). AMD: the vendor page timed out twice, so every AMD figure is secondary
([bestllmfor](https://bestllmfor.com/meilleur-llm/9070xt-llm/), [Tom's Hardware](https://tomshardware.com/pc-components/gpus/rx-9070-xt-and-rx-9070-specs-reportedly-leaked-up-to-4-096-sps-16gb-vram-and-2-9-ghz-boost)).
Decode is memory-bandwidth-bound, so the 40% bandwidth gap predicts a similar gap in single-stream
decode *(interpretation, to be measured in S4)*.

llama.cpp [build docs](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/docs/build.md) (P)
document HIP (`-DGGML_HIP=ON`) and Vulkan (`-DGGML_VULKAN=1`) backends but do not name gfx1201 (RDNA4)
or sm_120 (consumer Blackwell). Whether either works well is a measurement, not a given.

## 4. Spike candidates, ranked

Order is by dependency first, then value. S0 gates S1 to S3. S4 to S6 have no dependency on S0 and can
run in parallel. K1 to K3 are design-only and consume S4 numbers.

### S0. Containment: sandbox plus external grader

- Question: can an unattended loop be run so the agent has no path to the grader, the held-out data,
  or the loop's own accept rule?
- Why: aide2 pp. 3-4, 10, 24 above; [[agent-recursion-os-implications]] points 3 and 4.
- Method: loop runs in a throwaway git worktree inside a container; grader and held-out tasks mounted
  nowhere the agent process can see; accept is a fixed arg-max script run outside; every accepted
  rewrite gets a tag; a kill file stops the loop.
- Pass: a scripted adversary (agent told to read or edit the grader) fails 20 of 20 attempts *(proposed)*;
  rollback to a tag restores an identical tree hash; kill stops the loop within one step.
- Kill: if the grader needs files the agent must also read, redesign the split before any loop runs.

### S1. First loop at small scale, on the token audit

- Question: does a bi-level loop (inner agent sees a public score, outer loop selects on a private one)
  improve a harness measurably on this repo?
- Target: the prompt surface (about 31 KB always loaded today; 4 bytes per token gives roughly 8k tokens,
  an assumption). Public score: tokens per fixed task. Private score: pass rate on held-out tasks.
- Why: aide2 pp. 3-4, 9. This also produces the audit you asked for, as a measured result.
- Pass: at least one accepted rewrite whose private grade beats the incumbent by more than the
  seed-to-seed spread measured over 3 baseline runs, with tokens down *(proposed: 20%)*.
- Needs from S0. Baseline: incumbent run 3 times, spread recorded.

### S2. Ignition measurement

- Question: does an accepted agent used as its own editor reach the incumbent's grade in fewer steps?
- Why: the paper's own test was inconclusive on 3 seeds (aide2 pp. 10-11); the claim that matters for
  the goal is unproven.
- Pass: reaches the same grade in at most half the steps over at least 5 seeds *(proposed)*. If it does not,
  record that the improver is not improving and stop building on that assumption.
- Needs S1.

### S3. Reward-hacking probe on our own task

- Question: how often does the inner agent win the public score without winning the private one?
- Why: the paper measured 55% falling to 32% on 38 kernel pairs, with no intervals (aide2 p. 8); at that
  size 21, 15 and 12 of 38 are the implied counts ([[aide2]]), so a difference of three pairs is not a result.
- Pass: report the rate with a count and an interval on at least 100 pairs *(proposed)*; flag any lineage
  step where the rate rises. Needs S1.

### S4. Host inference baseline on one 16 GB card

- Question: what decode and prefill speed does a 16 GB card give for a mid-size MoE model at agent-length
  context, and where is the knee?
- Method: `llama-bench` sweep over `--n-cpu-moe`, `-ub`, context (8k, 32k) and KV types. Keep attention
  and shared FFN on GPU, route experts to CPU as needed
  ([guide](https://huggingface.co/blog/Doctor-Shotgun/llamacpp-moe-offload-guide), P: flags `-ngl 999`,
  `--n-cpu-moe`, `-ot "exps=CPU"`). The only reported gains I found are on 12 GB and 8 GB cards (S: Qwen3
  MoE roughly 17 to 34 tok/s on 12 GB when raising `--n-cpu-moe`
  [report](https://aiweekly.co/alerts/qwen3-moe-cpu-expert-offload-doubles-decode-speed-on-12gb-vram)).
  No 16 GB benchmark was found, so this is a gap to fill.
- KV cache: quantized K and V in llama.cpp. Quality data from the PR discussion (P): on one model,
  f16 PPL 6.2322, K and V at q8_0 6.2346, at q4_0 6.4384, with K more sensitive than V
  ([PR 7412](https://github.com/ggml-org/llama.cpp/pull/7412)). A newer KL test (S) found Gemma sensitive
  and Qwen not, with damage concentrated in long documents
  ([localbench](https://localbench.substack.com/p/kv-cache-quantization-benchmark)). Treat q8_0 as the
  candidate and measure, per model, at 32k.
- Pass: usable loop speed *(proposed: 30 tok/s decode, 1000 tok/s prefill at 32k)*, with KL against f16 KV
  below a threshold fixed after the first run. Report the same sweep on the AMD card via HIP and Vulkan.
- Needs: the card. Baseline: first run.

### S5. Speculative decoding on agent traces

- Question: which draft method pays on this workload (repo edits, tool calls), not on chat?
- Why: llama.cpp supports draft-model, EAGLE-3, n-gram and MTP drafts
  ([docs](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/docs/speculative.md), P). EAGLE-3
  claims up to 6.5x without a named baseline or hardware ([arXiv 2503.01840](https://arxiv.org/abs/2503.01840),
  P). The one consumer-adjacent data point found gave 1.84x on a code-refactor task and 0.99x on chat, on
  a data-center 24 GB card with vLLM (S, [Alauda](https://docs.alauda.io/ai/2.4/model_inference/inference_service/how_to/vllm_speculative_decoding.html)).
  Agent edits repeat the prompt heavily, which favors the n-gram methods *(interpretation)*.
- Method: record real agent turns from S1, replay with each draft type, report acceptance rate and
  speedup at greedy sampling (the docs warn stochastic sampling varies across runs).
- Pass: at least 1.3x decode speedup on the recorded traces with identical greedy output *(proposed)*.

### S6. Weight format on consumer Blackwell versus AMD

- Question: GGUF Q4 against NVFP4 against the AMD path, at equal memory, on a fixed task set.
- Why: NVFP4 needs Blackwell and its published quants target vLLM or SGLang, not llama.cpp
  ([Unsloth](https://unsloth.ai/docs/basics/nvfp4), P). One RTX 5090 test showed about 1.5 points lost on
  GSM8K and 30% faster than FP8 (S, [computingforgeeks](https://computingforgeeks.com/nvfp4-quantization-tensorrt-llm/)),
  one model on an RTX 5090, not a 5070 Ti. No llama.cpp NVFP4 data was found.
- Pass: report accuracy and tokens/s per format; adopt a format only if accuracy loss is within the seed
  spread of the S1 task set.

### S7. RecursiveMAS link training on 16 GB

- Question: can the links be trained on one consumer card, and do the gains reproduce at Light scale?
- Why: recursivemas p. 13 (15.29 GB per agent on H100/A100, p. 25). Only 13.12M parameters train
  (0.31%), so the memory is activations and frozen weights, which checkpointing and smaller backbones
  attack *(interpretation)*.
- Method: Light sequential (three small agents, recursivemas p. 8) with gradient checkpointing; compare
  against Recursive-TextMAS with the same models.
- Pass: trains on 16 GB without offload; accuracy within the paper's reported spread (average standard
  deviation 0.0041, recursivemas p. 9) of its Light numbers on at least 3 of 6 benchmarks *(proposed)*.
- Risk: the paper has inconsistent tables (see [[recursivemas]] Assessment), so choose the table to match
  before starting, in writing.

### K1. Compute-unit accounting (kernel design)

- Question: what unit does a per-principal budget charge so latent loops and KV cache are visible?
- Method: from S4 and S7, tabulate forward passes, GPU seconds and KV bytes per task; pick the unit that
  tracks wall-clock best; sketch the scheduler interface on [[scheduling]] and [[agent-scheduler]].
- Pass: a one-page design with the unit justified by S4 data, and a rejected-alternatives list.

### K2. Opaque tensor channel (kernel design)

- Question: what does the kernel enforce on a message whose payload it cannot inspect?
- Method: define identity stamping, size and count limits, and the capability to open the channel, in
  terms of the B2 endpoint ([b2-spike-report](../b2-spike-report.md)). Content policy stays out by
  construction.
- Pass: a design that states which of the project's core-value guarantees survive an opaque payload and
  which do not, written before any code.

### K3. Agent-image install by capability (kernel design)

- Question: can a self-rewrite be a capability-gated install into an agent image with staging and
  rollback instead of an in-place write?
- Why: [[agent-recursion-os-implications]] point 3; ties to S0 (the same shape on a host).
- Pass: design plus a ktest plan using existing OSTD ktests in QEMU
  (`./osdk.sh cargo osdk test`), not run in this task.

### K4. KV cache as a kernel-managed resource (kernel design)

- Question: does the KV cache map onto the wiki's [[paging]] and [[working-set-and-thrashing]] pages well
  enough to give an agent a quota and an eviction policy?
- Why: AIOS snapshots and restores context per agent ([[context-snapshot-restore]]). PagedAttention (vLLM,
  Kwon et al., 2023) manages KV in fixed blocks; recursivemas uses vLLM as a backend (recursivemas
  p. 25). I did not fetch the vLLM paper this session (B).
- Pass: a mapping table, with the places the analogy breaks (KV is recomputable, pages are not).

## 5. Suggested order

1. S0, and S4 in parallel (independent, S4 needs the card).
2. S1 on the token audit, then S2 and S3.
3. S5 and S6 using traces from S1 and the S4 baseline.
4. S7 once S4 shows the card's real memory behavior.
5. K1 to K4 after S4 and S7 supply numbers.

## 6. Objection, stated once

"Fully autonomous" before S0 passes would reproduce the gap the source paper leaves open: the loop
would hold write access to its own measurement. If S0 passes, I build to the stated goal.

## 7. Verified and not verified

- Verified (re-read against the sources today): the paper pages cited; NVIDIA specs; llama.cpp
  speculative and build docs; PR 7412 numbers; EAGLE-3 abstract.
- Not verified: AMD figures (vendor page timed out); every secondary benchmark; the vLLM PagedAttention
  paper; any 16 GB result, since none was found; all thresholds marked *(proposed)*.
