---
license: mit
base_model: openbmb/MiniCPM5-1B
library_name: peft
tags:
- build-small-hackathon
- openbmb
- minicpm
- minicpm5
- peft
- lora
- scam-defense
- tiny-titan
- well-tuned
datasets:
- build-small-hackathon/jawbreaker-scam-defense-data
---

# Jawbreaker MiniCPM5-1B LoRA v8

Jawbreaker MiniCPM5-1B LoRA v8 is the final small-model adapter used by the Jawbreaker Hugging Face Space.

- Base model: `openbmb/MiniCPM5-1B`
- Adapter: `build-small-hackathon/jawbreaker-minicpm5-1b-lora-v8`
- App: https://huggingface.co/spaces/build-small-hackathon/jawbreaker
- Dataset/eval bundle: https://huggingface.co/datasets/build-small-hackathon/jawbreaker-scam-defense-data
- GitHub: https://github.com/gowtham0992/jawbreaker

## Task

The model converts one suspicious text, email, or DM into strict JSON for a scam-safety card:

- risk level: `safe`, `needs_check`, `suspicious`, or `dangerous`
- scam type
- short summary
- safest next action
- warning tactics
- scam DNA fields for what the sender pretends to be, how they apply pressure, what they ask for, and what could happen

Jawbreaker is intentionally narrow. It is not a general chatbot. The goal is to help a non-expert pause before clicking, replying, sharing a code, or sending money.

## Training

The adapter was trained with PEFT/LoRA on Modal A100 using synthetic and sanitized scam-defense examples generated in the public repository.

The v8 pass is failure-driven calibration. It targeted two gaps found during earlier v7/fresh-pattern evals:

- wrong-number crypto / gold / trading grooming that could be under-called
- ordinary family, school, pharmacy, and logistics messages that should not be over-called as dangerous

Key training/eval files:

- `training/generate_v8_data.py`
- `training/data/train_v8.jsonl`
- `training/data/dev_v8.jsonl`
- `training/data/test_v8.jsonl`
- `eval/hard_v8_eval.jsonl`
- `eval/reports/jawbreaker-minicpm5-1b-lora-v8-hard632-safetyguard-v4.json`

## Final Eval

Final guarded Modal A100 eval on `eval/hard_v8_eval.jsonl`:

| Metric | Result |
| --- | ---: |
| Cases | 632 |
| Risk accuracy | 579/632, 91.61% |
| Scam type accuracy | 561/632, 88.77% |
| Mean tactic recall | 90.69% |
| Dangerous as safe | 0 |
| Dangerous as needs_check | 0 |
| Safe as dangerous or suspicious | 0 |
| Unsafe action violations | 0 |
| Invalid predictions | 0 |
| Model errors | 0 |

The final report is published in the dataset/eval bundle and in the app repository.

## Runtime Safety

The live app validates model output against a strict schema before rendering. It also applies a deterministic safety guard for obvious high-risk patterns, so a weak small-model response does not render an obvious scam as safe.

If the model fails, returns malformed JSON, or under-calls an obvious danger signal, Jawbreaker falls back to deterministic safety analysis and recommends verification through trusted official channels.

## Limitations

- This is a hackathon prototype, not legal, financial, or cybersecurity advice.
- Training data is synthetic/sanitized, not a proprietary corpus of private user messages.
- The model is optimized for short scam-like messages and may not generalize to long documents.
- The safest action should still be verified by the user through official channels or a trusted person.
