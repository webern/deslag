# Packet Policy

**Kind:** benchmark-engine
**Standard:** Production-Grade Builder v1.0.0

This deliverable adheres to the production-grade packet policy.
See the root policy at `production-grade-builder/PACKET_POLICY.md` for full details.

## Requirements

1. All required artifacts must be present.
2. `manifest.json` must be valid with required keys.
3. `scores/quality_score.json` must report pass=true.
4. `tools/quality_gate.py` must exit 0.
5. No secrets or credentials in any file.
6. README must contain verification instructions.
