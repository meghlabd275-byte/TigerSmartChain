// Staking suites: StakingPool (reward accrual, lock, early-exit penalty) and
// LockManager (linear vesting).
//
// LockManager is a regression target: `release` and `revokeLock` used to update
// the accounting numbers without ever moving tokens, so a beneficiary could
// never actually withdraw.

import { describe, it, assert, assertBigIntEqual, assertAddressEqual, assertReverts } from '../test-harness.mjs';
import { EvmChain, Contract, loadArtifact } from '../evm-chain.mjs';

const SUPPLY = 10_000_000n * 10n ** 18n;
const MIN_STAKE = 100n * 10n ** 18n;
const LOCK_AMOUNT = 1000n * 10n ** 18n;
const DAY = 86_400n;

async function deployTokens(chain) {
  const stake = await chain.deploy('TEP126/TEP126:TEP126', [
    'Stake Token',
    'STK',
    18,
    SUPPLY,
    SUPPLY,
    '1',
  ]);
  const reward = await chain.deploy('TEP126/TEP126:TEP126', [
    'Reward Token',
    'RWD',
    18,
    SUPPLY,
    SUPPLY,
    '1',
  ]);
  return { stake, reward };
}

async function deployPool(chain, { rewardsDuration = 30n * DAY } = {}) {
  const { stake, reward } = await deployTokens(chain);
  const pool = await chain.deploy('staking/StakingPool:StakingPool', [
    stake.address,
    reward.address,
    rewardsDuration,
  ]);
  return { pool, stake, reward };
}

/** Moves `amount` of `token` from the owner to `to`. */
async function fund(token, chain, to, amount) {
  await token.fn('transfer', to, amount);
}

