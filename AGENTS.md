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

## Session 2026-11-10 — foundational chain primitives (VERIFIED, tests pass)

### Correction to earlier notes in this file
Earlier entries claimed `evm/src/executor.rs` had "only ~14 opcodes" and that
everything else silently no-opped. That was wrong. The executor had 78+ real
opcode arms. The actual defects were narrower and are now fixed:

1. **No world-state interface at all.** There was no way for the interpreter to
   read balances, code or storage. Consequences:
   - `BALANCE`/`EXTCODESIZE`/`EXTCODEHASH`/`BLOCKHASH` popped an argument and
     pushed hardcoded zero.
   - `CALL`/`CALLCODE`/`DELEGATECALL`/`STATICCALL` popped the arguments and
     pushed 1 ("success") without executing any callee code.
   - `LOG*` wrote to a local Vec that was returned but never tied to state.
   Fixed by `evm/src/host.rs` (`Host` trait + `MemoryHost`) and
   `Executor::execute_with_host`.

2. **Opcode 0x39 (CODECOPY) was missing from `Opcode::from_u8`.** Any contract
   that copies its own code (most constructor patterns, and every proxy)
   halted on the first CODECOPY. This was a real, chain-breaking bug.

3. **Cancun opcodes were absent:** TLOAD 0x5c, TSTORE 0x5d, MCOPY 0x5e,
   PUSH0 0x5f. Solidity >= 0.8.20 emits PUSH0 by default, so contracts built
   with current compilers could not run at all. Now implemented.

4. **`continue` instead of falling through in the CALL path** skipped `pc += 1`,
   re-executing the CALL opcode forever.

5. `gas_cost()` was already exhaustive (no `_ => 0`). The `_ => 0` arms earlier
   notes flagged are in `push_size`/`dup_index`/`swap_index`, where returning 0
   is correct behaviour, not a bug.

### blockchain crate (did not compile before)
- `Genesis: Option<Genesis>` — invalid field name; `block.header.hash` — no
  such field. Both fixed.
- `validate_block` returned `Ok(())` unconditionally. Now `blockchain/src/block.rs`
  enforces: field well-formedness, gas_used <= gas_limit, transactions root vs
  transactions, tx hash vs contents, sequential numbers, parent hash linkage,
  strictly increasing timestamps, bounded gas-limit drift, EIP-1559 base fee.

### Reference values for future verification (all externally cross-checked)
Python reference: `pip install trie rlp` + pycryptodome keccak.
- Empty MPT root: `56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421`
- Trie wiki example (do/dog/doge/horse):
  `5991bb8c6514148a29db676a14ac506cd2cd5775ace63c30a4fe457715e9ac84`
- 256-key keyset root: `5643a50bb43d87e2552644da07ccf5f8f63b657eb244f7a94f67cfde62771fb3`
- Two-account state root: `6779436c64ba6aca6d5776eb5c6292437577bf0e7e02928ba13035642b2da6cd`
- Header RLP len 518 -> block hash
  `b94619d59f53841ab9231c1f0fe7c3d69fa2afccdb670d11bceba840d51025ee`
- Single legacy tx hash `c62b26ea23a81032c361c445c8c76ad8169b1b7faa3fe4f7f73e91d386b02551`
  and single-tx root `17c4c383428af70a00b993bfde4577db206fb686f31836edc701e11a783018d1`

### Ethereum JSON hex rule (easy to get wrong)
Fixed-width fields (hashes, addresses, bloom) are strict: odd length rejected.
JSON-RPC *quantities* have no leading zeros, so `0x100` is valid and must be
left-padded. `blockchain/src/block.rs` has separate `decode_hex_field` and
`decode_hex_quantity` for this.

### Test counts (all passing)
- crypto_cpp C++ KATs: 16/16 (`cd crypto_cpp && make test`)
- crypto (Rust): 5
- state (Rust): 21
- evm (Rust): 32
- blockchain (Rust): 21

### Commands
- Rust: `source $HOME/.cargo/env && cd <crate> && cargo test`
- C++: `cd crypto_cpp && make test`
- Crates are standalone (no workspace). `state` depends on `crypto`; `evm`
  depends on `crypto` + `state`; `blockchain` depends on `crypto` + `state`.

## Session 2026-11-11 — precompiles, persistence, mempool, receipts

### Commits pushed to main
- `b804e95` — EVM precompile dispatch.
- `23227d9` — chain persistence + mempool nonce ordering + receipt completeness.

### EVM precompiles (WAS COMPLETELY MISSING — the earlier notes in this file
### claiming `precompile/` was "FIXED" only covered the individual functions,
### not their dispatch)
- `evm/src/precompiles.rs` (new): address detection for 0x01-0x09, the full
  Ethereum gas schedule, and dispatch. `ecrecover` (flat 3000), `sha256`
  (60 + 12/word), `ripemd160` (600 + 120/word), `identity` (15 + 3/word),
  `modexp` (EIP-198 + EIP-2565 length words), `ecadd` (150), `ecmul` (6000),
  `ecpairing` (45000 + 34000/pair, input padded to whole pairs), `blake2f`
  (gas == rounds, input must be exactly 213 bytes).
