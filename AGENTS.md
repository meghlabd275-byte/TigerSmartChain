# TigerSmartChain — Agent Memory

## Toolchain status (verified this session)
- Working dir: `/workspace/project/TigerSmartChain`
- g++ 14.2.0 present. Rust NOT installed by default — installed via rustup (rustc 1.97.1). Go NOT installed by default.
- Network: crates.io returns 403 to HEAD; rustup installer downloads fine; npm registry reachable.
- `source $HOME/.cargo/env` to enable cargo/rustc.
- No root Cargo workspace; 147 independent `Cargo.toml` crates (each builds standalone).

## Verified audit findings (AUDIT_2026.md is ACCURATE — confirmed in source)
Security CRITICAL:
- `precompile/src/ecrecover.rs`: returns `vec![0u8; 32]` (zero address). BREAKS signature recovery.
- `precompile/src/sha256.rs`: returns `vec![0u8; 32]`. `bn128.rs`/`modexp.rs` return `vec![]`.
- `quantum_crypto/src/sphinx.rs` and `kyber.rs`: ALREADY FIXED (commit 67dfc5f). sphinx now uses
  real ML-DSA/Dilithium-3 via `pqc_dilithium` (verify() returns false for forged/tampered sigs);
  kyber now uses real ML-KEM via `pqc_kyber` with OsRng. `cargo check` + 17 tests pass.
- `contract_verifier_advanced/src/lib.rs:~129`: `let matches = true; // Would compare with chain` -> fake verified badge.

HIGH (confirmed):
- `indexer_advanced/src/lib.rs:416`: `ERC20_APPROVAL_TOPIC = "...2e2e2e2e2e2e2"` FABRICATED.
  Real keccak256("Approval(address,address,uint256)") = `0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925` (verified by my own keccak impl).
  Transfer topic `0xddf252ad...b3ef` is CORRECT.
- `explorer/event_logs_search_service/src/lib.rs:166,185,186`: more fabricated Approval/Stake/Unstake topics.
- `evm/src/executor.rs`: only ~14 opcodes implemented; every other (CALL/CREATE/STATICCALL/DELEGATECALL/JUMP/JUMPI/SHA3/LOG*/RETURN/REVERT) silently no-ops via `_ => {}`. output always empty, logs always empty.
- `evm/src/opcodes.rs gas_cost()`: `_ => 0` for unimplemented opcodes -> free-work attack.
- `rpc/src/handler.rs:717`: hardcoded `eth_blockNumber=0x0`, `eth_chainId=0x1` (chain ID should be 6666), `eth_gasPrice=0x4`; everything else "Method not found".
- `node/src/node.rs:147 start()`: no-op ("P2P, RPC, and Consensus would be started here").
- `go_services/internal/gateway/endpoints.go`: 118 `getMockData` placeholders across 191 handlers.
- `dex/src/client.rs`: returns `Ok(vec![])` at lines 180,255,313,353.
- `gas/src/tracker.rs:67`: `mock_price=20`.
- `historical_state/src/lib.rs:~191`: returns zero-balance mock AccountState.

## Fixes APPLIED this session (security — real crypto, no stubs)
### precompile crate — FIXED (15/15 tests pass)
- `ecrecover.rs`: real secp256k1 ECDSA recovery via `secp256k1` crate (recovery feature) + Keccak256 address derivation. No more zero bytes.
- `sha256.rs`: real SHA-256 via `sha2` crate. NIST test vectors pass.
- `modexp.rs`: real modular exponentiation via `num-bigint` BigInt.
- `bn128.rs`: real BN254 via `ark-bn254` (ecadd, ecmul, ecpairing) using AffineRepr/Pairing/BigInteger.
- Deps added: sha2 0.10, sha3 0.10, secp256k1 0.29 (recovery), num-bigint, ark-bn254 0.5, ark-ec 0.5, ark-ff 0.5.

### quantum_crypto crate — FIXED (17/17 tests pass incl. tamper-rejection regression)
- `sphinx.rs`: replaced always-true verifier with real ML-DSA (Dilithium-3) via `pqc_dilithium`. Sign/verify round-trip; tampered sig, wrong message, wrong verifier all REJECTED.
- `kyber.rs`: real ML-KEM (Kyber-768) via `pqc_kyber`. Real keygen/encapsulate/decapsulate; tampered ciphertext rejected.
- `hash.rs`: real SHAKE-256 (sha3), RFC 2104 HMAC-SHA-512, RFC 8018 PBKDF2-HMAC-SHA-256. Replaced fake XOR/cycle Shake256 and naive PBKDF2.
- `merkle.rs`: fixed proof verification (`&**sibling` deref).
- `lib.rs` manager: sign/verify/key_exchange now use real Dilithium/Kyber. `QuantumSignature` carries the original message (Dilithium verify needs it). Stats wrapped in RwLock for &self. Live Dilithium keypairs retained in `dilithium_keys` map.
- Deps added: pqc_kyber 0.7 (kyber768), pqc_dilithium 0.2, sha2 0.10, parking_lot 0.12, tracing 0.1, hex 0.4, rand_core 0.6.

