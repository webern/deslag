# Features

A tour of what NilaMind does. Everything here runs on-device.

## Talk to Nila
- **Text or voice chat** with Nila, a warm companion grounded in real therapy approaches who listens first and offers one small, concrete thing only when it helps.
- **Voice-first input** — tap to talk is primary; typing is one tap away (`ModeScreen.tsx`, `voice.ts`).
- **Spoken replies** — Nila's replies can be read aloud by default (on-device TTS).
- **Hands-free call mode** — a phone-call-style conversation.
- **"Hey Nila" wake word** — an on-device Vosk wake-word path (`wakeWord.ts`, `wakePrefs.ts`).

## Check-ins & mood
- **Opening check-in** — a light tap-based check-in when you open the app (`NilaCheckIn`, `checkin.ts`).
- **Mood tracking** over time (`moodHistory.ts`), surfaced in the dashboard and insights.

## Insight (your own trajectory, not a benchmark)
- **Dashboard & insights** — mood history, patterns, streaks (`DashboardScreen`, `dashboardInsights.ts`, `patternInsights.ts`, `streaks.ts`).
- **Inflection awareness** — Nila can gently notice and name a real shift in *your own* trajectory (off by default, at most once a day, §9-safe) using on-device, valence-aware trend detection (`nilaInflection.ts`, `inflectionPrefs.ts`).
- **Sleep signal** — optional sleep input (and a Health Connect path) feeding a manic-prodrome short-sleep detector (`sleepInsight.ts`, `healthConnect.ts`).

## Evidence-based skills
A library of short, research-based skills you can open and follow (`SkillsLibraryScreen`, `skillsLibrary.ts`, retrieved contextually by `skillRetrieval.ts`):
- **Thought Record** (CBT) — `ThoughtRecordScreen`
- **Diary Card** (DBT) — `DiaryCardScreen`
- **Behavioural Activation** — `behaviouralActivation.ts`
- **Values → Action** (ACT) — `ValuesToActionScreen`, `values.ts`
- **Self-Compassion** (CFT)
- **Wind-Down** (CBT-I, for sleep) — `WindDownScreen`, `windDown.ts`
- **Grounding library** — `GroundingLibraryScreen`
- **Psychoeducation** — `LearnScreen.tsx`, `psychoed.ts`
- **Safety Plan** — `SafetyPlanScreen`

## Staying connected to real people
- **"Letter to my unwell self" pact** — write, while well, what you want to hear when you're not; it can be surfaced back to you later (`PactScreen`, `pact.ts`, `pactNotice.ts`).
- **Notice → letter → human handoff** — a signal can lead to your own letter and a gentle nudge toward a trusted person.
- **Reach-out bridge** — a guided way to reach a trusted person (`ReachOutScreen`, `reachOut.ts`).
- **Dependency guard** — if you're leaning on Nila too heavily, she nudges you toward real human connection rather than becoming a replacement for it.

## Memory you control
- **Compounding memory** — Nila can hold a couple of stable facts and what you're working through, **only with your consent**, and surface them like a friend who remembers (`nilaMemory.ts`, `nilaInsights.ts`).
- **"What Nila remembers"** — view, edit, and delete all of it (`NilaMemoryScreen`).

## Other
- **Agent console** *(not yet shipped)* — a proposed surface for the more agent-like "notice and show up" behaviours. The phone-usage / "notice and show up" path currently ships dark (`phoneEnabled` is hardcoded `false`).
- **Assessments** — optional structured check-ins (`AssessmentScreen`, `assessments.ts`).
- **Identity & lock** — BIP39 on-device identity (`IdentityOnboarding`) with an optional biometric gate on sensitive actions (`BiometricGateHost`).
- **Theming** — light/dark/system.

> Every feature above is designed around the same constraint: it should *help*, on-device, without ever harvesting your data. Where a feature could touch safety, the [§9 layer](Crisis-Safety.md) runs in front of it.