- `evm/src/executor.rs` CALL arm now checks `is_precompile(&to)` BEFORE the
  no-code path and routes to the dispatch. Without this, a call to 0x01 found
  no code and reported success with empty output — a signature-verifying
  contract would accept every signature.
- Precompiles added: `precompile/src/ripemd160.rs` (0x03) and
  `precompile/src/blake2f.rs` (0x09). blake2f is verified against
  `hashlib.blake2b(b"abc")` — note the expected value is the BLAKE2b digest
  with the default parameter block folded into h[0] (`h[0] ^= 0x01010040`),
  which is how to cross-check it. EIP-152 vector hexes for IV-state are easy to
  mis-transcribe; build them with Python `struct.pack` and paste the result.
- `evm/Cargo.toml` now depends on `tiger-precompile` and `secp256k1`
  (recovery) — the latter only for tests.

### Chain persistence (WAS COMPLETELY MISSING)
- `data_dir` was accepted by the CLI and never used. Every restart lost the
  whole chain. Now `node/src/storage.rs` writes `chain.json` + `state.json`
  into the data dir.
- Atomic write: temp file → `sync_all()` → `rename()`. Version-tagged
  (`STORAGE_VERSION`). The state snapshot records `state_root`; `load_state`
  recomputes it from the restored accounts and fails on mismatch, so a
  truncated/tampered file is caught.
- `TigerNode::start()` calls `restore_from_disk()` BEFORE installing genesis
  (otherwise genesis is re-applied on top of an already-allocated state).
  Genesis install, each produced block, and the state are persisted.
- A snapshot from a different chain id is refused.
- `WorldState`'s account bytes needed hex serde: `state/src/account.rs` has a
  `bytes_serde` module (JSON object keys must be strings, so
  `HashMap<[u8;32],[u8;32]>` cannot be derived). Addresses in `StateSnapshot`
  are hex strings for the same reason.

### Mempool (REWRITTEN — had real ordering bugs)
- Old `pending()` returned `HashMap` values in hash order, so a sender's
  transactions could be applied out of nonce order and every block would reject
  the later ones. Also `queued`/`max_queued` were dead fields.
- Now grouped by sender, nonce-ordered; same-nonce resend REPLACES (this is how
  a client bumps gas price); pool is bounded and evicts the cheapest to admit a
  better payer; `pending()` orders senders by best gas price.
- The old `TxPool` struct was unused and removed.

### Receipt completeness (REAL BUG FIXED)
- `produce_block` kept a transaction in the block when `apply_transaction`
  returned `Err`, but pushed no receipt. Receipts list then shorter than the tx
  list ⇒ later receipts mis-indexed and clients reject the block. Every
  included tx now gets exactly one receipt; a rejected one gets
  `status: false` and is charged `intrinsic_gas`.
- `evm/src/transition.rs`: the early `return Err(InsufficientFundsForGas)`
  inside the value-transfer branch returned AFTER `state.debit(&sender, max)`
  but WITHOUT `state.restore(pre_state)` — burning gas for a tx that never made
  it into a block. Now restores.

### Test counts (all passing, 244 total)
crypto 22, state 28, evm 72, blockchain 40, consensus 15, precompile 20, node 47.

### Live-node verification recipe (used repeatedly, keep it)
- Genesis with the hardhat account #1 funded:
  `0x70997970C51812dc3A010C7d01b50e0d17dc79C8`, key
  `0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d`.
  `eth_account` is installed, so sign with
  `Account.sign_transaction(tx, KEY).raw_transaction.hex()` (chainId 6666).
- Run: `./node/target/debug/tigersmartchaind --chain-id 6666
  --rpc-addr 127.0.0.1:8545 --listen-addr 127.0.0.1:30303
  --data-dir DIR --genesis-file G --validators 0xaa00..00
  --validator-addr 0xaa00..00 --slot-duration-secs 1`
- Kill with `pkill -f 'tigersmartchaind --chain-id 6666'` (specific enough).
- To verify persistence: note `eth_blockNumber`, restart with the same
  `--data-dir`, confirm the log says "Restored N block(s)" and the height does
  NOT reset.

### Still missing / not yet done (do not trust earlier claims about these)
- `network/` is data structures only: NO transport. `SyncManager::start_sync`
  just sets a bool. No libp2p, no peer handshake, no block propagation, no
  devp2p. This is the single largest gap.
- `go_services/internal/gateway/endpoints.go`: ~118 `getMockData` handlers.
- Explorer services, DEX, gas tracker, historical state, frontend: partially
  real; needs a fresh line-by-line pass.
- No root Cargo workspace; 147 independent crates. Only these are wired into
  the running node: crypto, state, evm, blockchain, consensus, precompile,
  node, network (types only).

