#!/usr/bin/env bash
# Build a TigerSmartChain genesis file from a list of validator sealing keys.
#
# Usage:  scripts/generate-genesis.sh <chain-id> <out.json> <key> [<key> ...]
#
# Each key is a 32-byte hex private key. The script derives the matching
# address with the node's own crypto code, so the authority set in the file is
# exactly the set those keys can seal with — a mismatch would leave the chain
# unable to produce a block.
#
# The genesis pins `validators`, so every node on the chain enforces the same
# authority set and none can be talked into adopting a foreign proposer.
set -euo pipefail

if [ "$#" -lt 3 ]; then
  echo "usage: $0 <chain-id> <out.json> <key> [<key> ...]" >&2
  exit 2
fi

CHAIN_ID="$1"; shift
OUT="$1"; shift

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

addresses=()
for key in "$@"; do
  addr="$(cd "$REPO_ROOT/crypto" && cargo run --quiet --example derive_address -- "$key")"
  addresses+=("$addr")
done

python3 - "$CHAIN_ID" "$OUT" "${addresses[@]}" <<'PY'
import json, sys

chain_id, out, *validators = sys.argv[1:]
chain_id = int(chain_id)

genesis = {
    "config": {
        "chainId": chain_id,
        "homesteadBlock": 0,
        "eip150Block": 0,
        "eip155Block": 0,
        "eip158Block": 0,
        "byzantiumBlock": 0,
        "constantinopleBlock": 0,
        "petersburgBlock": 0,
        "istanbulBlock": 0,
        "berlinBlock": 0,
        "londonBlock": 0,
        "epochLength": 200,
        "blockReward": "2000000000000000000",
        "validators": validators,
    },
    "nonce": "0x0",
    "timestamp": "0x0",
    "extraData": "0x" + "TigerSmartChain".encode().hex(),
    "gasLimit": "0x1c9c380",
    "difficulty": "0x1",
    "mixHash": "0x" + "00" * 32,
    "coinbase": "0x" + "00" * 20,
    "alloc": {},
}

# Fund the treasury and each validator from the chain's own allocation, so the
# chain starts with a spendable supply rather than an empty state.
genesis["alloc"]["0x0000000000000000000000000000000000001000"] = {
    "balance": "1000000000000000000000000000"
}
for v in validators:
    genesis["alloc"][v] = {"balance": "1000000000000000000000000"}

with open(out, "w") as fh:
    json.dump(genesis, fh, indent=2)
    fh.write("\n")

print(f"wrote {out}: chain {chain_id}, {len(validators)} validator(s)")
PY
