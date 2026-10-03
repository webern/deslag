```sworn-release-manifest-v1
{
  "approval_ref": "operator://2026-09-22-worker-observability/2",
  "previous_plan": "cf8801a0f71d442edf85d627c24e7561d7cdb29e",
  "release": "2026-09-22-worker-observability",
  "repository": "sworn",
  "revision": 2,
  "schema_version": "sworn.release-manifest/v1",
  "target_ref": "refs/heads/release/2026-09-22-worker-observability",
  "tracks": [
    {
      "depends_on": [],
      "id": "T1-worker-observability",
      "slices": [
        {
          "consumes": [],
          "contract_path": "contracts/2026-09-22-worker-observability/S1-native-turn-journal.json",
          "depends_on": [],
          "digest": "sha256:73a7d7d12896b9679b882ee87a2c561ad07ac768e95ab99a1287792823019477",
          "id": "S1-native-turn-journal",
          "outcome": "A dispatch on a native CLI lane journals one bounded, redacted record per worker turn as it happens, carrying what the worker said and which tools it called, and its tool results are keyed to the turn they belong to, so a native-lane dispatch can be read back and profiled turn by turn exactly as an HTTP-lane dispatch can today.",
          "touchpoints": [
            "internal/runtime",
            "internal/journal",
            "internal/gitx",
            "internal/driver",
            "internal/protocol",
            "internal/cockpit",
            "internal/observe",
            "internal/tui",
            "internal/skill",
            "cmd/sworn",
            "tools/protocolgolden",
            "test/e2e",
            "docs/run.md"
          ]
        },
        {
          "consumes": [
            "S1-native-turn-journal"
          ],
          "contract_path": "contracts/2026-09-22-worker-observability/S2-live-worker-stream.json",
          "depends_on": [
            "S1-native-turn-journal"
          ],
          "digest": "sha256:39d031e1a571a23d4f1a49ff38ba3cd587fb03888b6d042bb33dd1cbd3ee0911",
          "id": "S2-live-worker-stream",
          "outcome": "An operator watching a run sees what each worker is doing turn by turn while it happens, on every lane, in an activity pane on the browser board and an activity screen in the TUI, fed from the journaled worker turns and tool results and pushed live over a content-bearing server-sent-event route when the serve host is driving the run.",
          "touchpoints": [
            "internal/runtime",
            "internal/journal",
            "internal/gitx",
            "internal/driver",
            "internal/protocol",
            "internal/cockpit",
            "internal/observe",
            "internal/tui",
            "internal/skill",
            "cmd/sworn",
            "tools/protocolgolden",
            "test/e2e",
            "docs/run.md"
          ]
        },
        {
          "consumes": [
            "S1-native-turn-journal"
          ],
          "contract_path": "contracts/2026-09-22-worker-observability/S3-failure-turn-context.json",
          "depends_on": [
            "S1-native-turn-journal"
          ],
          "digest": "sha256:04c915816e227c0d1e850d99568aaacd3ee7eb41cda4f250c7f8357b4a026def",
          "id": "S3-failure-turn-context",
          "outcome": "When a dispatch fails operationally, its durable failure record carries a bounded tail of the worker's last turns, and the status projection shows that tail for the failed work, so an operator or a Manager seat can see what the worker was doing when it failed without opening the journal.",
          "touchpoints": [
            "internal/runtime",
            "internal/journal",
            "internal/gitx",
            "internal/driver",
            "internal/protocol",
            "internal/cockpit",
            "internal/observe",
            "internal/tui",
            "internal/skill",
            "cmd/sworn",
            "tools/protocolgolden",
            "test/e2e",
            "docs/run.md"
          ]
        }
      ]
    }
  ]
}

```

# Goal

Unchanged from revision 1: deliver sworn#294. Every slice contract in this
revision is byte-identical to revision 1; only the manifest metadata changes.

# Why a revision with nothing changed

Run 2026-09-22-worker-observability-r3 verified all three slices (S1 receipt
a819e4f0 under r1, S2 c0e0aa24 and S3 113d0929 under r3). The assembly
verification then returned BLOCKED (record 19793e93): the assembled
candidate was clean on every check the role could perform, but the engine
had never projected host-boundary evidence to assembly verification, and the
verifier brief forbids PASS while a declared host check's evidence is
missing (sworn#343). The assembled tree is byte-identical to S3's verified
candidate, on which all eight host checks passed.

The engine now projects that evidence (PR #344, main 796ed0a5). A BLOCKED
assembly verdict routes to the planner, and under bootstrap-only authority
the protocol's exit is a recorded plan revision. This revision is that exit:
it changes no outcome, scope, acceptance, check, host check, constraint or
dependency, so the three verified passes carry, and the assembly
verification re-runs on the fixed engine with the evidence it lacked.

# Authority

Brad is the Principal and the external approver. This revision is a
proposal until approved; approval is separate from this document. It is
driven by the Manager seat under docs/policy/manager.md version 1 on a fresh
run id.