describe('StakingPool', () => {
  it('reports its constructor configuration', async () => {
    const chain = await EvmChain.create();
    const { pool, stake, reward } = await deployPool(chain);

    assertAddressEqual(await pool.value('stakingToken'), stake.address);
    assertAddressEqual(await pool.value('rewardToken'), reward.address);
    assertBigIntEqual(await pool.value('minStakeAmount'), MIN_STAKE);
    assertBigIntEqual(await pool.value('lockPeriod'), 90n * DAY);
    assertBigIntEqual(await pool.value('earlyPenalty'), 500n);
    assertBigIntEqual(await pool.value('totalStaked'), 0n);
  });

  it('rejects a zero token address', async () => {
    const chain = await EvmChain.create();
    const zero = '0x' + '00'.repeat(20);

    await assertReverts(
      chain.deploy('staking/StakingPool:StakingPool', [zero, zero, 1n]),
      'invalid staking token',
    );
  });

  it('stakes, records the unlock time and holds the tokens', async () => {
    const chain = await EvmChain.create();
    const { pool, stake } = await deployPool(chain);
    const alice = await chain.account('alice');

    await fund(stake, chain, alice, MIN_STAKE);
    await stake.fnFrom('approve', [pool.address, MIN_STAKE], { from: alice });

    const { logs } = await pool.fnFrom('stake', [MIN_STAKE], { from: alice });
    assert(logs.some((l) => l.name === 'Staked'), 'Staked was not emitted');

    assertBigIntEqual(await pool.value('totalStaked'), MIN_STAKE);
    assertBigIntEqual(await stake.value('balanceOf', pool.address), MIN_STAKE);
    assertBigIntEqual(await stake.value('balanceOf', alice), 0n);

    const [staked, , , unlockTime] = await pool.values('stakerInfo', alice);
    assertBigIntEqual(staked, MIN_STAKE);
    assertBigIntEqual(BigInt(unlockTime), chain.timestamp + 90n * DAY);
  });

  it('rejects a stake below the minimum', async () => {
    const chain = await EvmChain.create();
    const { pool } = await deployPool(chain);
    const alice = await chain.account('alice');

    await assertReverts(
      pool.fnFrom('stake', [MIN_STAKE - 1n], { from: alice }),
      'amount below minimum',
    );
  });

  it('enforces the maximum stake per account', async () => {
    const chain = await EvmChain.create();
    const { pool, stake } = await deployPool(chain);
    const alice = await chain.account('alice');

    await pool.fn('setStakeLimits', 1n, 200n * 10n ** 18n);
    await fund(stake, chain, alice, 300n * 10n ** 18n);
    await stake.fnFrom('approve', [pool.address, 300n * 10n ** 18n], { from: alice });

    await pool.fnFrom('stake', [200n * 10n ** 18n], { from: alice });
    await assertReverts(
      pool.fnFrom('stake', [1n], { from: alice }),
      'amount exceeds maximum',
    );
  });

  it('accrues rewards over time and pays them on claim', async () => {
    const chain = await EvmChain.create();
    const { pool, stake, reward } = await deployPool(chain);
    const alice = await chain.account('alice');
    const rewardPool = 1_000n * 10n ** 18n;

    await fund(stake, chain, alice, MIN_STAKE);
    await stake.fnFrom('approve', [pool.address, MIN_STAKE], { from: alice });
    await pool.fnFrom('stake', [MIN_STAKE], { from: alice });

    await reward.fn('approve', pool.address, rewardPool);
    await pool.fn('addRewards', rewardPool);

    const rate = await pool.value('rewardRate');
    assert(BigInt(rate) > 0n, 'rewardRate must be non-zero after funding');

    // Half the rewards duration later, roughly half the pool is earned.
    await chain.mine(1, { secondsPerBlock: Number(30n * DAY) / 2 });
    const earned = BigInt(await pool.value('earnedRewards', alice));
    assert(earned > 0n, 'expected rewards to accrue');

    const before = await reward.value('balanceOf', alice);
    await pool.fnFrom('claimReward', [], { from: alice });
    const after = await reward.value('balanceOf', alice);

    assertBigIntEqual(after - before, earned);
    assertBigIntEqual(await pool.value('earnedRewards', alice), 0n);
  });

  it('refuses to claim when nothing was earned', async () => {
    const chain = await EvmChain.create();
    const { pool } = await deployPool(chain);
    const alice = await chain.account('alice');

    await assertReverts(pool.fnFrom('claimReward', [], { from: alice }), 'no rewards');
  });

  it('returns the full stake once the lock period has passed', async () => {
    const chain = await EvmChain.create();
    const { pool, stake } = await deployPool(chain);
    const alice = await chain.account('alice');

    await fund(stake, chain, alice, MIN_STAKE);
    await stake.fnFrom('approve', [pool.address, MIN_STAKE], { from: alice });
    await pool.fnFrom('stake', [MIN_STAKE], { from: alice });

    await chain.mine(1, { secondsPerBlock: 91 * 86_400 });
    const { logs } = await pool.fnFrom('unstake', [MIN_STAKE], { from: alice });

    const unstaked = logs.find((l) => l.name === 'Unstaked');
    assertBigIntEqual(unstaked.args[2], 0n);
    assertBigIntEqual(await stake.value('balanceOf', alice), MIN_STAKE);
    assertBigIntEqual(await pool.value('totalStaked'), 0n);
  });

  it('charges the early-exit penalty and leaves it with the pool', async () => {
    const chain = await EvmChain.create();
    const { pool, stake } = await deployPool(chain);
    const alice = await chain.account('alice');

    await fund(stake, chain, alice, MIN_STAKE);
    await stake.fnFrom('approve', [pool.address, MIN_STAKE], { from: alice });
    await pool.fnFrom('stake', [MIN_STAKE], { from: alice });

    const { logs } = await pool.fnFrom('unstake', [MIN_STAKE], { from: alice });
    const penalty = (MIN_STAKE * 500n) / 10_000n;

    const unstaked = logs.find((l) => l.name === 'Unstaked');
    assertBigIntEqual(unstaked.args[2], penalty);
    assertBigIntEqual(await stake.value('balanceOf', alice), MIN_STAKE - penalty);
    assertBigIntEqual(await stake.value('balanceOf', pool.address), penalty);
  });

  it('rejects an unstake above the staked balance', async () => {
    const chain = await EvmChain.create();
    const { pool } = await deployPool(chain);
    const alice = await chain.account('alice');

    await assertReverts(
      pool.fnFrom('unstake', [MIN_STAKE], { from: alice }),
      'insufficient stake',
    );
  });

  it('caps the early penalty at 20%', async () => {
    const chain = await EvmChain.create();
    const { pool } = await deployPool(chain);

    await assertReverts(pool.fn('setEarlyPenalty', 2001n), 'penalty too high');
    await pool.fn('setEarlyPenalty', 2000);
    assertBigIntEqual(await pool.value('earlyPenalty'), 2000n);
  });

  it('restricts configuration to the admin role', async () => {
    const chain = await EvmChain.create();
    const { pool } = await deployPool(chain);
    const mallory = await chain.account('mallory');

    await assertReverts(
      pool.fnFrom('setLockPeriod', [1n], { from: mallory }),
      'AccessControlUnauthorizedAccount',
    );
    await assertReverts(
      pool.fnFrom('addRewards', [1n], { from: mallory }),
      'AccessControlUnauthorizedAccount',
    );
  });

  it('updates the lock period for future stakes', async () => {
    const chain = await EvmChain.create();
    const { pool, stake } = await deployPool(chain);
    const alice = await chain.account('alice');

    await pool.fn('setLockPeriod', 1n);
    await fund(stake, chain, alice, MIN_STAKE);
    await stake.fnFrom('approve', [pool.address, MIN_STAKE], { from: alice });
    await pool.fnFrom('stake', [MIN_STAKE], { from: alice });

    const [, , , unlockTime, canUnstake] = await pool.values('stakerInfo', alice);
    assertBigIntEqual(BigInt(unlockTime), chain.timestamp + 1n);
    assertEqualBoolean(canUnstake, false);
  });
});

