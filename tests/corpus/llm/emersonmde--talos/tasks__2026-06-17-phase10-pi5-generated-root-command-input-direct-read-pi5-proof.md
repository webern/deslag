# Phase 10 Pi 5 Generated-Root Command-Input Direct-Read Pi 5 Proof

Task id: phase10-pi5-generated-root-command-input-direct-read-pi5-proof-20260617

Status: accepted

Classification:
command-input-command0-prelude-blocked

Evidence level: lab-controller API, serialized Pi 5 hardware proof,
same-power-cycle TFTP, serial direct-read hardware output, serial write
evidence, direct-read proof validator wrapper, restore proof, and task-owned
JSON evidence.

## Goal

Run the serialized Pi 5 proof selected by the direct-read harness core: prove
generated-root command input through command-indexed /serial/read evidence
after the old /serial/observe cursor path saturated.

## Result

The proof did not accept generated-root command input. The candidate boot
archive was published, the Pi fetched the expected selected kernel and
initramfs before restore, and direct serial read retained the same-boot
firmware-initramfs valid-artifact source gate, ready command=0, and a visible
talos> prompt. The pre-write command 0 read was fresh and /serial/write
accepted the rootinfo payload.

The first failing invariant is command 0: the post-write direct-read window did
not retain rootinfo, source=firmware-initramfs reason=valid-artifact, or
dispatch command=0 status=handled responses=1. It retained only the tail of the
command 0 edit/ready output and later empty input timeout records. The command
1 manifest proof therefore cannot be accepted.

## Findings

- fixed: retained archive identity, selected-tree identity, stable
  same-power-cycle TFTP evidence, serial direct-read readiness, serial write
  responses, final pre-restore identity, and restore proof under
  hardwareTestLock.
- fixed: ran the accepted direct-read helper against the selected hardware
  evidence and retained a JSON validator-wrapper record for the rejection.
- blocked: command 0 direct-read evidence is insufficient; command input is not
  accepted.
- deferred: command-input closeout must reconcile this command0 prelude blocker
  before any further generated-root command-input hardware retry, persistence,
  storage, networking, SSH, Phase 11/12 expansion, or phase transition.
- rejected: generated-root command-input acceptance, persistence, writable
  filesystem, SD/USB/block storage, networking, SSH, Phase 11/12 expansion, and
  phase transition claims.

## Evidence

- Classification:
  tasks/evidence/2026-06-17-phase10-pi5-generated-root-command-input-direct-read-pi5-proof/classification.json.
- Evidence map:
  tasks/evidence/2026-06-17-phase10-pi5-generated-root-command-input-direct-read-pi5-proof/evidence-map.json.
- Selected run:
  tasks/evidence/2026-06-17-phase10-pi5-generated-root-command-input-direct-read-pi5-proof/candidate-direct-read-20260617T043803Z/.

## Validation

- serialized Pi 5 hardware proof under hardwareTestLock: accepted terminal
  blocker with command-input-command0-prelude-blocked.
- candidate identity via lab API: pass for selected archive publication and
  final pre-restore identity.
- fresh serial/direct-read evidence: pass for generated-root readiness and
  command 0 pre-write freshness; blocked for command 0 handled response.
- TFTP delta via GET /tftp/logs before restore: pass; stable same-cursor delta
  includes da591740/kernel_2712.img and da591740/initramfs_2712.
- post-run baseline restore proof: pass; restored tree hash
  a0452458391d0e398b7e17e0f068bb652235f666bf277d004e0e214626128d10.
- direct-read proof validator: rejected selected hardware evidence as expected
  for the command0 blocker.
- jq empty on task-owned JSON evidence: pass.
- git diff --check: pass.
- mdbook build: pass.
- git diff --cached --check: pass before commit.

## Acceptance Check

- Hardware lock acquisition/release, candidate identity, serial/direct-read
  freshness setup, TFTP delta, final identity, and restore evidence are
  recorded: satisfied.
- Successful command-input proof retains command 0 and command 1 direct-read
  evidence: not satisfied.
- Blocked proof classifies the first failing invariant: satisfied as
  command-input-command0-prelude-blocked.
- Known-good control and candidate rerun were not required because the selected
  run reached selected-tree identity, TFTP identity, direct-read readiness, and
  restore proof; the failure is the command0 prelude evidence contract.
- Rejected claims remain explicit: satisfied.

## Next Action

Promote
phase10-pi5-generated-root-command-input-direct-read-closeout-20260617 on the
next worker wake if dependencies remain satisfied, the repository is clean,
hardwareTestLock is unlocked/restored, and supervisorIntervention is inactive.
Do not start another generated-root command-input hardware retry, persistence,
storage, networking, SSH, Phase 11/12 expansion, or a phase transition from
this proof.