### Fabricated event-signature hashes — FIXED (11 replacements across 4 files)
Replaced fabricated keccak256 topics with verified real values (computed via pyca keccak256):
- Transfer `0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef` (was fake `...9529521f1c5d5ba481f6d2a9c92955c5be`).
- Approval `0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925` (was fake `...f5bdaa510684b1e5c059b1...` and `...b9314c4bde82b4e8e6d2e2e2e2e2e2e2`).
- TransferSingle `0xc3d58168c5ae7397731d063d5bbf3d657854427343f4c083240f7aacaa2d0f62` (was fake `...17307e6d6c086e92...1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1`).
- Stake `0xe678d5bdcf670ef07ff1a819fad381a3a917e3d6c70d4d1c064b52bb7506c745`, Unstake `0x2458986e35eb8d33d29c9a7dbe095b7957a08ca23784d5e3636e3a03f99058bf`, RewardPaid `0xe2403640ba68fed3a2f88b7557551d1993f84b99bb10ff833f0cf8db0c5e0486`.
- Also fixed `hex::decoded` typo → `hex::decode` in token_approval_service.
- Files: indexer_advanced/src/lib.rs, explorer/indexer_service_complete/src/lib.rs, explorer/event_logs_search_service/src/lib.rs, explorer/token_approval_service/src/lib.rs.

## Remaining (NOT yet fixed — see task tracker)
- EVM: only ~14/170 opcodes; gas_cost=0 default. executor.rs `_ => {}`.
- RPC handler hardcoded values; missing namespaces.
- node.rs start() no-op.
- 116 gateway getMockData handlers.
- bridge-engine in-memory only. — FIXED 2026-08-16: real lock/mint/burn/unlock flows +
  real Ed25519 (ed25519-dalek) relayer signature verification, deterministic Keccak256 event
  ids, sqlx postgres persistence (in-memory fallback), idempotency. 6 integration tests pass.
  See `bridge-engine/src/engine.rs` + `bridge-engine/tests/bridge_flows.rs`.
- dex/gas/historical_state/contract_verifier_advanced stubs.
- committed binaries; SQLite→PostgreSQL+Redis.

## bridge-engine build/test notes (env quirk)
- `cargo check` passes in `bridge-engine/` without extra setup.
- `cargo test`/`cargo build` (linking a bin/test) needs OpenSSL dev libs (`-lssl -lcrypto`);
  the sandbox has only `libssl.so.3`/`libcrypto.so.3`, not the `-lssl`/`-lcrypto` dev symlinks,
  and no root to install `libssl-dev`. Workaround without root:
  `mkdir -p /tmp/ossllibs && ln -sf /usr/lib/x86_64-linux-gnu/libssl.so.3 /tmp/ossllibs/libssl.so
   && ln -sf /usr/lib/x86_64-linux-gnu/libcrypto.so.3 /tmp/ossllibs/libcrypto.so`
  then `LIBRARY_PATH=/tmp/ossllibs cargo test`.

Committed build artifacts (should be gitignored/removed):
- `tigersmartchaind` (5MB ELF at repo root), `pruning/target/` (257 files), `crypto_cpp/*.o` + `libtiger_crypto.a`.

## Real keccak256 canonical event topics (verified)
- Transfer(address,address,uint256): 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef
- Approval(address,address,uint256): 0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925
- Transfer(address,uint256) [ERC721]: 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef (same as ERC20)
- OwnershipTransferred(address,address): 0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0

## Chain config
- chain_id should be 6666 (audit: eth_chainId returns 0x1 but should be 6666).

## Style / conventions
- Rust crates are standalone (no workspace). Each has own Cargo.toml + [profile.release] lto=true.
- Frontend: Next.js app router under `frontend/src/app/`, theme switching via `frontend/src/app/providers.tsx` + `frontend/src/lib/store.ts`.
- DB: PostgreSQL schema at `explorer/databases/postgres_schema/schema.sql`. User wants SQLite removed in favor of PostgreSQL + Redis.

## contract_verifier_advanced (already fixed as of commit b79c2ef)
- `verify()` previously had `let matches = true;` stub (line ~129). Now does REAL comparison:
  `fetch_onchain_bytecode()` -> `eth_getCode` via `reqwest::blocking` (RPC_HTTP_URL env or `new_with_rpc()`), then `bytecode_matches()` strips trailing CBOR metadata (`0xa1 0x65` prefix) before comparing runtime bytecode. Returns false when RPC unavailable or bytecode differs.
- `compile_sources()` invokes real `solc --combined-json bin,bin-runtime,abi` (no longer a hardcoded stub). Note: requires solc installed on PATH to actually produce bytecode.
- Cargo deps include `reqwest` (json+blocking) and `hex`. `cargo check` passes (only unused-variable warnings elsewhere).

## dex + historical_state (already fixed as of commit 6df8deb)
- `dex/src/client.rs` previously discarded fetched subgraph data with `Ok(vec![])` in `get_pairs`, `search_pairs`, `get_top_tokens`, `get_swaps` (pre-fix lines 180/255/313/353). Now all return real parsed data via `parse_pairs`/`parse_tokens`/`parse_swaps`; `get_analytics` aggregates factory-level stats. No `Ok(vec![])` remains.
- `historical_state/src/lib.rs` previously returned a zero-balance mock `AccountState` around old line 191 ("// For now, create a mock state", balance "0x0"). `HistoricalIndexer` now uses a real `reqwest::blocking` client + `rpc()` helper issuing `eth_getProof` / `eth_getBalance` / `eth_getStorageAt` / `eth_getCode` / `eth_getBlockByNumber` against `rpc_url` (errors if empty). The remaining `unwrap_or("0x0")`/`"0x000..0"` strings are fallback defaults for missing RPC fields, not mock data.
- Both crates: `cargo check` passes (only unused-variable/import + non-snake-case warnings).

## Build/test approach
- Compile per-crate: `cd <crate> && cargo build` (after `source $HOME/.cargo/env`).
- For real crypto, add crates: sha2, sha3, secp256k1, ark-bn254/ark-ec, num-bigint, etc.
