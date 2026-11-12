# Vendored third-party libraries

These are unmodified upstream sources, checked in so the build is reproducible
and needs no network access. Do not edit them in place; to upgrade, replace the
whole directory from the upstream commit named below and re-run the KZG tests.

## blst

- Upstream: https://github.com/supranational/blst
- Commit: `cf7f2e372bc7ff76fa99dd1ee1b4085d2aeee6f1`
- License: Apache-2.0 (see `blst/LICENSE`)
- Vendored: `src/`, `bindings/blst.h`, `bindings/blst_aux.h`, `build/` (the
  per-format assembly, including `elf/`), `LICENSE`.

BLS12-381 curve arithmetic and pairings, used for the EIP-4844 KZG proof
verification and for the BLS signature checks the consensus layer needs.

`build/` is checked in even though the repository's `.gitignore` excludes a
`build/` directory by name. That rule is meant for compiler output. The files
here are hand-written assembly sources that the build requires, so they are
force-added (`git add -f`). If you copy this tree with an ignore-respecting
tool, make sure they come along.

## c-kzg-4844

- Upstream: https://github.com/ethereum/c-kzg-4844
- Commit: `a751e2d92abcd63b01d1400789789f5ea289a8b0`
- License: Apache-2.0 (see `ckzg/LICENSE`)
- Vendored: `src/ckzg.c`, `src/ckzg.h`, `src/common/`, `src/eip4844/`,
  `src/eip7594/`, `src/setup/`, `src/trusted_setup.txt`, `LICENSE`.

The reference EIP-4844 implementation: `blob_to_kzg_commitment`,
`compute_kzg_proof`, `verify_kzg_proof`, `verify_blob_kzg_proof`. It is a
single translation unit, `src/ckzg.c`, which `#include`s the rest.

### Trusted setup

`ckzg/src/trusted_setup.txt` is the mainnet EIP-4844 ceremony output: 4096 G1
monomial points, 65 G2 monomial points, and the 4096-point Lagrange form. The
KZG commitments and proofs this component produces are only meaningful against
this exact setup. Ethereum's EIP-4844 point-evaluation precompile uses the same
one, so a proof produced here verifies against mainnet's precompile and vice
versa.

## Verification

`blob_processor/cpp/tests/kzg_vectors.cpp` exercises both libraries against the
trusted setup: a valid proof must verify, and a tampered proof, a wrong claimed
value, and a mismatched commitment must each be rejected. Run it with
`make test` in `blob_processor/cpp`.
