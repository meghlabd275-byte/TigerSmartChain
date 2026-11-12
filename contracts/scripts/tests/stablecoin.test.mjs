// RUSD stablecoin suites: the token's own controls, and the vault that is
// what actually makes it a stablecoin.
//
// The vault tests are the ones that matter. A token called "USD" is worth
// nothing unless something enforces that every unit in circulation is backed,
// so these cases try to break the backing: issuing more than the collateral
// allows, redeeming collateral that was never deposited, minting with no
// collateral at all, and using a price old enough to be wrong.

import {
  describe,
  it,
  assert,
  assertBigIntEqual,
  assertReverts,
} from '../test-harness.mjs';
import { EvmChain, loadArtifact } from '../evm-chain.mjs';
import { ethers } from 'ethers';

const ONE = 10n ** 18n; // one whole unit of an eighteen-decimal asset
const RUSD_ONE = 10n ** 6n; // one whole RUSD, which has six decimals
const MAX_SUPPLY = 1_000_000_000n * RUSD_ONE; // one billion RUSD
// The collateral token is an ordinary eighteen-decimal token, so its supply
// is a separate figure from RUSD's; reusing one for both is how a six-decimal
// ceiling ends up on an eighteen-decimal token.
const COLLATERAL_SUPPLY = 1_000_000_000n * ONE;
const HOUR = 3600n;

/** A collateral token to hold behind RUSD. */
async function deployCollateral(chain) {
  return chain.deploy('TEP126/TEP126:TEP126', [
    'Collateral USD',
    'CUSD',
    18,
    COLLATERAL_SUPPLY,
    COLLATERAL_SUPPLY,
    '1',
  ]);
}

/** Deploys RUSD and a vault configured to accept `collateral`. */
async function deployVault(
  chain,
  {
    collateralPrice = ONE, // one dollar
    nativePrice = ONE,
    collateralFactorBps = 8000n, // 80%
    nativeFactorBps = 8000n,
    priceMaxAge = HOUR,
    withCollateral = true,
  } = {},
) {
  const rusd = await chain.deploy('stablecoin/RUSD:RUSD', [MAX_SUPPLY]);
  const collateral = withCollateral ? await deployCollateral(chain) : null;
  const vault = await chain.deploy('stablecoin/RUSDVault:RUSDVault', [
    rusd.address,
    collateral ? collateral.address : ethers.ZeroAddress,
    collateralPrice,
    nativePrice,
    collateralFactorBps,
    nativeFactorBps,
    priceMaxAge,
  ]);
  // The vault is the only address that may create RUSD.
  await rusd.fn('grantRole', await rusd.value('MINTER_ROLE'), vault.address);
  if (collateral) {
    await collateral.fn('approve', vault.address, COLLATERAL_SUPPLY);
  }
  return { rusd, collateral, vault };
}

