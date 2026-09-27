# Public update draft - Milestone 1 complete, 2026-06-28

Target: GitHub Discussions / Announcements for `redclawanon-rgb/decentralized-forge`.

Status: posted to GitHub Discussions on 2026-06-28 after local wording review.

## Title

Decentralized Forge Milestone 1: reproducible static prototype complete

## Body

This is a research/prototype status update for Decentralized Forge, a GitHub-class decentralized forge experiment.

Milestone 1 is now complete as a reproducible, evidence-scoped static prototype.

What is included:

- a documented project registry schema;
- two local registry fixtures: `example-project` and `portable-lab`;
- a reusable local CLI: `scripts/forge_registry.py`;
- deterministic static HTML outputs and machine-readable summaries under `output/`;
- a live evidence index that separates local fixtures, selected-relay Nostr readback, disposable Radicle smoke evidence, and non-claims;
- GitHub Actions CI that runs the local verification suite on `main` and pull requests.

Verification:

- Local verification command: `npm run verify:local`
- Latest Milestone 1 completion commit: `1e20e9fb107f33bfa714d86dabfd0fae9ba3f004`
- Passing GitHub Actions run: https://github.com/redclawanon-rgb/decentralized-forge/actions/runs/28335959425

Evidence boundaries:

- **Nostr:** one selected-relay repository-announcement readback is recorded. This is not proof of durability, global propagation, identity trust, security, full NIP-34 compatibility, or production readiness.
- **Radicle:** one disposable local/private replay and one bounded disposable public seed/remote-clone smoke are recorded. This is not proof of persistent seed operation, broad Radicle network availability, durability, censorship resistance, security, identity trust, full Radicle compatibility, or production readiness.
- **Artifacts:** local SHA-256/CID metadata and local CAR/CID readback are recorded. This is not IPFS daemon add/fetch, public gateway availability, pinning, Filecoin/Arweave durability, wallet-backed storage, or paid storage evidence.
- **CI/provenance:** GitHub Actions now runs local validation. The registry provenance fields remain synthetic fixtures; this is not real signing, Sigstore, in-toto, Rekor, SLSA compliance, or a supply-chain security guarantee.

Next gated lanes:

1. Live IPFS add/fetch/gateway checks for the local CAR/CID fixture.
2. Repeated or broader Radicle public-network checks using disposable state.
3. Nostr issue/patch readback using disposable project-scoped keys.
4. Optional disposable or keyless signing/provenance evidence.

Those lanes still need explicit target/approval before execution. The project still forbids spending money, paid pinning/storage, wallets, production/private personal keys, direct person outreach, persistent public seed operation, or stronger durability/censorship/security/production claims without separate evidence and approval.

Repo: https://github.com/redclawanon-rgb/decentralized-forge

If you review one thing, review whether the claim boundaries are clear enough for readers to distinguish local fixtures, narrow live evidence, and unverified future claims.

## Posting result

- Posted URL: https://github.com/redclawanon-rgb/decentralized-forge/discussions/7
- Posted surface: GitHub Discussions / Announcements
- Boundary: public project channel only; no direct outreach, paid service, production/private key, live storage action, new protocol action, or unsupported durability/censorship-resistance/security/SLSA/production-readiness claim.
