# Spike: an on-device model for Jinx on the S21 Ultra and S22 Ultra

Date: 2026-10-13 · Status: **step 1 done (runtime pinned); steps 2–4 (measurements) NOT run yet**

## Question

Can the two phones answer a Jinx-sized chat turn locally, with no internet, fast enough and cool enough to
be useful while Coucou's island keeps running?

## Runtime (step 1, from Google's pages on 2026-10-13)

- **LiteRT-LM** (Kotlin). MediaPipe's LLM Inference API is maintenance-only and points here.
- Gradle: `com.google.ai.edge.litertlm:litertlm-android` (the page says `latest.release`; **pin a concrete
  version from Google Maven at spike time**). minSdk and ABIs: not stated on the page — check the artifact.
- API: `Engine(EngineConfig(modelPath = …))`, `engine.initialize()` (up to ~10 s, off the UI thread),
  `engine.createConversation()`, `sendMessage` / `sendMessageAsync` (Flow). Optional `cacheDir` speeds later loads.
- Backends: `Backend.CPU()`, `Backend.GPU()`, `Backend.NPU(nativeLibraryDir)`. GPU needs
  `uses-native-library` entries for `libvndksupport.so` and `libOpenCL.so` (`required="false"`) in the manifest.
- Models (`.litertlm`, Hugging Face): Gemma-4-E2B `litert-community/gemma-4-E2B-it-litert-lm` (2583 MB),
  Gemma 3n E2B `google/gemma-3n-E2B-it-litert-lm` (2965 MB), Gemma 3 1B `litert-community/Gemma3-1B-IT`
  (1005 MB). The page states no licence/acceptance step for downloading Gemma; **verify on the model card
  before shipping a download** (Gemma terms normally require accepting the licence on Hugging Face).
- Reference speed (Google's page, Galaxy S24 Ultra, Gemma-3n-E2B): CPU ≈ 111 tok/s prefill / 16 tok/s decode;
  GPU ≈ 816 / 16. The S21 and S22 are older: expect slower; **not promised until measured**.

## Procedure for steps 2–4 (needs the phones' Wireless-debugging ip:port)

1. Branch `spike/litert-lm`, throwaway activity with one button; logs tag `JinxSpike`:
   load time, time-to-first-token, decode tok/s, `Debug.getNativeHeapAllocatedSize()`.
2. 5-turn script: a 40-token greeting, then four ~60-token prompts built from `JINX-CORE.md` + a short thread.
3. Per phone × model × backend (CPU, GPU): `adb push` the model, `install -r` the spike APK, `am start`,
   read `adb logcat -s JinxSpike`, and `dumpsys battery | grep temperature` before and after. No taps.
4. Pass criteria for phase 1c: TTFT ≤ 8 s warm, decode ≥ 5 tok/s, no OOM or island restart while Coucou runs.

## Results

| Phone | Model | Backend | Load s | TTFT s | Decode tok/s | Peak RAM MB | ΔC | Verdict |
|---|---|---|---|---|---|---|---|---|
| S21 Ultra | Gemma-4-E2B | CPU | — | — | — | — | — | not run |
| S21 Ultra | Gemma-4-E2B | GPU | — | — | — | — | — | not run |
| S21 Ultra | Gemma 3 1B | CPU | — | — | — | — | — | not run |
| S22 Ultra | Gemma-4-E2B | CPU | — | — | — | — | — | not run |
| S22 Ultra | Gemma-4-E2B | GPU | — | — | — | — | — | not run |
| S22 Ultra | Gemma 3 1B | CPU | — | — | — | — | — | not run |

## Decision

Pending the measurements above. Phase 1c (a `LocalModelBrain : LocalBrain` with `via = Via.Local`, model
download with a pinned SHA-256, release after idle) is written from these numbers; `ChatRepository` already
accepts it in its `brains` list.