## Repository policy (owner instruction, standing)
- **Never open pull requests.** Commit and `git push` every change directly to
  `main`. This is an explicit, standing owner order and overrides the default
  "branch + PR" workflow in the generic agent guidelines.
- There are no open PRs to merge; if one ever exists, merge it into `main`
  directly rather than reviewing it as a separate step.
- Keep `main` buildable: run the relevant crate's tests before pushing.

## Session 2026-11-11 — EIP-1559 fee semantics (VERIFIED, tests pass)

### Real defects found and fixed
1. **Base fee was permanently zero.** `node/src/node.rs` hardcoded
   `base_fee: 0` / `base_fee_per_gas: Some(0)` for both genesis and produced
   blocks, so the chain had no EIP-1559 fee mechanism at all. Added
   `next_base_fee()` in `blockchain/src/block.rs` (real EIP-1559 schedule:
   target = limit/2, +/- 1/8 per block, saturating) and wired it into block
   production. Genesis now sets 1 gwei; each block derives its fee from the
   parent. Verified live: 1e9 -> 0x147b0e55 after 8 empty blocks (= 0.875^8).

2. **The whole gas charge was credited to the validator.** `evm/src/transition.rs`
   credited `actual_cost = gas_price * gas_used` to the coinbase, minting the
   base-fee portion. Now `effective_gas_price()` = `min(maxFeePerGas,
   baseFee + maxPriorityFeePerGas)` and `fee_split()` burns the base fee while
   crediting only the tip. `TransitionOutcome` gained a `burned: [u8; 32]`
   field. Verified against an independent sender: validator received exactly
   `used * tip`, sender was debited exactly `used * effective`, and
   `used * base_fee` was burned.

3. **`effective_gas_price` was computed after the sender was debited.** An
   unusable fee ceiling returned an error while leaving the account mutated.
   Moved the computation before any state change, so a rejection is atomic.

4. **Invalid transactions were included in blocks.** `produce_block` pushed
   any recoverable transaction into the block and, when `apply_transaction`
   failed (bad nonce, insufficient funds, fee ceiling below base fee),
   synthesised a receipt with intrinsic gas that no state transition backed.
   The header's `gas_used` then disagreed with the receipts. Now a
   transaction that fails validation is excluded entirely (`continue`), while
   an execution *revert* is still included with a failed receipt, as before.
   Verified: an under-priced EIP-1559 tx is left out, nonce and balance
   untouched, so it remains valid for a later block.

5. **Gas-limit overflow was checked after execution.** Applying a transaction
   and then deciding it does not fit would leave its state changes in a block
   that does not list it. The limit is now checked against `request.gas_limit`
   before the transaction runs.

6. **The mempool evicted transactions that were never mined.** It removed
   every pending hash after a block was produced. Now only hashes present in
   the produced block are removed, so a transaction left out for space or
   price is not silently lost.

7. **Decoded quantities were not canonical.** `trim_leading_zeros` stripped
   zero *bytes* but not zero *nibbles*, so the value 1000 decoded as `0x03e8`
   instead of `0x3e8` — invalid per the JSON-RPC spec. Replaced with
   `hex_quantity()`. This was caught by an existing node test.

8. **Receipt/transaction JSON reported the wrong price and type.**
   `effectiveGasPrice` echoed `tx.gas_price` (the *ceiling* for a type-2 tx),
   and `type` was hardcoded `"0x0"`. Both fixed via the shared
   `blockchain::effective_gas_price()` helper; mined transactions now report
   the price actually paid while `maxFeePerGas` carries the ceiling.

9. **`eth_gasPrice`/`eth_maxPriorityFeePerGas`/`eth_feeHistory` were stale.**
   `eth_gasPrice` reported the last transaction's `gas_price` (a ceiling);
   it now returns next-block base fee + a suggested tip. `eth_maxPriorityFeePerGas`
   is the median tip of recent blocks, floored at `min_gas_price`.
   `eth_feeHistory` rewards now report the *tip* (not the whole price) and the
   `baseFeePerGas` array ends with the next block's fee, as clients expect.

10. **Consensus now enforces the base fee.** `validate_block` previously only
    checked that `base_fee_per_gas` was present once London was active.
    Without an equality check a validator could set any fee. It now requires
    exactly `next_base_fee(parent...)`.

### Test counts (all passing)
- state 32, evm 77, blockchain 50, node 58, debug_trace 9, network 22

### Live verification (chain id 6666, genesis 1e21 wei, key 0x..01)
- Node on 127.0.0.1:8554, base fee falls 1e9 -> 0x147b0e55 over 8 empty blocks.
- Real type-2 tx from an independent key: receipt `status 0x1`, `type 0x2`,
  `effectiveGasPrice == block baseFee + tip`, sender debit == `gasUsed * eff`,
  validator credit == `gasUsed * tip`, burn == `gasUsed * baseFee`.

### Note for future sessions
Earlier notes in this file said `blockchain` had 21 tests; it now has 50.
The `TransactionRequest` (evm) and `Transaction` (blockchain) structs both
carry `max_priority_fee_per_gas`; any new literal must set it.
