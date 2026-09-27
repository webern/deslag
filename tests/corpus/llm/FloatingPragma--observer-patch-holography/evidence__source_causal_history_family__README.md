# Source causal-history family receipts

This package mirrors the simulator's 24-to-384-event source-history custody
receipt, `source_causal_history_family_receipt.json`, together with its
theorem-level publication projection and the checker
`verify_source_causal_history_family_projection.py`. Its causal readout is
superseded by the causal-poset package in `../source_net_causal_poset/`; the
package stays as the custody input that the particle-side source projections
pin.

## Bound causal-order report

The receipt's `canonical_four_round_source_binding` block binds the
source-derived causal-order report at
`data/causal_order/source_derived_causal_order_receipt.json` through the
report's canonical self-hash
`sha256:87caa4561af5601149d725d7d2d652ded7f582c7edf07025cdc584fdc37f7e87`,
which the report records in its own `report_sha256` field. That report is
vendored here as `source_derived_causal_order_receipt.json`. Its file digest is

```
sha256 50ff410dc1f11ffd52ffb8ced8a85af7e5ae7dc62cdda2e252ec2fc2d101fe4e
```

Source repository: <https://github.com/muellerberndt/oph-physics-sim>, commit
`6080f3a4045b7cbd821476a0a8fa0a6b57eeeeab`, path
`data/causal_order/source_derived_causal_order_receipt.json`. The two mirrored
receipts above come from the same simulator directory, `data/causal_order/`.