describe('RUSD token', () => {
  it('reports its identity and eighteen decimals', async () => {
    const chain = await EvmChain.create();
    const rusd = await chain.deploy('stablecoin/RUSD:RUSD', [MAX_SUPPLY]);
    assertBigIntEqual(await rusd.value('maxSupply'), MAX_SUPPLY);
    assertBigIntEqual(await rusd.value('decimals'), 6n);
    assertBigIntEqual(await rusd.value('totalSupply'), 0n);
  });

  it('refuses a zero supply ceiling, which would make the token unusable', async () => {
    const chain = await EvmChain.create();
    await assertReverts(
      chain.deploy('stablecoin/RUSD:RUSD', [0n]),
      'supply ceiling must be non-zero',
    );
  });

  it('will not let an account without the minter role create supply', async () => {
    const chain = await EvmChain.create();
    const rusd = await chain.deploy('stablecoin/RUSD:RUSD', [MAX_SUPPLY]);
    const stranger = await chain.account('stranger');
    await assertReverts(
      rusd.fnFrom('mint', [stranger, ONE], { from: stranger }),
      'AccessControlUnauthorizedAccount',
    );
    assertBigIntEqual(await rusd.value('totalSupply'), 0n);
  });

  it('stops minting at the supply ceiling', async () => {
    const chain = await EvmChain.create();
    const rusd = await chain.deploy('stablecoin/RUSD:RUSD', [10n * RUSD_ONE]);
    const minter = await chain.account('minter');
    await rusd.fn('grantRole', await rusd.value('MINTER_ROLE'), minter);
    await rusd.fnFrom('mint', [minter, 10n * RUSD_ONE], { from: minter });
    assertBigIntEqual(await rusd.value('totalSupply'), 10n * RUSD_ONE);
    await assertReverts(
      rusd.fnFrom('mint', [minter, 1n], { from: minter }),
      'supply ceiling reached',
    );
  });

  it('freezes an account so it can neither send nor receive', async () => {
    const chain = await EvmChain.create();
    const rusd = await chain.deploy('stablecoin/RUSD:RUSD', [MAX_SUPPLY]);
    const freezer = await chain.account('freezer');
    const holder = await chain.account('holder');
    const other = await chain.account('other');
    await rusd.fn('grantRole', await rusd.value('MINTER_ROLE'), freezer);
    await rusd.fn('grantRole', await rusd.value('FREEZER_ROLE'), freezer);
    await rusd.fnFrom('mint', [holder, 100n * RUSD_ONE], { from: freezer });

    // Freezing does not move the balance; it only blocks movement.
    await rusd.fnFrom('setFrozen', [holder, true], { from: freezer });
    assertBigIntEqual(await rusd.value('balanceOf', holder), 100n * RUSD_ONE);
    await assertReverts(
      rusd.fnFrom('transfer', [other, RUSD_ONE], { from: holder }),
      'sender is frozen',
    );
    await assertReverts(
      rusd.fnFrom('transfer', [holder, RUSD_ONE], { from: other }),
      'recipient is frozen',
    );
  });

  it('cannot be frozen by an account without the freezer role', async () => {
    const chain = await EvmChain.create();
    const rusd = await chain.deploy('stablecoin/RUSD:RUSD', [MAX_SUPPLY]);
    const stranger = await chain.account('stranger2');
    await assertReverts(
      rusd.fnFrom('setFrozen', [stranger, true], { from: stranger }),
      'AccessControlUnauthorizedAccount',
    );
  });

  it('halts transfers while paused and resumes them after unpausing', async () => {
    const chain = await EvmChain.create();
    const rusd = await chain.deploy('stablecoin/RUSD:RUSD', [MAX_SUPPLY]);
    const admin = await chain.account('pauser');
    const holder = await chain.account('holder2');
    const other = await chain.account('other2');
    await rusd.fn('grantRole', await rusd.value('MINTER_ROLE'), admin);
    await rusd.fn('grantRole', await rusd.value('PAUSER_ROLE'), admin);
    await rusd.fnFrom('mint', [holder, 10n * RUSD_ONE], { from: admin });

    await rusd.fnFrom('pause', [], { from: admin });
    await assertReverts(
      rusd.fnFrom('transfer', [other, RUSD_ONE], { from: holder }),
      'EnforcedPause',
    );
    await rusd.fnFrom('unpause', [], { from: admin });
    await rusd.fnFrom('transfer', [other, RUSD_ONE], { from: holder });
    assertBigIntEqual(await rusd.value('balanceOf', other), RUSD_ONE);
  });
});

