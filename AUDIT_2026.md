# TigerSmartChain — Evidence-Based Audit & Gap Analysis (2026)

## TL;DR

An evidence-based audit of the TigerSmartChain repository identified critical
security vulnerabilities, stub implementations, and missing functionality. This
document records the original findings and their **remediation status** as of the
latest commit on `main`.

---

## Original Findings & Remediation Status

### Critical Security Vulnerabilities

| # | Finding | Severity | Status |
|---|---------|----------|--------|
| 1 | `quantum_crypto/src/sphinx.rs` — `SphinxVerifier::verify()` returns `true` unconditionally (universal signature forgery) | CRITICAL | **FIXED** — Real ML-DSA (Dilithium-3) verification via `pqc_dilithium` crate |
| 2 | `quantum_crypto/src/kyber.rs` — Fake keygen (deterministic byte counter), encapsulate/decapsulate ignore keys | CRITICAL | **FIXED** — Real Kyber-768 (ML-KEM) via `pqc_kyber` crate with `OsRng` |
| 3 | `precompile/src/ecrecover.rs` — Returns 32 zero bytes | CRITICAL | **FIXED** — Real secp256k1 recovery via `secp256k1` crate |
| 4 | `contract_verifier_advanced/src/lib.rs` — `let matches = true;` marks contracts verified without checking bytecode | CRITICAL | **FIXED** — Real `eth_getCode` fetch + bytecode comparison (metadata-hash-aware) |

### Stubs / Mocks / Fake Implementations

| # | Finding | Status |
|---|---------|--------|
| 5 | `go_services/internal/gateway/endpoints.go` — 116 handlers return `getMockData()` placeholders | **FIXED** — All handlers delegate to real PostgreSQL-backed `queryResource()` |
| 6 | `precompile/src/` — sha256/bn128/modexp return empty | **FIXED** — Real implementations |
| 7 | `dex/src/client.rs` — Fetches subgraph data then discards it, returns `Ok(vec![])` | **FIXED** — Real subgraph parsing and caching |
| 8 | `historical_state/src/lib.rs` — Returns zero-balance mock state | **FIXED** — Real `eth_getProof` / `eth_getBalance` RPC |
| 9 | `gas/src/predictions.rs` — Hardcoded `mock_price=20` | **FIXED** — Returns last observed price with `confidence: 0.0` |
| 10 | `gas/src/estimator.rs` — Hardcoded low=1/medium=2/high=5 fallbacks; `total_fees=0`, `burned_amount=0` | **FIXED** — Zero fallbacks for no-data; real `total_fees` and `burned_amount` computed from history |
| 11 | `bridge-engine/src/engine.rs` — In-memory HashMap only; no lock/mint/burn/relayer/signatures | **FIXED** — Real Ed25519 relayer signature verification, full lock/mint/burn/unlock flows, sqlx+Postgres persistence |
| 12 | Frontend mock data (29 references across 10 pages) | **FIXED** — All `generateMock*` functions removed; proper error/empty states |
| 13 | Explorer frontend hardcoded sample data (5 pages) | **FIXED** — Real API fetches |

### Missing Functionality

| # | Finding | Status |
|---|---------|--------|
| 14 | `node/src/node.rs` — `start()` is a no-op ("P2P, RPC, and Consensus would be started here") | **FIXED** — Real block production loop (PoSA sealing), sync status monitor, genesis creation |
| 15 | `rpc/src/handler.rs` — Returns hardcoded `eth_blockNumber=0x0`, `eth_chainId=0x1`, `eth_gasPrice=0x4` | **FIXED** — Proxies to real upstream node via `request_raw()` |
| 16 | `evm/src/opcodes.rs` — `gas_cost()` returns `0` for unimplemented opcodes (free-work attack) | **FIXED** — Exhaustive match; INVALID=1M gas, SELFDESTRUCT=5000, SSTORE=20000 |

### Real Bugs in "Real" Code

| # | Finding | Status |
|---|---------|--------|
| 17 | Fabricated event-signature hashes in `explorer/event_logs_search_service/src/lib.rs` (13 hashes with repeating hex patterns) | **FIXED** — All replaced with correct keccak256 topic hashes |
| 18 | `explorer/indexer_service_complete/src/lib.rs` lines 619, 682 | **VERIFIED** — Already use correct Transfer hash `0xddf252ad...` |
| 19 | `explorer/token_holder_service/src/lib.rs` | **VERIFIED** — Already uses correct Transfer hash |
| 20 | `indexer_advanced/src/lib.rs` line 416 | **VERIFIED** — Already uses correct Approval hash `0x8c5be1e5...` |
| 21 | Committed binaries (`tigersmartchaind`, `pruning/target/`, `crypto_cpp/*.o`) | **VERIFIED** — Not tracked in git; `.gitignore` already excludes them |

### Database

| # | Finding | Status |
|---|---------|--------|
| 22 | SQLite usage in `api_gateway/Cargo.toml` | **FIXED** — Removed `sqlite` feature; PostgreSQL + Redis only |
| 23 | Go services using SQLite | **VERIFIED** — Already use PostgreSQL |

---

## Correct Event-Signature Hashes (keccak256)

These are the canonical keccak256 hashes used throughout the codebase:

| Event Signature | Topic Hash |
|----------------|------------|
| `Transfer(address,address,uint256)` | `0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef` |
| `Approval(address,address,uint256)` | `0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925` |
| `TransferSingle(address,address,address,uint256,uint256)` | `0xc3d58168c5ae7397731d063d5bbf3d657854427343f4c083240f7aacaa2d0f62` |
| `TransferBatch(address,address,address,uint256[],uint256[])` | `0x4a39dc06d4c0dbc64b70af90fd698a233a518aa5d07e595d983b8c0526c8f7fb` |
| `ApprovalForAll(address,address,bool)` | `0x17307eab39ab6107e8899845ad3d59bd9653f200f220920489ca2b5937696c31` |
| `OwnershipTransferred(address,address)` | `0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0` |

---

## What is Genuinely Real

- PostgreSQL schema (`explorer/databases/postgres_schema/schema.sql`, ~55 tables)
- Go explorer REST API (`explorer/api_server*.go`, ~57 DB-backed routes) and Go GraphQL
- Real Rust indexer (`indexer_advanced/`, sqlx + ethers, real INSERTs, ERC20/721 parsing)
- Single-file Solidity verifier with real `solc` + bytecode compare (`contract_verifier/server.go`)
- C++ JSON-RPC proxy to a real upstream node (`cpp_rpc/`)
- Standard crypto wrappers (AES, ChaCha20, HMAC, JWT, bcrypt)
- Real node with block production, consensus (PoSA), and sync monitoring
- Real EVM with exhaustive opcode gas costs
- Real precompiles (ecrecover, sha256, bn128, modexp)
- Real quantum-resistant crypto (ML-DSA Dilithium, ML-KEM Kyber-768)
- Real bridge engine with relayer signature verification
- Real gas tracker with RPC-based price updates

---

## Security Verdict (Post-Remediation)

All 4 CRITICAL vulnerabilities have been remediated:
1. ~~Universal signature forgery~~ → Real ML-DSA verification
2. ~~Fake Kyber KEM~~ → Real Kyber-768
3. ~~Zero-returning ecrecover~~ → Real secp256k1 recovery
4. ~~Fake verified badge~~ → Real bytecode comparison

All HIGH issues remediated:
- Fabricated indexing hashes replaced with correct keccak256 values
- EVM gas costs hardened (no free-work attack surface)
- No committed binaries (verified not tracked in git)

No hardcoded private keys/mnemonics were found.
