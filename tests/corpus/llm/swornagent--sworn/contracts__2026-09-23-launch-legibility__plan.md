```sworn-release-manifest-v1
{
  "approval_ref": "operator://2026-09-23-launch-legibility/1",
  "previous_plan": null,
  "release": "2026-09-23-launch-legibility",
  "repository": "sworn",
  "revision": 1,
  "schema_version": "sworn.release-manifest/v1",
  "target_ref": "refs/heads/release/2026-09-23-launch-legibility",
  "tracks": [
    {
      "depends_on": [],
      "id": "T1-launch-legibility",
      "slices": [
        {
          "consumes": [],
          "contract_path": "contracts/2026-09-23-launch-legibility/S1-launch-refusals.json",
          "depends_on": [],
          "digest": "sha256:b8e30279af0664c54928de7d14985664daaae7b1f2f9aedfe931065f074a8293",
          "id": "S1-launch-refusals",
          "outcome": "Every launch refusal names its cause: serve prints the typed code and which input failed instead of one generic line, the operator config refusal distinguishes an unsafe file mode from a malformed file, and plan record, pin and lint accept an abbreviated object id resolved through Git or name the flag that carried a bad one, so no launch failure has to be diagnosed by probing the code.",
          "touchpoints": [
            "cmd/sworn",
            "internal/runtime",
            "internal/journal",
            "internal/gitx",
            "internal/driver",
            "internal/cockpit",
            "internal/observe",
            "internal/tui",
            "internal/skill",
            "internal/protocol",
            "tools/protocolgolden",
            "test/e2e",
            "docs/run.md",
            "docs/launch.md",
            "docs/policy/manager.md",
            "README.md"
          ]
        },
        {
          "consumes": [],
          "contract_path": "contracts/2026-09-23-launch-legibility/S2-canonical-authoring.json",
          "depends_on": [],
          "digest": "sha256:786d9df4d866663cfc941b22045d6244ac7d4bf3e8490cbce0767f35e9edf6d6",
          "id": "S2-canonical-authoring",
          "outcome": "An operator or Manager seat produces every canonical launch input with a command instead of reverse-engineering the encoder: `sworn manifest canonical` prints or writes the exact bytes admission accepts for a runtime manifest or a driver config, plan pin can write the pinned plan in place, and the NONCANONICAL and STALE_BINDING refusals name the command that fixes them.",
          "touchpoints": [
            "cmd/sworn",
            "internal/runtime",
            "internal/journal",
            "internal/gitx",
            "internal/driver",
            "internal/cockpit",
            "internal/observe",
            "internal/tui",
            "internal/skill",
            "internal/protocol",
            "tools/protocolgolden",
            "test/e2e",
            "docs/run.md",
            "docs/launch.md",
            "docs/policy/manager.md",
            "README.md"
          ]
        },
        {
          "consumes": [],
          "contract_path": "contracts/2026-09-23-launch-legibility/S3-host-check-failure-facts.json",
          "depends_on": [],
          "digest": "sha256:3373b8f2879021b5fabdd30d29ee0abd6b877e1f2346e5919528e5e7d494bbc2",
          "id": "S3-host-check-failure-facts",
          "outcome": "When a candidate's host check fails, the status projection shared by sworn status, sworn_status, the board and the TUI says which check failed, its exit code, whether it was re-executed and with what result, which declared checks were therefore not run, and a bounded excerpt of its output; check effects show whether the check passed, not only that it executed; and an assembly BLOCKED park's advice names the route that fits it.",
          "touchpoints": [
            "cmd/sworn",
            "internal/runtime",
            "internal/journal",
            "internal/gitx",
            "internal/driver",
            "internal/cockpit",
            "internal/observe",
            "internal/tui",
            "internal/skill",
            "internal/protocol",
            "tools/protocolgolden",
            "test/e2e",
            "docs/run.md",
            "docs/launch.md",
            "docs/policy/manager.md",
            "README.md"
          ]
        },
        {
          "consumes": [],
          "contract_path": "contracts/2026-09-23-launch-legibility/S4-lane-live-probe.json",
          "depends_on": [],
          "digest": "sha256:93778d7180bed14b43f4ec7bcc6f9b1c2d28caef239b506938af77135712bf72",
          "id": "S4-lane-live-probe",
          "outcome": "One cheap, bounded live probe proves a configured lane is admitting requests right now, and the whole-roster readiness check names what it is missing: `sworn driver probe` sends one minimal request for an explicitly named profile and model and reports the provider's typed refusal and request id, and `doctor --all` and `certify --all` name every missing production family and surface from one roster declaration.",
          "touchpoints": [
            "cmd/sworn",
            "internal/runtime",
            "internal/journal",
            "internal/gitx",
            "internal/driver",
            "internal/cockpit",
            "internal/observe",
            "internal/tui",
            "internal/skill",
            "internal/protocol",
            "tools/protocolgolden",
            "test/e2e",
            "docs/run.md",
            "docs/launch.md",
            "docs/policy/manager.md",
            "README.md"
          ]
        },
        {
          "consumes": [
            "S4-lane-live-probe"
          ],
          "contract_path": "contracts/2026-09-23-launch-legibility/S5-transient-provider-backoff.json",
          "depends_on": [
            "S4-lane-live-probe"
          ],
          "digest": "sha256:edd651cde23ec9486f0023c3c50ae3d0e64c0d3f2de2767a88eda3a4010b88fc",
          "id": "S5-transient-provider-backoff",
          "outcome": "A transient provider stall no longer burns the try budget: after a try fails with PROVIDER_UNAVAILABLE, or PROVIDER_LIMITED with no reset time, the engine waits with a bounded backoff and starts the next try only once a live probe of the same lane passes, journaling every wait and probe, and parks with a typed provider-stall cause naming the probe results when the stall outlasts its bound.",
          "touchpoints": [
            "cmd/sworn",
            "internal/runtime",
            "internal/journal",
            "internal/gitx",
            "internal/driver",
            "internal/cockpit",
            "internal/observe",
            "internal/tui",
            "internal/skill",
            "internal/protocol",
            "tools/protocolgolden",
            "test/e2e",
            "docs/run.md",
            "docs/launch.md",
            "docs/policy/manager.md",
            "README.md"
          ]
        },
        {
          "consumes": [],
          "contract_path": "contracts/2026-09-23-launch-legibility/S6-context-window-clamp.json",
          "depends_on": [],
          "digest": "sha256:9f8411411a78fcacfdca781e65da4d65a671dc4a16426cdd517872835ccd861a",
          "id": "S6-context-window-clamp",
          "outcome": "An HTTP lane never sends a request whose output ceiling cannot fit in the model's context window: an operator-declared context window lets the adapter clamp max_output_tokens to the room left after the previous turn's input, a turn that cannot fit a minimal output stops with a typed context-exhaustion cause before sending, and the status projection shows the latest turn's input tokens so the seat can see a context approaching its window.",
          "touchpoints": [
            "cmd/sworn",
            "internal/runtime",
            "internal/journal",
            "internal/gitx",
            "internal/driver",
            "internal/cockpit",
            "internal/observe",
            "internal/tui",
            "internal/skill",
            "internal/protocol",
            "tools/protocolgolden",
            "test/e2e",
            "docs/run.md",
            "docs/launch.md",
            "docs/policy/manager.md",
            "README.md"
          ]
        }
      ]
    }
  ]
}

```

