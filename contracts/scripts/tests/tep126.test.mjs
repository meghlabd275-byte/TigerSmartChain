// TEP126: the chain's flagship token. Covers the fixed `override` lists and
// the permit path, which is the part that silently breaks if the domain
// separator or nonce handling is wrong.

import { ethers } from 'ethers';
import {
  describe,
  it,
  assert,
  assertEqual,
  assertBigIntEqual,
  assertAddressEqual,
} from '../test-harness.mjs';
import { EvmChain } from '../evm-chain.mjs';

const CAP = 1_000_000n * 10n ** 18n;
const INITIAL = 100_000n * 10n ** 18n;

async function deployToken(chain) {
  return chain.deploy('TEP126/TEP126:TEP126', [
    'Tiger Token',
    'TGR',
    18,
    CAP,
    INITIAL,
    '2.0.0',
  ]);
}

describe('TEP126', () => {
  it('exposes the metadata and initial supply passed to the constructor', async () => {
    const chain = await EvmChain.create();
    const token = await deployToken(chain);

    assertEqual(await token.value('name'), 'Tiger Token');
    assertEqual(await token.value('symbol'), 'TGR');
    assertBigIntEqual(await token.value('decimals'), 18n);
    assertBigIntEqual(await token.value('cap'), CAP);
    assertBigIntEqual(await token.value('totalSupply'), INITIAL);
    assertBigIntEqual(await token.value('balanceOf', chain.owner), INITIAL);
    assertEqual(await token.value('version'), '2.0.0');
    assertEqual(await token.value('mintingEnabled'), true);
  });

  it('grants every role to the deployer', async () => {
    const chain = await EvmChain.create();
    const token = await deployToken(chain);

    const admin = await token.value('DEFAULT_ADMIN_ROLE');
    const minter = await token.value('MINTER_ROLE');
    const pauser = await token.value('PAUSER_ROLE');

    assertEqual(await token.value('hasRole', admin, chain.owner), true);
    assertEqual(await token.value('hasRole', minter, chain.owner), true);
    assertEqual(await token.value('hasRole', pauser, chain.owner), true);
  });

  it('rejects a zero cap and an initial supply above the cap', async () => {
    const chain = await EvmChain.create();

    await assertRevertsMessage(
      chain.deploy('TEP126/TEP126:TEP126', ['T', 'T', 18, 0n, 0n, '1']),
      'TEP126: cap is 0',
    );
    await assertRevertsMessage(
      chain.deploy('TEP126/TEP126:TEP126', ['T', 'T', 18, 100n, 101n, '1']),
      'TEP126: initial supply exceeds cap',
    );
  });

  it('mints, emits Minted and enforces the cap', async () => {
    const chain = await EvmChain.create();
    const token = await deployToken(chain);
    const alice = await chain.account('alice');

    const { logs } = await token.fn('mint', alice, 500n * 10n ** 18n);
    // ERC20._mint emits Transfer before the token's own Minted event.
    const minted = logs.find((l) => l.name === 'Minted');
    assert(minted, 'Minted was not emitted');
    assertAddressEqual(minted.args[0], alice);
    assertBigIntEqual(minted.args[1], 500n * 10n ** 18n);
    assertBigIntEqual(await token.value('balanceOf', alice), 500n * 10n ** 18n);

    const remaining = CAP - INITIAL - 500n * 10n ** 18n;
    await token.fn('mint', alice, remaining);
    assertBigIntEqual(await token.value('totalSupply'), CAP);

    await token.expectRevert('mint', [alice, 1n], 'TEP126: cap exceeded');
  });

  it('refuses to mint without the minter role', async () => {
    const chain = await EvmChain.create();
    const token = await deployToken(chain);
    const mallory = await chain.account('mallory');

    await token.expectRevert('mint', [mallory, 1n], 'AccessControlUnauthorizedAccount', {
      from: mallory,
    });
  });

  it('burns and reflects the reduced supply', async () => {
    const chain = await EvmChain.create();
    const token = await deployToken(chain);

    const { logs } = await token.fn('burn', 1000n * 10n ** 18n);
    assertEqual(logs.some((l) => l.name === 'Burned'), true);
    assertBigIntEqual(await token.value('totalSupply'), INITIAL - 1000n * 10n ** 18n);
    assertBigIntEqual(await token.value('balanceOf', chain.owner), INITIAL - 1000n * 10n ** 18n);
  });

  it('blocks transfers while paused and resumes after unpause', async () => {
    const chain = await EvmChain.create();
    const token = await deployToken(chain);
    const alice = await chain.account('alice');

    await token.fn('pause');
    assertEqual(await token.value('paused'), true);
    await token.expectRevert('transfer', [alice, 1n], 'EnforcedPause');

    await token.fn('unpause');
    assertEqual(await token.value('paused'), false);
    await token.fn('transfer', alice, 1n);
    assertBigIntEqual(await token.value('balanceOf', alice), 1n);
  });

  it('accepts an ERC-2612 permit and spends the granted allowance', async () => {
    const chain = await EvmChain.create();
    const token = await deployToken(chain);

    const ownerKey = await chain.privateKeyOf(chain.owner);
    const spender = await chain.account('spender');
    const value = 750n * 10n ** 18n;
    const deadline = 2n ** 48n;

    const nonce = await token.value('nonces', chain.owner);
    const signature = await signPermit(chain, token, ownerKey, {
      owner: chain.owner,
      spender,
      value,
      nonce,
      deadline,
    });

    const { logs } = await token.fnFrom('permit', [
      chain.owner,
      spender,
      value,
      deadline,
      signature.v,
      signature.r,
      signature.s,
    ]);

    assertEqual(logs.some((l) => l.name === 'Approval'), true);
    assertBigIntEqual(await token.value('allowance', chain.owner, spender), value);
    assertBigIntEqual(await token.value('nonces', chain.owner), nonce + 1n);

    await token.fnFrom('transferFrom', [chain.owner, spender, value], { from: spender });
    assertBigIntEqual(await token.value('balanceOf', spender), value);
    assertBigIntEqual(await token.value('allowance', chain.owner, spender), 0n);
  });

  it('rejects a permit with an expired deadline', async () => {
    const chain = await EvmChain.create();
    const token = await deployToken(chain);

    const ownerKey = await chain.privateKeyOf(chain.owner);
    const spender = await chain.account('spender');
    const deadline = 1n;

    const signature = await signPermit(chain, token, ownerKey, {
      owner: chain.owner,
      spender,
      value: 1n,
      nonce: await token.value('nonces', chain.owner),
      deadline,
    });

    await token.expectRevert(
      'permit',
      [chain.owner, spender, 1n, deadline, signature.v, signature.r, signature.s],
      'ERC2612ExpiredSignature',
    );
  });

  it('rejects a permit signed by someone other than the owner', async () => {
    const chain = await EvmChain.create();
    const token = await deployToken(chain);

    const impostorKey = await chain.privateKeyOf(await chain.account('impostor'));
    const spender = await chain.account('spender');
    const deadline = 2n ** 48n;

    const signature = await signPermit(chain, token, impostorKey, {
      owner: chain.owner,
      spender,
      value: 1n,
      nonce: await token.value('nonces', chain.owner),
      deadline,
    });

    await token.expectRevert(
      'permit',
      [chain.owner, spender, 1n, deadline, signature.v, signature.r, signature.s],
      'ERC2612InvalidSigner',
    );
  });

  it('rejects a replayed permit nonce', async () => {
    const chain = await EvmChain.create();
    const token = await deployToken(chain);
    const ownerKey = await chain.privateKeyOf(chain.owner);
    const spender = await chain.account('spender');
    const deadline = 2n ** 48n;
    const nonce = await token.value('nonces', chain.owner);

    const signature = await signPermit(chain, token, ownerKey, {
      owner: chain.owner,
      spender,
      value: 5n,
      nonce,
      deadline,
    });
    const args = [chain.owner, spender, 5n, deadline, signature.v, signature.r, signature.s];

    await token.fnFrom('permit', args);
    await token.expectRevert('permit', args, 'ERC2612InvalidSigner');
  });

  it('stops minting when minting is disabled', async () => {
    const chain = await EvmChain.create();
    const token = await deployToken(chain);

    await token.fn('setMintingEnabled', false);
    assertEqual(await token.value('mintingEnabled'), false);
    await token.expectRevert('mint', [chain.owner, 1n], 'TEP126: minting disabled');
  });

  it('lets the admin raise the cap', async () => {
    const chain = await EvmChain.create();
    const token = await deployToken(chain);

    await token.fn('updateCap', CAP * 2n);
    assertBigIntEqual(await token.value('cap'), CAP * 2n);
  });
});

async function assertRevertsMessage(promise, match) {
  try {
    await promise;
  } catch (error) {
    assert(
      error.message.includes(match),
      `expected revert matching "${match}" but got: ${error.message}`,
    );
    return;
  }
  throw new Error(`expected a revert matching "${match}" but the deploy succeeded`);
}

// Signs an ERC-2612 permit using the token's own DOMAIN_SEPARATOR, so the test
// fails if the contract computes a separator that does not match the chain id.
async function signPermit(chain, token, privateKey, { owner, spender, value, nonce, deadline }) {
  const domainSeparator = await token.value('DOMAIN_SEPARATOR');
  const structHash = ethers.keccak256(
    ethers.AbiCoder.defaultAbiCoder().encode(
      ['bytes32', 'address', 'address', 'uint256', 'uint256', 'uint256'],
      [
        ethers.keccak256(
          ethers.toUtf8Bytes('Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)'),
        ),
        owner,
        spender,
        value,
        nonce,
        deadline,
      ],
    ),
  );

  const digest = ethers.keccak256(
    ethers.concat(['0x1901', domainSeparator, structHash]),
  );

  const signature = new ethers.Wallet(privateKey).signingKey.sign(digest);
  return { v: signature.v, r: signature.r, s: signature.s };
}
