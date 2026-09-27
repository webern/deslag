---
created: 2026-04-21T07:15:33.399Z
title: Resolve proxy implementation ABI in Fetcher
area: general
files:
  - app/controllers/contracts_controller.rb:6-22
  - app/services/etherscan_client.rb
  - app/services/chain_reader/
  - app/services/contract_document/classifier.rb
---

## Problem

Proxy contracts (EIP-1967 / transparent / UUPS) are indexed as their proxy shell, not as their underlying implementation. Live reproducer: AAVE token on mainnet (`0x7Fc66500c84A76Ad7e9c93437bFc5Ac33E2DDaE9`) returns via `mcp__smarts__get_contract_info`:

- `name: "InitializableAdminUpgradeabilityProxy"`
- `classification: nil`
- `view_function_count: 0`
- `write_function_count: 7` (just proxy admin methods)

Expected: transparent pass-through as ERC-20 with the implementation's ~20 view/write functions and `erc20` classification, so MCP and the doc page are useful for proxied tokens. This matters broadly — most major protocol tokens (AAVE, stablecoins, many LP/vault tokens) are behind proxies, and Aave V3 itself is proxy-heavy.

Current code path has no proxy handling:

- `ContractsController#show` calls `EtherscanClient#fetch_contract_info` directly and saves the response as-is (`app/controllers/contracts_controller.rb:6-22`).
- `app/services/chain_reader/` contains only `base.rb`, `multicall3_client.rb`, `single_caller.rb`, `view_caller.rb` — no `ProxyResolver`.
- `app/services/contract_document/` has `classifier.rb` and `ai_enricher.rb` — no `Fetcher`.

CLAUDE.md's architecture section specifies `ChainReader::ProxyResolver` and `ContractDocument::Fetcher`, both of which are still unbuilt.

## Solution

TBD. Rough shape:

1. Build `ChainReader::ProxyResolver` — read EIP-1967 implementation slot (`0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc`), plus admin slot and beacon slot; handle transparent proxy's non-standard layouts as fallback.
2. Extract the contract-loading logic out of `ContractsController` into `ContractDocument::Fetcher.call(chain:, address:)` per CLAUDE.md architecture. Fetcher detects proxy via Etherscan metadata (`Proxy: "1"`, `Implementation: "0x…"`) or via ProxyResolver's slot read, then fetches the implementation's ABI/source and stores it as the effective ABI on the proxy's Contract record (while preserving proxy address as the canonical identity).
3. Data model decision: one Contract row for the proxy with merged ABI, OR separate rows for proxy + impl with a `implementation_id` link. Former is simpler for MVP; latter is cleaner if impl changes (upgrades). Probably start with the simple option and add `implementation_address` column for traceability.
4. Classifier runs against the resolved implementation ABI, so AAVE-as-proxy classifies as `erc20`.
5. Watch for impl upgrades — cache TTL or event subscription on the proxy's `Upgraded(address)` event (out of scope for MVP, but note it).