# Goal

Make launch and failure legible to the Manager seat without a code or
journal read. This is the second release of ADR-0014 Track B (legibility).
It closes the launch gate of the v1.0.0 checklist (gate 4: #324 to #327) and
the failure and provider legibility issues filed by the seat during the
2026-09-22-worker-observability release: #330, #331, #336, #340 and #342,
with the advice-text half of #349.

# Authority and preparation

Brad is the Principal and the external approver. This revision is a
proposal, not an approval or a run receipt. Nothing here has been recorded,
approved or launched. The operator reference above is the proposed approval
identity and carries no authority by itself. Review these exact pinned
manifest bytes and the six contract files.

The inspection baseline is main at 6ea55d02, which contains the worker
observability release (#350), ADR-0015 (#345) and manager policy version 2
(#346). Prepare the named release target from that main before recording
this plan, and record its exact base and contract-tree identity then. No
existing release, track or live run is taken over by this plan.

# What was found

Launch. `sworn serve` discards every error behind one generic line, and the
cause is lost twice before that: cockpit's manifest admission turns every
runtime manifest refusal into INVALID_MANIFEST, and the operator config
loader returns one untyped error from about twenty sites, so a 0644 file and
a malformed one read the same. The canonical runtime manifest encoder exists
but is unexported, and nothing emits canonical bytes. `plan pin` only
prints. `plan record` passes flags straight to an exact-width object id
parser, and cmd/sworn resolves HEAD with its own ad hoc `git rev-parse`
outside gitx. `driver doctor --all` requires one profile of every production
family, but the registry build and the readiness report check two different
rosters, and the refusal names neither.

Failures. A failed host check is known in full to the engine (command, exit
code, outcome, rerun, bounded output) and handed to the implementer as
repair context, but the status projection says only "Host check fail ...
(exit 1)", and every executed check effect reads succeeded. Checks skipped
after a failure are not recorded anywhere. The bootstrap park advice is one
constant that tells the operator to revise the contract, including when an
assembly BLOCKED was an engine gap.

Providers. Doctor makes no live call; certify runs the whole agent loop. The
engine retries a failed try immediately, so a provider admission stall of
three minutes spends the whole identical-failure budget, and the provider's
reset time is dropped when adapter errors are normalized. The responses
adapter fixes max_output_tokens at the configured ceiling for the whole
dispatch, and no adapter knows its context window.

# What changes

One track, six slices, serial. Two parallel tracks were tried and are not
admissible: cmd/sworn imports every package the runtime and driver slices
change, so their derived scopes must include cmd/sworn, where the launch
slices live, and parallel admission refuses the overlap
(PARALLEL_TOUCH_CONFLICT, then UNDER_DERIVED_SCOPE). This release therefore
does not tick the v1.0.0 gate 3 parallel-tracks box.

The first two slices fix launch; the next four fix failure and provider
legibility.

S1-launch-refusals makes serve name the typed cause and the failing input,
gives the operator config loader typed refusals, resolves abbreviated object
ids through one sanitized gitx resolver, and adds docs/launch.md.
S2-canonical-authoring adds `sworn manifest canonical` for runtime manifests
and driver configs, judged by the same admission functions the engine uses,
`plan pin --write`, and refusal text that names the fixing command.

S3-host-check-failure-facts projects one bounded host check failure fact
(command, exit, rerun, not-run checks, excerpt), shows check outcomes beside
effect states, and corrects the assembly BLOCKED park advice to name manager
policy M9. S4-lane-live-probe adds `sworn driver probe`, one minimal live
request per named lane, exposed to the runtime as one function, and makes
doctor and certify --all name the missing families from one roster
declaration. S5-transient-provider-backoff waits with a bounded, journaled
backoff after a transient provider failure and starts the next try only
after that probe passes, parking with a typed provider-stall cause after 30
minutes; the try budget and identical-failure rule are unchanged.
S6-context-window-clamp adds an optional context_window_tokens to HTTP
profiles, clamps max_output_tokens from the previous turn's reported usage,
stops a turn that cannot fit with a typed economy code, and shows the latest
input tokens in the dispatch view.

# Deliberately not in this release

The routing of an assembly BLOCKED verdict (#349's main ask). BLOCKED hands
to the planner in the embedded protocol package, mirrored by the pinned
JavaScript reference; making it retryable through control is a protocol
change that re-pins the bundle, and is put to the Principal separately.
Until then manager policy M9 is the route, and S3 makes the park say so.

Durable journal location (#351). Handled first in the Manager skill and
launch procedure (journal in the ops home, archive at end of release); an
engine default is a later decision.

Probe at run admission for every lane, token-level usage accounting, and the
typed explain read.

# How this run is driven

By the Manager seat: /sworn-orchestrate under docs/policy/manager.md version
2, on a fresh run id, with the roles named planner, implementer, lead and
verifier. Every decision the seat takes is in the ops-home decision journal.
Anything the policy does not name is escalated to the Principal. It is a
candidate for the v1.0.0 gate 3 streak (a release with three or more slices,
and one with a non-Claude implementer) if it runs without hand operator
work.

# Proposal status

Proposed, not approved. No plan revision has been recorded, no approval
receipt exists and no run has been started. Approval is Brad's and is
separate from this document.