describe('LockManager', () => {
  async function deploy(chain) {
    const token = await chain.deploy('TEP126/TEP126:TEP126', [
      'Lock Token',
      'LCK',
      18,
      SUPPLY,
      SUPPLY,
      '1',
    ]);
    const manager = await chain.deploy('staking/LockManager:LockManager', [token.address]);
    return { token, manager };
  }

  it('creates a lock and escrows the tokens', async () => {
    const chain = await EvmChain.create();
    const { token, manager } = await deploy(chain);
    const alice = await chain.account('alice');

    await token.fn('approve', manager.address, LOCK_AMOUNT);
    await manager.fn('createLock', [alice, LOCK_AMOUNT, 30n * DAY, 1]);

    const ids = await manager.value('getLocksForBeneficiary', alice);
    assertBigIntEqual(ids.length, 1n);
    assertBigIntEqual(await manager.value('totalLocked'), LOCK_AMOUNT);
    assertBigIntEqual(await manager.value('getLockedAmount', alice), LOCK_AMOUNT);
  });

  it('rejects an amount below the minimum and a bad duration', async () => {
    const chain = await EvmChain.create();
    const { manager } = await deploy(chain);
    const alice = await chain.account('alice');

    await assertReverts(
      manager.fn('createLock', [alice, 10n ** 17n, 30n * DAY, 1]),
      'InvalidAmount',
    );
    await assertReverts(
      manager.fn('createLock', [alice, LOCK_AMOUNT, 3600n, 1]),
      'InvalidDuration',
    );
    await assertReverts(
      manager.fn('createLock', ['0x' + '00'.repeat(20), LOCK_AMOUNT, 30n * DAY, 1]),
      'InvalidBeneficiary',
    );
  });

  it('refuses to release before anything has vested', async () => {
    const chain = await EvmChain.create();
    const { token, manager } = await deploy(chain);
    const alice = await chain.account('alice');

    await token.fn('approve', manager.address, LOCK_AMOUNT);
    await manager.fn('createLock', [alice, LOCK_AMOUNT, 30n * DAY, 1]);
    const [lockId] = await manager.value('getLocksForBeneficiary', alice);

    await assertReverts(manager.fn('release', [lockId]), 'StillLocked');
  });

  it('pays out the vested share on release', async () => {
    const chain = await EvmChain.create();
    const { token, manager } = await deploy(chain);
    const alice = await chain.account('alice');

    await token.fn('approve', manager.address, LOCK_AMOUNT);
    await manager.fn('createLock', [alice, LOCK_AMOUNT, 100n * DAY, 1]);
    const [lockId] = await manager.value('getLocksForBeneficiary', alice);

    // Half of the schedule.
    await chain.mine(1, { secondsPerBlock: Number(50n * DAY) });
    const releasable = BigInt(await manager.value('getReleasableAmount', lockId));
    assert(releasable > 0n, 'expected a vested amount');

    const before = await token.value('balanceOf', alice);
    const { logs } = await manager.fn('release', [lockId]);
    const after = await token.value('balanceOf', alice);

    // Tokens actually move; this is the regression this suite guards.
    assertBigIntEqual(after - before, releasable);
    assert(logs.some((l) => l.name === 'LockReleased'), 'LockReleased was not emitted');
    assertBigIntEqual(await manager.value('getReleasableAmount', lockId), 0n);
  });

  it('releases the remainder once the schedule completes', async () => {
    const chain = await EvmChain.create();
    const { token, manager } = await deploy(chain);
    const alice = await chain.account('alice');

    await token.fn('approve', manager.address, LOCK_AMOUNT);
    await manager.fn('createLock', [alice, LOCK_AMOUNT, 10n * DAY, 1]);
    const [lockId] = await manager.value('getLocksForBeneficiary', alice);

    await chain.mine(1, { secondsPerBlock: Number(11n * DAY) });
    await manager.fn('release', [lockId]);
    assertBigIntEqual(await token.value('balanceOf', alice), LOCK_AMOUNT);
    assertBigIntEqual(await manager.value('totalLocked'), 0n);
  });

  it('pays the vested part to the beneficiary and returns the rest on revoke', async () => {
    const chain = await EvmChain.create();
    const { token, manager } = await deploy(chain);
    const alice = await chain.account('alice');

    await token.fn('approve', manager.address, LOCK_AMOUNT);
    await manager.fn('createLock', [alice, LOCK_AMOUNT, 100n * DAY, 1]);
    const [lockId] = await manager.value('getLocksForBeneficiary', alice);

    await chain.mine(1, { secondsPerBlock: Number(25n * DAY) });
    const releasable = BigInt(await manager.value('getReleasableAmount', lockId));

    const ownerBefore = await token.value('balanceOf', chain.owner);
    await manager.fn('revokeLock', [lockId]);
    const ownerAfter = await token.value('balanceOf', chain.owner);

    assertBigIntEqual(await token.value('balanceOf', alice), releasable);
    assertBigIntEqual(ownerAfter - ownerBefore, LOCK_AMOUNT - releasable);
    assertBigIntEqual(await manager.value('totalLocked'), 0n);
  });

  it('blocks release and revoke while the emergency stop is engaged', async () => {
    const chain = await EvmChain.create();
    const { token, manager } = await deploy(chain);
    const alice = await chain.account('alice');

    await token.fn('approve', manager.address, LOCK_AMOUNT);
    await manager.fn('createLock', [alice, LOCK_AMOUNT, 10n * DAY, 1]);
    const [lockId] = await manager.value('getLocksForBeneficiary', alice);

    await manager.fn('emergencyStop', true);
    assertRevertsName(await captureError(() => manager.fn('release', [lockId])), 'ContractStopped');
    await manager.fn('emergencyStop', false);
    await chain.mine(1, { secondsPerBlock: Number(11n * DAY) });
    await manager.fn('release', [lockId]);
  });

  it('creates a batch of locks in one call', async () => {
    const chain = await EvmChain.create();
    const { token, manager } = await deploy(chain);
    const a = await chain.account('a');
    const b = await chain.account('b');
    const amount = 1000n * 10n ** 18n;

    await token.fn('approve', manager.address, amount * 2n);
    await manager.fn('createMultipleLocks', [
      [a, b],
      [amount, amount],
      [30n * DAY, 60n * DAY],
    ]);

    assertBigIntEqual(await manager.value('getLockedAmount', a), amount);
    assertBigIntEqual(await manager.value('getLockedAmount', b), amount);
    assertBigIntEqual(await manager.value('totalLocked'), amount * 2n);
  });

  it('honours the cliff before releasing', async () => {
    const chain = await EvmChain.create();
    const { token, manager } = await deploy(chain);
    const alice = await chain.account('alice');

    await manager.fn('setDefaultCliffPeriod', 10n * DAY);
    await token.fn('approve', manager.address, LOCK_AMOUNT);
    // Lock type 2 == Cliff.
    await manager.fn('createLock', [alice, LOCK_AMOUNT, 100n * DAY, 2]);
    const [lockId] = await manager.value('getLocksForBeneficiary', alice);

    await chain.mine(1, { secondsPerBlock: Number(5n * DAY) });
    assertBigIntEqual(await manager.value('getReleasableAmount', lockId), 0n);
    await assertReverts(manager.fn('release', [lockId]), 'StillLocked');

    await chain.mine(1, { secondsPerBlock: Number(6n * DAY) });
    assert(BigInt(await manager.value('getReleasableAmount', lockId)) > 0n);
  });
});

function assertEqualBoolean(actual, expected) {
  assert(Boolean(actual) === expected, `expected ${expected} but got ${actual}`);
}

function assertRevertsName(message, name) {
  assert(message.includes(name), `expected revert "${name}" but got: ${message}`);
}

async function captureError(fn) {
  try {
    await fn();
  } catch (error) {
    return error.message;
  }
  throw new Error('expected the call to revert');
}