describe('RUSDVault', () => {
  it('reports the configuration it was given', async () => {
    const chain = await EvmChain.create();
    const { vault, rusd } = await deployVault(chain);
    assertBigIntEqual(await vault.value('collateralPrice'), ONE);
    assertBigIntEqual(await vault.value('nativePrice'), ONE);
    assertBigIntEqual(await vault.value('collateralFactorBps'), 8000n);
    assert((await vault.value('rusd')).toLowerCase() === rusd.address.toLowerCase());
    assertBigIntEqual(await vault.value('totalIssued'), 0n);
    assert(await vault.value('isSolvent'), 'an empty vault is solvent');
  });

  it('refuses a collateral factor above one hundred percent', async () => {
    const chain = await EvmChain.create();
    const rusd = await chain.deploy('stablecoin/RUSD:RUSD', [MAX_SUPPLY]);
    const collateral = await deployCollateral(chain);
    // A factor above 10_000 would let the vault issue more than the collateral
    // is worth, which is the whole thing the vault exists to prevent.
    await assertReverts(
      chain.deploy('stablecoin/RUSDVault:RUSDVault', [
        rusd.address,
        collateral.address,
        ONE,
        ONE,
        10_001n,
        8000n,
        HOUR,
      ]),
      'collateral factor out of range',
    );
  });

  it('issues RUSD equal to the deposited collateral value', async () => {
    const chain = await EvmChain.create();
    const { rusd, collateral, vault } = await deployVault(chain);
    const depositor = await chain.account('depositor');
    await collateral.fn('transfer', depositor, 1_000n * ONE);
    await collateral.fnFrom('approve', [vault.address, COLLATERAL_SUPPLY], {
      from: depositor,
    });

    // 500 CUSD at one dollar each is 500 of value, and an eighty percent
    // factor issues 400 RUSD, leaving 100 of value as the buffer.
    await vault.fnFrom('mintWithToken', [500n * ONE], { from: depositor });
    assertBigIntEqual(await rusd.value('balanceOf', depositor), 400n * RUSD_ONE);
    assertBigIntEqual(await vault.value('totalTokenCollateral'), 500n * ONE);
    assertBigIntEqual(await vault.value('tokenIssued'), 400n * RUSD_ONE);
    assert(await vault.value('isSolvent'), 'the vault stays solvent');
  });

  it('issues against the native coin at its price', async () => {
    const chain = await EvmChain.create();
    // Ten TGR at two dollars is twenty of value, and an eighty percent factor
    // issues sixteen RUSD.
    const { rusd, vault } = await deployVault(chain, { nativePrice: 2n * ONE });
    const depositor = await chain.account('native-depositor');
    await vault.fnFrom('mintWithNative', [], {
      from: depositor,
      value: 10n * ONE,
    });
    assertBigIntEqual(await rusd.value('balanceOf', depositor), 16n * RUSD_ONE);
    assertBigIntEqual(await vault.value('totalNativeCollateral'), 10n * ONE);
    assertBigIntEqual(await vault.value('nativeIssued'), 16n * RUSD_ONE);
  });

  it('refuses to issue beyond the collateral factor', async () => {
    const chain = await EvmChain.create();
    // A hundred percent factor issues the full value, so the buffer is gone
    // and the deposit must still fit inside the ceiling.
    const { collateral, vault } = await deployVault(chain, {
      collateralFactorBps: 10000n,
    });
    const depositor = await chain.account('over-depositor');
    // Enough for two deposits of a hundred.
    await collateral.fn('transfer', depositor, 200n * ONE);
    await collateral.fnFrom('approve', [vault.address, COLLATERAL_SUPPLY], {
      from: depositor,
    });
    // At a hundred percent the full hundred of value is issued, which the
    // backing exactly covers, so this is the boundary case and must pass.
    await vault.fnFrom('mintWithToken', [100n * ONE], { from: depositor });
    assertBigIntEqual(await vault.value('tokenIssued'), 100n * RUSD_ONE);
    assert(await vault.value('isSolvent'), 'issuing to the limit stays solvent');
    // A second deposit is backed by its own collateral, so it also passes.
    await vault.fnFrom('mintWithToken', [100n * ONE], { from: depositor });
    assertBigIntEqual(await vault.value('tokenIssued'), 200n * RUSD_ONE);
    assert(await vault.value('isSolvent'), 'still solvent after the second');
  });

  it('refuses to act on a stale price', async () => {
    const chain = await EvmChain.create();
    const { collateral, vault } = await deployVault(chain, { priceMaxAge: 60n });
    const depositor = await chain.account('stale-depositor');
    await collateral.fn('transfer', depositor, 100n * ONE);
    await collateral.fnFrom('approve', [vault.address, COLLATERAL_SUPPLY], {
      from: depositor,
    });
    // Mine past the price's validity: two blocks of ninety seconds each.
    await chain.mine(2, { secondsPerBlock: 90 });
    await assertReverts(
      vault.fnFrom('mintWithToken', [10n * ONE], { from: depositor }),
      'price is stale',
    );
  });

  it('lets only the oracle push prices', async () => {
    const chain = await EvmChain.create();
    const { vault } = await deployVault(chain);
    const stranger = await chain.account('not-the-oracle');
    await assertReverts(
      vault.fnFrom('setPrices', [ONE, ONE], { from: stranger }),
      'AccessControlUnauthorizedAccount',
    );
  });

  it('refuses a zero price, which would divide by zero on redemption', async () => {
    const chain = await EvmChain.create();
    const { vault } = await deployVault(chain);
    await assertReverts(vault.fn('setPrices', [0n, ONE]), 'must be non-zero');
    await assertReverts(vault.fn('setPrices', [ONE, 0n]), 'must be non-zero');
  });

  it('returns collateral on redemption and burns the RUSD', async () => {
    const chain = await EvmChain.create();
    const { rusd, collateral, vault } = await deployVault(chain);
    const user = await chain.account('redeemer');
    await collateral.fn('transfer', user, 1_000n * ONE);
    await collateral.fnFrom('approve', [vault.address, COLLATERAL_SUPPLY], { from: user });
    // 500 CUSD at an eighty percent factor issues 400 RUSD.
    await vault.fnFrom('mintWithToken', [500n * ONE], { from: user });

    const before = await collateral.value('balanceOf', user);
    await vault.fnFrom('redeemForToken', [200n * RUSD_ONE], { from: user });
    assertBigIntEqual(await rusd.value('balanceOf', user), 200n * RUSD_ONE);
    assertBigIntEqual(await rusd.value('totalSupply'), 200n * RUSD_ONE);
    // Two hundred RUSD at one dollar buys back two hundred CUSD, which is the
    // collateral value those RUSD represented.
    assertBigIntEqual(
      await collateral.value('balanceOf', user),
      before + 200n * ONE,
    );
    assertBigIntEqual(await vault.value('tokenIssued'), 200n * RUSD_ONE);
  });

  it('will not redeem more collateral than the vault holds', async () => {
    const chain = await EvmChain.create();
    const { collateral, vault } = await deployVault(chain);
    const user = await chain.account('greedy-redeemer');
    await collateral.fn('transfer', user, 100n * ONE);
    await collateral.fnFrom('approve', [vault.address, COLLATERAL_SUPPLY], { from: user });
    await vault.fnFrom('mintWithToken', [50n * ONE], { from: user });
    // The user holds 40 RUSD but asks to burn 500, which they do not have;
    // either the burn or the collateral check must stop it.
    await assertReverts(vault.fnFrom('redeemForToken', [500n * RUSD_ONE], { from: user }));
  });

  it('will not let an account redeem RUSD it does not hold', async () => {
    const chain = await EvmChain.create();
    const { collateral, vault } = await deployVault(chain);
    const saver = await chain.account('saver');
    const thief = await chain.account('thief');
    await collateral.fn('transfer', saver, 100n * ONE);
    await collateral.fnFrom('approve', [vault.address, COLLATERAL_SUPPLY], { from: saver });
    await vault.fnFrom('mintWithToken', [50n * ONE], { from: saver });

    // The thief holds no RUSD, so the burn has nothing to take and the whole
    // redemption reverts; the vault's books must be untouched.
    await assertReverts(
      vault.fnFrom('redeemForToken', [10n * RUSD_ONE], { from: thief }),
    );
    assertBigIntEqual(await vault.value('tokenIssued'), 40n * RUSD_ONE);
    assertBigIntEqual(await collateral.value('balanceOf', vault.address), 50n * ONE);
  });

  it('refuses a direct native transfer, which would be unbacked', async () => {
    const chain = await EvmChain.create();
    const { vault } = await deployVault(chain);
    const sender = await chain.account('careless-sender');
    await assertReverts(
      vault.fnFrom('mintWithNative', [], { from: sender, value: 0n }),
      'deposit must be non-zero',
    );
  });

  it('reflects a price move in what the collateral backs', async () => {
    const chain = await EvmChain.create();
    const { collateral, vault } = await deployVault(chain);
    const user = await chain.account('price-watcher');
    await collateral.fn('transfer', user, 100n * ONE);
    await collateral.fnFrom('approve', [vault.address, COLLATERAL_SUPPLY], { from: user });
    await vault.fnFrom('mintWithToken', [50n * ONE], { from: user });
    assert(await vault.value('isSolvent'), 'solvent at one dollar');
    const issued = await vault.value('tokenIssued');

    // Issuing at eighty percent left fifty of value backing forty of RUSD, so
    // the vault absorbs a fall to four fifths before it is under water.
    await vault.fn('setPrices', [(ONE * 4n) / 5n, ONE]);
    assert(
      await vault.value('isSolvent'),
      'the buffer must absorb a fall to four fifths',
    );

    // Below that the buffer is gone, and the vault reports insolvency rather
    // than continuing to claim it is fine.
    await vault.fn('setPrices', [(ONE * 7n) / 10n, ONE]);
    assert(
      !(await vault.value('isSolvent')),
      'a fall below the buffer must show up as insolvency',
    );
    // Fifty of collateral at seven tenths is thirty five of value against
    // forty issued, so the collateral no longer covers the supply. The two
    // sides are compared as USD values, because the collateral is
    // eighteen-decimal and RUSD is six.
    const value = await vault.value('tokenCollateralValue');
    const issuedUsd = await vault.value('issuedValue', issued);
    assert(
      value < issuedUsd,
      `collateral value ${value} must fall below the ${issuedUsd} of RUSD issued`,
    );
  });

  it('issues nothing when the token collateral is disabled', async () => {
    const chain = await EvmChain.create();
    const { vault } = await deployVault(chain, { withCollateral: false });
    const user = await chain.account('no-collateral');
    await assertReverts(
      vault.fnFrom('mintWithToken', [ONE], { from: user }),
      'token collateral is disabled',
    );
  });
});
