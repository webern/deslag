# Library/API Mode — Design Plan

## TL;DR
Expose `interleave` as a clean Java library so external tools can construct programs,
invoke verification, and consume results programmatically. Most types are already public;
the work is packaging, a stable facade, and a thin result wrapper.

## Current State
- `Program`, `Step`, `SharedState`, `Configuration`, all explorers, `Trace`, `Invariant` — all public.
- Only package-private member: `ModelThread.advance()`.
- `Program` constructor is public; takes `SharedState` + `List<ModelThread>`.
- `DfsResult` is public but carries internal concerns (raw state map, unfiltered traces).
- CLI (`Main`) prints to stdout; no structured return type for programmatic use.
- `build.gradle.kts` applies both `application` and `java-library` plugins. `application` is for CLI usage only; library consumers use the JAR from `build/libs/`.
- Duplicate `HashingStateStore` in `search` and `state` packages was removed; `state.HashingStateStore` with `CanonicalEncoder` is the canonical version.
- `Trace` is immutable (`final`, `List.copyOf()` in constructor).

## Goal
External callers can do:

```java
Program program = Interleave.program(state, thread0, thread1);
VerificationResult result = Interleave.verify(program, Strategy.DPOR);
if (result.hasViolation()) {
    Trace trace = result.failingTraces().get(0);
    Configuration replayed = Interleave.replay(program, trace);
}
```

## Proposed API Surface

### 1. Public Facade: `Interleave`
- `Interleave.program(SharedState state, ModelThread... threads)` — static factory, convenience wrapper over public constructor. Validates non-null/non-empty.
- `Interleave.verify(Program program, Strategy strategy)` — returns `VerificationResult`.
- `Interleave.verify(Program program, Strategy strategy, Invariant invariant)` — with invariant checking.
- `Interleave.replay(Program program, Trace trace)` — replays a trace, returns final `Configuration`.
- Overloads returning `CompletableFuture<VerificationResult>` for async use.

### 2. Stable Result Type: `VerificationResult`
Thin wrapper around internal `DfsResult` plus timing/memory:

```java
public final class VerificationResult {
    public boolean hasViolation();
    public List<Trace> failingTraces();      // TraceOutcome.VIOLATION
    public List<Trace> deadlockedTraces();    // TraceOutcome.DEADLOCK
    public List<Trace> completedTraces();     // TraceOutcome.COMPLETED
    public long statesExplored();
    public Duration wallTime();
    public long peakMemoryBytes();
    public Strategy strategyUsed();
    public String toJson();                   // hand-rolled, no external dependency
}
```

**Relationship to `DfsResult`:**
- `DfsResult` stays internal.
- `VerificationResult` wraps it and filters traces by `TraceOutcome`.
- `DfsResult.traces()` currently conflates violations, deadlocks, and completions. The wrapper uses `Trace.outcome()` annotation at creation time so filtering is unambiguous.

### 3. TraceOutcome Enum
Define in `search` package alongside `Trace`:

```java
public enum TraceOutcome {
    VIOLATION,    // invariant did not hold
    COMPLETED,    // all threads terminated
    DEADLOCK      // no enabled threads, not all terminated
}
```

**Mapping from explorer code:**
| Condition | TraceOutcome |
|-----------|-------------|
| `!invariant.holds(state, config)` | `VIOLATION` |
| `config.allTerminated()` | `COMPLETED` |
| `enabled.isEmpty() && !allTerminated` | `DEADLOCK` |

### 4. Strategy Enum
```java
public enum Strategy {
    DFS,
    STATIC_POR,
    DPOR
}
```
- Maps 1:1 to existing explorers: `DfsExplorer`, `StaticPorExplorer`, `DporExplorer`.
- State stores (hashing, bitstate) are orthogonal to exploration strategies and are not exposed in this API.
- `Strategy.explorer()` returns the corresponding explorer instance.

### 5. Invariant API
Already public in `dev.samhb.interleave.search.Invariant`:
```java
public interface Invariant {
    boolean holds(SharedState state, Configuration config);
}
```
- No promotion needed.
- Combinators (`and`/`or`) deferred to follow-up. Not in acceptance criteria.

### 6. Program Construction
Static factory as convenience wrapper:
```java
Program program = Interleave.program(state, thread0, thread1);
```
- `Program` constructor stays public. The factory adds no extra validation today, but gives us a seam to make it package-private later without breaking callers.

### 7. Serialization
- No JSON on `SharedState` interface — keeps domain model clean.
- Hand-roll JSON in `VerificationResult.toJson()` following existing `ReportWriter.writeJson()` pattern.
- No new dependencies. If structured serialization becomes a real need later, add Jackson/Gson then.

### 8. Package Boundaries
One public package: `dev.samhb.interleave`.
- Public types: `Interleave`, `Strategy`, `VerificationResult`, `Trace`, `TraceOutcome`, `Invariant`, `Program`, `Step`, `SharedState`, `ModelThread`, `Configuration`, `StepOutcome`.
- Implementation types stay in existing subpackages (`core`, `search`, `state`, `por`, `dpor`).
- No `api` or `internal` subpackage.

### 9. JAR Packaging
- `java-library` plugin already applied.
- `application` plugin remains for CLI usage only; library consumers ignore it and use the JAR from `build/libs/`.
- Single-project Gradle stays; no module split needed.

### 10. Backward Compatibility
- Existing public types stay in place; no package moves.
- New types (`Interleave`, `Strategy`, `VerificationResult`, `TraceOutcome`) are additive.
- CLI behavior unchanged.

## Acceptance Criteria
- External Gradle project can add `interleave` as a dependency and compile against `dev.samhb.interleave.Interleave`.
- `Interleave.verify(...)` returns a deterministic `VerificationResult`.
- `VerificationResult.toJson()` produces valid JSON without external libraries.
- `Interleave.replay(program, trace)` returns a `Configuration` reachable by replaying the trace.
- Existing tests pass, except 4 pre-existing failures tracked separately:
  - `DeltaDebuggerTest` x3: `ArrayIndexOutOfBoundsException` in production code
  - `StaticPorExplorerTest` x1: explores more states than DFS
  These are correctness bugs, not infrastructure issues, and are not caused by this work.

## Out of Scope
- OSGi/JPMS module system.
- Remote/async execution beyond `CompletableFuture` wrappers.
- Binary compatibility guarantees across major versions.
- Moving existing public classes to new packages.
- `Step.description()` — no consumer identified; defer.
- `Invariant.and()`/`or()` combinators — defer to follow-up.

## Implementation Order
1. Add `TraceOutcome` enum + update all 3 explorers to set `Trace.outcome()` at creation time.
2. Add `VerificationResult` wrapper + `toJson()` serialization.
3. Add `Interleave` facade + `Strategy` enum (merged: `program()`, `verify()`, `replay()` are all on the same class).
4. Add timing/memory measurement in `Interleave.verify()`.
5. Add integration test from external consumer perspective.
6. Add `examples/` directory with sample usage.
