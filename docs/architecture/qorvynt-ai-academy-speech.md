# QORVYNT AI Academy and In-House Speech Architecture

**Status:** Architecture proposal; no AI, speech, TTS, ASR, or cartoon-teacher runtime is claimed to be implemented.
**Scope:** QORVYNT language and Studio only. This does not add a dependency on a hosted AI provider.

## 1. Product goal

Build an original, local-first AI teaching character inside QORVYNT Studio and the browser learning experience. The teacher should explain QORVYNT concepts, demonstrate code, listen to a learner, answer questions, speak responses, and animate its face with synchronized mouth movement.

The long-term goal is to own the model architecture, training pipeline, runtime, voice, and teaching behavior. Operating-system audio drivers and documented hardware interfaces are allowed as device interfaces; they are not the AI service or the teacher's intelligence.

## 2. No hidden third-party AI dependency

- No mandatory hosted LLM, hosted TTS, hosted speech-recognition API, or vendor-specific AI account.
- Core lessons, examples, syntax help, and deterministic exercises must work offline.
- Local inference is the default target when the device has enough CPU/RAM/GPU capacity.
- Downloadable model packs must be versioned, checksummed, licensed, and optional.
- A learner must be told when a feature requires a model pack, network access, or more capable hardware.
- Optional external integrations, if ever added, must be clearly labeled and disabled by default.

Using an OS audio device API to capture or play sound does not make the AI model a third-party service. The speech models and inference logic remain QORVYNT-owned.

## 3. System components

| Component | Responsibility | Initial delivery |
|---|---|---|
| Academy content engine | Lessons, examples, quizzes, progress, translations | Deterministic offline content |
| QORVYNT Tutor Core | Lesson-aware explanations and safe tool routing | Rule-based tutor before local model |
| QORVYNT Language Model | Natural-language tutoring and code explanations | Research/training track; not yet available |
| QORVYNT TTS | Convert generated text into speech | First prototype after phoneme/audio pipeline |
| QORVYNT ASR | Convert learner speech into text | First prototype after dataset and evaluation plan |
| Voice runtime | Audio input/output, buffering, device selection | Native host integration |
| Cartoon presenter | Character states, expressions, gestures, subtitles | 2D first |
| Lip-sync | Viseme timing from phonemes/audio | Deterministic viseme driver first |
| Studio integration | Teacher panel, lesson UI, code sandbox, accessibility | UI only after stable APIs |

## 4. Speech pipeline

### Text-to-speech

Tutor response -> text normalization -> pronunciation/phoneme sequence -> acoustic model -> vocoder -> audio stream -> audio device.

The TTS project must define its own supported phoneme inventory, pronunciation dictionary, Hindi/English code-switching rules, model format, streaming interface, and quality evaluation set. A small deterministic prototype may speak a limited set of recorded or generated phoneme units, but that is not equivalent to natural neural speech.

### Speech recognition

Microphone -> capture/voice activity detection -> audio features -> acoustic/sequence model -> decoder -> transcript -> tutor.

The first milestones are microphone permissions, recording limits, voice activity detection, transcript confidence, and an offline evaluation corpus. Recognition must show uncertain words rather than silently inventing them.

### Cartoon synchronization

The presenter consumes semantic events and viseme timestamps. The first version can animate mouth shapes from phoneme/viseme events; later versions can add expressions, gestures, and turn-taking. Animation must remain usable with audio muted and with reduced-motion settings.

## 5. Tutor safety and code execution

- The tutor may explain, propose, and generate .qv code.
- Running code requires an explicit learner action or a clearly scoped lesson sandbox.
- Generated code runs in the existing QORVYNT sandbox, never with ambient filesystem, network, USB, or GPU permissions.
- Hardware access is capability-gated and unrelated to whether the AI model is trusted.
- Model output is untrusted input: parse, validate, compile, and enforce runtime permissions before execution.
- The tutor must distinguish verified language features from experimental or planned features.

