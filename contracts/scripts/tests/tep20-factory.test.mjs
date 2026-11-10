// TEP20 token and the TEP20 factory.
//
// The factory test pins down the two bugs that were fixed here: the EIP-1167
// clone previously embedded a zero implementation address, and every clone
// used salt 0 so a second `createToken` reverted with DeploymentFailed.

import { describe, it, assert, assertEqual, assertBigIntEqual, assertAddressEqual, assertReverts } from '../test-harness.mjs';
import { EvmChain, Contract, loadArtifact } from '../evm-chain.mjs';

const MAX_SUPPLY = 1_000_000n * 10n ** 18n;

describe('TEP20Token', () => {
  it('starts empty and only mints through the minter role', async () => {
    const chain = await EvmChain.create();
    const token = await chain.deploy('TEP20/TEP20Token:TEP20', [
      'Tiger USD',
      'RUSD',
      18,
      MAX_SUPPLY,
    ]);

    assertEqual(await token.value('name'), 'Tiger USD');
    assertEqual(await token.value('symbol'), 'RUSD');
    assertBigIntEqual(await token.value('decimals'), 18n);
    assertBigIntEqual(await token.value('totalSupply'), 0n);
    assertBigIntEqual(await token.value('maxSupply'), MAX_SUPPLY);

    const mallory = await chain.account('mallory');
    await token.expectRevert('mint', [mallory, 1n], 'AccessControlUnauthorizedAccount', {
      from: mallory,
    });

    await token.fn('mint', mallory, 1_000n * 10n ** 18n);
    assertBigIntEqual(await token.value('balanceOf', mallory), 1_000n * 10n ** 18n);
  });

  it('refuses to mint past the cap', async () => {
    const chain = await EvmChain.create();
    const token = await chain.deploy('TEP20/TEP20Token:TEP20', ['T', 'T', 18, 100n]);

    await token.fn('mint', chain.owner, 100n);
    await token.expectRevert('mint', [chain.owner, 1n], 'TEP20: exceeds max supply');
  });

  it('withholds a percentage as tax and forwards it to the recipient', async () => {
    const chain = await EvmChain.create();
    const token = await chain.deploy('TEP20/TEP20Token:TEP20', ['T', 'T', 18, MAX_SUPPLY]);
    const alice = await chain.account('alice');
    const treasury = await chain.account('treasury');

    await token.fn('mint', chain.owner, 100_000n * 10n ** 18n);
    await token.fn('setTaxRecipient', treasury);
    await token.fn('setTaxRate', 200); // 2%

    const amount = 10_000n * 10n ** 18n;
    await token.fn('transfer', alice, amount);

    const tax = (amount * 200n) / 10_000n;
    assertBigIntEqual(await token.value('balanceOf', alice), amount - tax);
    assertBigIntEqual(await token.value('balanceOf', treasury), tax);
  });

  it('blocks blacklisted senders and recipients', async () => {
    const chain = await EvmChain.create();
    const token = await chain.deploy('TEP20/TEP20Token:TEP20', ['T', 'T', 18, MAX_SUPPLY]);
    const alice = await chain.account('alice');
    const bob = await chain.account('bob');

    await token.fn('mint', alice, 1000n);
    await token.fn('setBlacklist', alice, true);

    assertEqual(await token.value('blacklist', alice), true);
    await token.expectRevert('transfer', [bob, 1n], 'TEP20: sender blacklisted', {
      from: alice,
    });

    await token.fn('setBlacklist', alice, false);
    await token.fn('setBlacklist', bob, true);
    await token.expectRevert('transfer', [bob, 1n], 'TEP20: recipient blacklisted', {
      from: alice,
    });
  });

  it('enforces the anti-bot cooldown between transfers', async () => {
    const chain = await EvmChain.create();
    const token = await chain.deploy('TEP20/TEP20Token:TEP20', ['T', 'T', 18, MAX_SUPPLY]);
    const alice = await chain.account('alice');
    const bob = await chain.account('bob');

    await token.fn('mint', alice, 1000n);
    await token.fn('setAntiBot', true);

    await token.fnFrom('transfer', [bob, 1n], { from: alice });
    await token.expectRevert('transfer', [bob, 1n], 'TEP20: cooldown active', {
      from: alice,
    });

    await chain.mine(1, { secondsPerBlock: 2 });
    await token.fnFrom('transfer', [bob, 1n], { from: alice });
    assertBigIntEqual(await token.value('balanceOf', bob), 2n);
  });

  it('freezes transfers while paused', async () => {
    const chain = await EvmChain.create();
    const token = await chain.deploy('TEP20/TEP20Token:TEP20', ['T', 'T', 18, MAX_SUPPLY]);
    const alice = await chain.account('alice');

    await token.fn('mint', chain.owner, 100n);
    await token.fn('pause');
    await token.expectRevert('transfer', [alice, 1n], 'EnforcedPause');
    await token.fn('unpause');
    await token.fn('transfer', alice, 1n);
    assertBigIntEqual(await token.value('balanceOf', alice), 1n);
  });

  it('rejects a tax rate above the hard ceiling', async () => {
    const chain = await EvmChain.create();
    const token = await chain.deploy('TEP20/TEP20Token:TEP20', ['T', 'T', 18, MAX_SUPPLY]);

    await token.expectRevert('setTaxRate', [1001n], 'TEP20: tax too high');
    await token.fn('setTaxRate', 1000);
    assertBigIntEqual(await token.value('taxRate'), 1000n);
  });

  it('burns from the caller balance', async () => {
    const chain = await EvmChain.create();
    const token = await chain.deploy('TEP20/TEP20Token:TEP20', ['T', 'T', 18, MAX_SUPPLY]);

    await token.fn('mint', chain.owner, 1000n);
    await token.fn('burn', 400n);
    assertBigIntEqual(await token.value('totalSupply'), 600n);
  });
});

