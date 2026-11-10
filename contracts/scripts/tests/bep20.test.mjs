// BEP20: the self-contained token implementation used for RUSD-style assets.
// The tax path is the interesting part: it splits the fee between the treasury
// and a burn, and both halves have to land in the right place.

import { describe, it, assert, assertEqual, assertBigIntEqual, assertAddressEqual, assertReverts } from '../test-harness.mjs';
import { EvmChain } from '../evm-chain.mjs';

const SUPPLY = 1_000_000n * 10n ** 18n;

async function deployToken(chain, { treasury, supply = SUPPLY } = {}) {
  const treasuryAddress = treasury ?? (await chain.account('treasury'));
  const token = await chain.deploy('bep20/BEP20:BEP20Token', [
    'Tiger USD',
    'RUSD',
    18,
    supply,
    treasuryAddress,
  ]);
  return { token, treasury: treasuryAddress };
}

describe('BEP20Token', () => {
  it('mints the whole supply to the deployer and records the treasury', async () => {
    const chain = await EvmChain.create();
    const { token, treasury } = await deployToken(chain);

    assertEqual(await token.value('name'), 'Tiger USD');
    assertEqual(await token.value('symbol'), 'RUSD');
    assertBigIntEqual(await token.value('decimals'), 18n);
    assertBigIntEqual(await token.value('totalSupply'), SUPPLY);
    assertBigIntEqual(await token.value('balanceOf', chain.owner), SUPPLY);
    assertAddressEqual(await token.value('treasury'), treasury);
    assertAddressEqual(await token.value('operator'), chain.owner);
  });

  it('starts with the documented default tax parameters', async () => {
    const chain = await EvmChain.create();
    const { token } = await deployToken(chain);

    assertBigIntEqual(await token.value('taxRate'), 200n);
    assertBigIntEqual(await token.value('burnRate'), 50n);
    assertEqual(await token.value('taxEnabled'), true);
    assertEqual(await token.value('blacklistEnabled'), true);
  });

  it('splits the transfer tax between treasury and burn', async () => {
    const chain = await EvmChain.create();
    const { token, treasury } = await deployToken(chain);
    const alice = await chain.account('alice');
    const bob = await chain.account('bob');

    // The deployer and the treasury are fee-excluded, so move the balance to an
    // ordinary holder before switching the tax on.
    await token.fn('setTaxEnabled', false);
    await token.fn('transfer', alice, 10_000n * 10n ** 18n);
    await token.fn('setTaxEnabled', true);

    const amount = 10_000n * 10n ** 18n;
    await token.fnFrom('transfer', [bob, amount], { from: alice });

    // 2% tax, half of which is burned.
    const tax = (amount * 200n) / 10_000n;
    const burned = (tax * 50n) / 100n;

    assertBigIntEqual(await token.value('balanceOf', bob), amount - tax);
    assertBigIntEqual(await token.value('balanceOf', treasury), tax - burned);
    assertBigIntEqual(await token.value('totalSupply'), SUPPLY - burned);
  });

  it('moves the full amount when tax is disabled', async () => {
    const chain = await EvmChain.create();
    const { token, treasury } = await deployToken(chain);
    const alice = await chain.account('alice');

    await token.fn('setTaxEnabled', false);
    await token.fn('transfer', alice, 1000n);

    assertBigIntEqual(await token.value('balanceOf', alice), 1000n);
    assertBigIntEqual(await token.value('balanceOf', treasury), 0n);
    assertBigIntEqual(await token.value('totalSupply'), SUPPLY);
  });

  it('skips the tax when either side is excluded from fees', async () => {
    const chain = await EvmChain.create();
    const { token } = await deployToken(chain);
    const alice = await chain.account('alice');
    const bob = await chain.account('bob');

    // Seed alice without tax, then tax the alice -> bob leg.
    await token.fn('setTaxEnabled', false);
    await token.fn('transfer', alice, 2000n);
    await token.fn('setTaxEnabled', true);

    await token.fn('excludeFromFee', alice);
    await token.fnFrom('transfer', [bob, 1000n], { from: alice });
    assertBigIntEqual(await token.value('balanceOf', bob), 1000n);

    await token.fn('includeInFee', alice);
    await token.fnFrom('transfer', [bob, 1000n], { from: alice });
    assertBigIntEqual(await token.value('balanceOf', bob), 1980n);
  });

  it('honours the blacklist in both directions', async () => {
    const chain = await EvmChain.create();
    const { token } = await deployToken(chain);
    const alice = await chain.account('alice');
    const bob = await chain.account('bob');

    await token.fn('transfer', alice, 1000n);
    await token.fn('addToBlacklist', alice);
    assertEqual(await token.value('isBlacklisted', alice), true);
    await assertReverts(token.fnFrom('transfer', [bob, 1n], { from: alice }), 'Blacklisted');

    await token.fn('removeFromBlacklist', alice);
    await token.fn('addToBlacklist', bob);
    await assertReverts(token.fnFrom('transfer', [bob, 1n], { from: alice }), 'Blacklisted');
  });

  it('enforces the tax and burn rate ceilings', async () => {
    const chain = await EvmChain.create();
    const { token } = await deployToken(chain);

    await assertReverts(token.fn('setTaxRate', 1001n), 'Rate too high');
    await assertReverts(token.fn('setBurnRate', 101n), 'Rate too high');

    await token.fn('setTaxRate', 1000);
    await token.fn('setBurnRate', 100);
    assertBigIntEqual(await token.value('taxRate'), 1000n);
    assertBigIntEqual(await token.value('burnRate'), 100n);
  });

  it('restricts the admin surface to the operator', async () => {
    const chain = await EvmChain.create();
    const { token } = await deployToken(chain);
    const mallory = await chain.account('mallory');

    for (const call of [
      ['setTaxRate', [1n]],
      ['setBurnRate', [1n]],
      ['setTaxEnabled', [false]],
      ['addToBlacklist', [mallory]],
      ['excludeFromFee', [mallory]],
      ['mint', [mallory, 1n]],
    ]) {
      await assertReverts(
        token.fnFrom(call[0], call[1], { from: mallory }),
        'Not operator',
      );
    }
  });

  it('lets the operator hand over the operator role', async () => {
    const chain = await EvmChain.create();
    const { token } = await deployToken(chain);
    const newOperator = await chain.account('newOperator');

    await token.fn('transferOperator', newOperator);
    assertAddressEqual(await token.value('operator'), newOperator);
    await assertReverts(token.fn('setTaxRate', 1n), 'Not operator');
    await token.fnFrom('setTaxRate', [1n], { from: newOperator });
  });

  it('mints and burns as the operator', async () => {
    const chain = await EvmChain.create();
    const { token } = await deployToken(chain);
    const alice = await chain.account('alice');

    await token.fn('mint', alice, 500n);
    assertBigIntEqual(await token.value('totalSupply'), SUPPLY + 500n);

    await token.fnFrom('burn', [200n], { from: alice });
    assertBigIntEqual(await token.value('balanceOf', alice), 300n);
  });

  it('supports approve plus transferFrom', async () => {
    const chain = await EvmChain.create();
    const { token } = await deployToken(chain);
    const alice = await chain.account('alice');
    const spender = await chain.account('spender');
    const bob = await chain.account('bob');

    // The deployer and the treasury are fee-excluded, so the taxed leg has to
    // run through an ordinary holder.
    await token.fn('transfer', alice, 10_000n);
    await token.fnFrom('approve', [spender, 1000n], { from: alice });
    assertBigIntEqual(await token.value('allowance', alice, spender), 1000n);

    await token.fnFrom('transferFrom', [alice, bob, 1000n], { from: spender });
    assertBigIntEqual(await token.value('balanceOf', bob), 1000n - 20n);
    assertBigIntEqual(await token.value('allowance', alice, spender), 0n);
  });

  it('rejects a transferFrom above the allowance', async () => {
    const chain = await EvmChain.create();
    const { token } = await deployToken(chain);
    const spender = await chain.account('spender');
    const bob = await chain.account('bob');

    await token.fn('approve', spender, 10n);
    await assertReverts(
      token.fnFrom('transferFrom', [chain.owner, bob, 11n], { from: spender }),
      'BEP20: insufficient allowance',
    );
  });

  it('rejects a transfer larger than the balance', async () => {
    const chain = await EvmChain.create();
    const { token } = await deployToken(chain);
    const alice = await chain.account('alice');

    await assertReverts(
      token.fn('transfer', alice, SUPPLY + 1n),
      'BEP20: transfer amount exceeds balance',
    );
  });
});