## 6. Model ownership and training

Owning a model requires more than writing an inference wrapper. QORVYNT needs:

1. A lawful, documented, language-balanced dataset with clear redistribution and training rights.
2. Tokenizer and phoneme specifications with reproducible versions.
3. Model architecture and training code maintained in QORVYNT-controlled repositories.
4. Reproducible training recipes, checkpoints, evaluation sets, and model cards.
5. Inference formats and quantized model packs suitable for ordinary computers.
6. Security, privacy, bias, hallucination, and quality evaluations before public claims.

A useful Hindi/English voice and conversational tutor may require substantial training data and compute. The code can be open-source and free to use, but training compute, hardware, data preparation, and distribution bandwidth are real costs. Do not promise high-quality offline neural speech on every device until measured.

## 7. Phased implementation gates

### Phase A — Offline Academy foundation
- [ ] Lesson manifest format with stable IDs and versioning.
- [ ] Beginner QORVYNT lessons, runnable examples, quizzes, and progress state.
- [ ] Searchable built-in reference generated from the actual language grammar and tested examples.
- [ ] Tutor API that can explain only features marked implemented/experimental/planned.
- [ ] Tests that prove lessons do not require network access.

### Phase B — Audio device and presenter foundation
- [ ] Native audio input/output abstraction with explicit user permission.
- [ ] Device selection, mute, volume, and accessibility controls.
- [ ] 2D cartoon state machine and subtitle rendering.
- [ ] Phoneme-to-viseme event interface and deterministic lip-sync tests.
- [ ] No cloud speech service or remote audio upload.

### Phase C — In-house speech research
- [ ] Hindi/English speech dataset and licensing review.
- [ ] Reproducible ASR training/evaluation pipeline.
- [ ] Reproducible TTS training/evaluation pipeline.
- [ ] Offline inference prototype and model-pack format.
- [ ] Latency, memory, word error rate, intelligibility, and device compatibility reports.

### Phase D — In-house tutor model
- [ ] QORVYNT-specific documentation and code corpus with provenance.
- [ ] Local model training/fine-tuning experiments and held-out evaluations.
- [ ] Tool-call schema restricted to read-only project help by default.
- [ ] Sandboxed code proposal and learner-confirmed execution.
- [ ] Model cards, known limitations, and reproducible releases.

### Phase E — Studio integration
- [ ] Cartoon teacher panel in Windows Studio and browser playground.
- [ ] Voice conversation with push-to-talk and visible microphone state.
- [ ] Hindi/English selection and adjustable speech speed.
- [ ] Offline fallback to text lessons and deterministic help.
- [ ] End-to-end tests on real supported hardware before claiming compatibility.

## 8. Cost and hardware policy

- QORVYNT source code, lesson content, and model formats should be free/open-source under compatible licenses.
- No dedicated GPU is required to write QORVYNT programs or use the deterministic Academy lessons.
- Local neural inference may need sufficient RAM/CPU/GPU; small quantized models are a later optimization target.
- Training original speech and language models may need substantial compute. GitHub-hosted CI is for tests and packaging, not a promise of free large-scale model training.
- USB drives and optional accelerators are not required for the Academy.
- Paid hosted inference must never be a hidden requirement.

## 9. Release honesty checklist

Do not advertise “QORVYNT has its own AI voice” until an in-repository model can synthesize speech offline and has reproducible quality results. Do not advertise speech recognition until offline microphone-to-transcript tests pass. Do not advertise a cartoon AI teacher until the Studio/browser integration works end-to-end. Until then, label these items planned or experimental.

## 10. Current baseline

The repository currently has a Rust-hosted compiler/VM, a QORVYNT-written source frontend, a browser demo playground, and an early Windows Studio shell. The omni-domain document describes planned runtime profiles; it does not implement them. This document adds an architecture and gated roadmap only. No speech model, AI tutor model, audio engine, cartoon teacher, or GPU/USB runtime is included by this commit.