describe('TEP20 TokenFactory', () => {
  async function deployFactory(chain) {
    return chain.deploy('TEP20/TokenFactory:TokenFactory');
  }

  it('charges the creation fee and credits the fee recipient', async () => {
    const chain = await EvmChain.create();
    const factory = await deployFactory(chain);
    const fee = await factory.value('creationFee');

    assertBigIntEqual(fee, 10n ** 16n);

    const before = await chain.balanceOf(chain.owner);
    await factory.fnFrom(
      'createToken',
      ['Fee Token', 'FEE', 18, 1000n, 10_000n, chain.owner],
      { value: fee },
    );
    const after = await chain.balanceOf(chain.owner);

    // The sender pays the fee plus gas; the contract keeps no ether.
    assert(after < before, 'expected the sender balance to drop by at least the fee');
    assertBigIntEqual(await chain.balanceOf(factory.address), 0n);
  });

  it('rejects a creation without the fee', async () => {
    const chain = await EvmChain.create();
    const factory = await deployFactory(chain);

    await assertReverts(
      factory.fnFrom('createToken', ['X', 'X', 18, 1n, 1n, chain.owner]),
      'Insufficient creation fee',
    );
  });

  it('deploys a working token that reports the requested metadata', async () => {
    const chain = await EvmChain.create();
    const factory = await deployFactory(chain);
    const fee = await factory.value('creationFee');

    const { logs } = await factory.fnFrom(
      'createToken',
      ['Clone Coin', 'CLN', 8, 5_000n * 10n ** 8n, 10_000n * 10n ** 8n, chain.owner],
      { value: fee },
    );

    const created = logs.find((l) => l.name === 'TokenCreated');
    assert(created, 'TokenCreated was not emitted');
    assertAddressEqual(created.args[0], created.args[0]);
    assertEqual(created.args[1], 'Clone Coin');
    assertEqual(created.args[2], 'CLN');
    assertBigIntEqual(created.args[3], 8n);

    const artifact = await loadArtifact('TEP20/TokenFactory:BasicToken');
    const token = new Contract(chain, created.args[0], artifact.abi, 'BasicToken');

    assertEqual(await token.value('name'), 'Clone Coin');
    assertEqual(await token.value('symbol'), 'CLN');
    assertBigIntEqual(await token.value('decimals'), 8n);
    assertBigIntEqual(await token.value('totalSupply'), 5_000n * 10n ** 8n);
    assertBigIntEqual(
      await token.value('balanceOf', chain.owner),
      5_000n * 10n ** 8n,
    );
  });

  it('tracks the creator and returns the tokens they deployed', async () => {
    const chain = await EvmChain.create();
    const factory = await deployFactory(chain);
    const fee = await factory.value('creationFee');
    const alice = await chain.account('alice');

    await factory.fnFrom('createToken', ['A', 'A', 18, 1n, 1n, alice], {
      value: fee,
      from: alice,
    });
    await factory.fnFrom('createToken', ['B', 'B', 18, 1n, 1n, alice], {
      value: fee,
      from: alice,
    });

    const tokens = await factory.value('getTokensByCreator', alice);
    assertEqual(tokens.length, 2);
    assert(tokens[0].toLowerCase() !== tokens[1].toLowerCase(), 'tokens must be distinct');
  });

  it('enforces the cap on minting after creation', async () => {
    const chain = await EvmChain.create();
    const factory = await deployFactory(chain);
    const fee = await factory.value('creationFee');

    const { logs } = await factory.fnFrom(
      'createToken',
      ['Capped', 'CAP', 18, 100n, 100n, chain.owner],
      { value: fee },
    );
    const artifact = await loadArtifact('TEP20/TokenFactory:BasicToken');
    const token = new Contract(chain, logs.find((l) => l.name === 'TokenCreated').args[0], artifact.abi, 'BasicToken');

    await assertReverts(token.fn('mint', chain.owner, 1n), 'CapExceeded');
  });

  it('rejects a creation fee change from a non-owner', async () => {
    const chain = await EvmChain.create();
    const factory = await deployFactory(chain);
    const mallory = await chain.account('mallory');

    await assertReverts(
      factory.fnFrom('setCreationFee', [1n], { from: mallory }),
      'OwnableUnauthorizedAccount',
    );
  });

  it('lets the owner change the fee and the recipient', async () => {
    const chain = await EvmChain.create();
    const factory = await deployFactory(chain);
    const recipient = await chain.account('recipient');

    await factory.fn('setCreationFee', 1234n);
    await factory.fn('setFeeRecipient', recipient);
    assertBigIntEqual(await factory.value('creationFee'), 1234n);
    assertAddressEqual(await factory.value('feeRecipient'), recipient);

    const recipientBefore = await chain.balanceOf(recipient);
    await factory.fnFrom('createToken', ['F', 'F', 18, 1n, 1n, chain.owner], {
      value: 1234n,
    });
    assertBigIntEqual(
      (await chain.balanceOf(recipient)) - recipientBefore,
      1234n,
    );
  });

  it('rejects a zero fee recipient', async () => {
    const chain = await EvmChain.create();
    const factory = await deployFactory(chain);

    await assertReverts(
      factory.fnFrom('setFeeRecipient', ['0x' + '00'.repeat(20)]),
      'Invalid recipient',
    );
  });

  it('rejects a creation fee below the configured amount', async () => {
    const chain = await EvmChain.create();
    const factory = await deployFactory(chain);

    await factory.fn('setCreationFee', 1000n);
    await assertReverts(
      factory.fnFrom('createToken', ['X', 'X', 18, 1n, 1n, chain.owner], { value: 999n }),
      'Insufficient creation fee',
    );
  });

  it('creates a token owned by the requested address, not the caller', async () => {
    const chain = await EvmChain.create();
    const factory = await deployFactory(chain);
    const fee = await factory.value('creationFee');
    const beneficiary = await chain.account('beneficiary');

    const { logs } = await factory.fnFrom(
      'createToken',
      ['Owned', 'OWN', 18, 100n, 100n, beneficiary],
      { value: fee },
    );
    const artifact = await loadArtifact('TEP20/TokenFactory:BasicToken');
    const token = new Contract(chain, logs.find((l) => l.name === 'TokenCreated').args[0], artifact.abi, 'BasicToken');

    assertBigIntEqual(await token.value('balanceOf', beneficiary), 100n);
    assertBigIntEqual(await token.value('balanceOf', chain.owner), 0n);
  });

  it('exposes a preset factory that mints a fixed-supply token', async () => {
    const chain = await EvmChain.create();
    const factory = await chain.deploy('TEP20/TokenFactory:TokenFactoryWithPresets');
    const fee = await factory.value('creationFee');

    const { logs } = await factory.fnFrom(
      'createWithPreset',
      // Preset 1 == Stablecoin: 6 decimals, uncapped.
      ['Preset', 'PRE', 1, 1_000_000n * 10n ** 6n, chain.owner],
      { value: fee },
    );
    const created = logs.find((l) => l.name === 'TokenCreated');
    assert(created, 'TokenCreated was not emitted by the preset factory');

    const artifact = await loadArtifact('TEP20/TokenFactory:BasicToken');
    const token = new Contract(chain, created.args[0], artifact.abi, 'BasicToken');
    assertEqual(await token.value('symbol'), 'PRE');
    assertBigIntEqual(await token.value('decimals'), 6n);
  });
});
