// MultiSigWallet and Treasury: M-of-N approval with real value movement.

import {
  describe,
  it,
  assert,
  assertEqual,
  assertBigIntEqual,
  assertAddressEqual,
  assertReverts,
} from '../test-harness.mjs';
import { EvmChain } from '../evm-chain.mjs';

describe('MultiSigWallet', () => {
  async function deploy(chain, { owners, required }) {
    return chain.deploy('bep20/MultiSigWallet:MultiSigWallet', [owners, required]);
  }

  async function twoOfThree(chain) {
    const a = await chain.account('owner-a');
    const b = await chain.account('owner-b');
    const c = await chain.account('owner-c');
    const wallet = await deploy(chain, { owners: [a, b, c], required: 2n });
    await chain.fund(wallet.address, 10n ** 18n);
    return { wallet, a, b, c };
  }

  it('records the owners and the requirement', async () => {
    const chain = await EvmChain.create();
    const { wallet, a, b, c } = await twoOfThree(chain);

    assertBigIntEqual(await wallet.value('getOwnerCount'), 3n);
    assertBigIntEqual(await wallet.value('required'), 2n);
    for (const owner of [a, b, c]) {
      assertEqual(await wallet.value('owners', owner), true);
    }
    const listed = await wallet.value('getOwners');
    assertEqual(listed.map((x) => x.toLowerCase()).join(','), [a, b, c].map((x) => x.toLowerCase()).join(','));
  });

  it('rejects a zero owner, a zero requirement and an impossible requirement', async () => {
    const chain = await EvmChain.create();
    const a = await chain.account('a');
    const zero = '0x' + '00'.repeat(20);

    await assertReverts(
      chain.deploy('bep20/MultiSigWallet:MultiSigWallet', [[], 1n]),
      'no owners',
    );
    await assertReverts(
      chain.deploy('bep20/MultiSigWallet:MultiSigWallet', [[a], 0n]),
      'required > 0',
    );
    await assertReverts(
      chain.deploy('bep20/MultiSigWallet:MultiSigWallet', [[a], 2n]),
      'required > owners',
    );
    await assertReverts(
      chain.deploy('bep20/MultiSigWallet:MultiSigWallet', [[a, zero], 1n]),
      'ZeroAddress',
    );
  });

  it('refuses to submit from a non-owner', async () => {
    const chain = await EvmChain.create();
    const { wallet, b } = await twoOfThree(chain);
    const mallory = await chain.account('mallory');

    await assertReverts(
      wallet.fnFrom('submitTransaction', [b, 1n, '0x'], { from: mallory }),
      'NotOwner',
    );
  });

  it('rejects a transaction that carries neither value nor data', async () => {
    const chain = await EvmChain.create();
    const { wallet, a, b } = await twoOfThree(chain);

    await assertReverts(
      wallet.fnFrom('submitTransaction', [b, 0n, '0x'], { from: a }),
      'InvalidValue',
    );
    await assertReverts(
      wallet.fnFrom('submitTransaction', ['0x' + '00'.repeat(20), 1n, '0x'], { from: a }),
      'ZeroAddress',
    );
  });

  it('auto-confirms the submitter and requires a second confirmation', async () => {
    const chain = await EvmChain.create();
    const { wallet, a, b } = await twoOfThree(chain);

    await wallet.fnFrom('submitTransaction', [b, 10n ** 17n, '0x'], { from: a });
    assertBigIntEqual(await wallet.value('getConfirmationCount', 0n), 1n);
    assertEqual(await wallet.value('isConfirmed', 0n, a), true);

    await assertReverts(
      wallet.fnFrom('executeTransaction', [0n], { from: a }),
      'InsufficientConfirmations',
    );

    await wallet.fnFrom('confirmTransaction', [0n], { from: b });
    assertBigIntEqual(await wallet.value('getConfirmationCount', 0n), 2n);
  });

  it('executes a payment once enough owners confirm', async () => {
    const chain = await EvmChain.create();
    const { wallet, a, b } = await twoOfThree(chain);
    const recipient = await chain.account('recipient');

    const before = await chain.balanceOf(recipient);
    await wallet.fnFrom('submitTransaction', [recipient, 10n ** 17n, '0x'], { from: a });
    await wallet.fnFrom('confirmTransaction', [0n], { from: b });
    await wallet.fnFrom('executeTransaction', [0n], { from: a });

    assertBigIntEqual((await chain.balanceOf(recipient)) - before, 10n ** 17n);
    assertBigIntEqual(await chain.balanceOf(wallet.address), 10n ** 18n - 10n ** 17n);

    const tx = await wallet.values('getTransaction', 0n);
    assertEqual(tx[4], true);
  });

  it('blocks a second confirmation from the same owner', async () => {
    const chain = await EvmChain.create();
    const { wallet, a, b } = await twoOfThree(chain);

    await wallet.fnFrom('submitTransaction', [b, 1n, '0x'], { from: a });
    await assertReverts(
      wallet.fnFrom('confirmTransaction', [0n], { from: a }),
      'AlreadyConfirmed',
    );
  });

  it('cannot execute the same transaction twice', async () => {
    const chain = await EvmChain.create();
    const { wallet, a, b } = await twoOfThree(chain);

    await wallet.fnFrom('submitTransaction', [b, 1n, '0x'], { from: a });
    await wallet.fnFrom('confirmTransaction', [0n], { from: b });
    await wallet.fnFrom('executeTransaction', [0n], { from: a });

    await assertReverts(
      wallet.fnFrom('executeTransaction', [0n], { from: b }),
      'AlreadyExecuted',
    );
  });

  it('cancels a transaction before it executes', async () => {
    const chain = await EvmChain.create();
    const { wallet, a, b } = await twoOfThree(chain);

    await wallet.fnFrom('submitTransaction', [b, 1n, '0x'], { from: a });
    await wallet.fnFrom('cancelTransaction', [0n], { from: b });
    await assertReverts(
      wallet.fnFrom('confirmTransaction', [0n], { from: b }),
      'AlreadyCancelled',
    );
    await assertReverts(
      wallet.fnFrom('executeTransaction', [0n], { from: a }),
      'AlreadyCancelled',
    );
  });

  it('reverts the whole transaction when the inner call fails', async () => {
    const chain = await EvmChain.create();
    const { wallet, a, b } = await twoOfThree(chain);

    // The wallet holds 1 ether, so asking for more must fail.
    await wallet.fnFrom('submitTransaction', [b, 5n * 10n ** 18n, '0x'], { from: a });
    await wallet.fnFrom('confirmTransaction', [0n], { from: b });
    await assertReverts(
      wallet.fnFrom('executeTransaction', [0n], { from: a }),
      'ExecutionFailed',
    );
    // The failure rolled back the executed flag, so the owners can retry.
    const tx = await wallet.values('getTransaction', 0n);
    assertEqual(tx[4], false);
  });

  it('adds and removes owners through the multisig itself', async () => {
    const chain = await EvmChain.create();
    const { wallet, a, b } = await twoOfThree(chain);
    const newcomer = await chain.account('newcomer');

    const data = wallet.encode('addOwner', [newcomer]);
    await wallet.fnFrom('submitTransaction', [wallet.address, 0n, data], { from: a });
    await wallet.fnFrom('confirmTransaction', [0n], { from: b });
    await wallet.fnFrom('executeTransaction', [0n], { from: a });

    assertEqual(await wallet.value('owners', newcomer), true);
    assertBigIntEqual(await wallet.value('getOwnerCount'), 4n);

    const remove = wallet.encode('removeOwner', [newcomer]);
    await wallet.fnFrom('submitTransaction', [wallet.address, 0n, remove], { from: a });
    await wallet.fnFrom('confirmTransaction', [1n], { from: b });
    await wallet.fnFrom('executeTransaction', [1n], { from: a });

    assertEqual(await wallet.value('owners', newcomer), false);
    assertBigIntEqual(await wallet.value('getOwnerCount'), 3n);
  });

  it('lists the pending transactions', async () => {
    const chain = await EvmChain.create();
    const { wallet, a, b } = await twoOfThree(chain);

    await wallet.fnFrom('submitTransaction', [b, 1n, '0x'], { from: a });
    await wallet.fnFrom('submitTransaction', [b, 2n, '0x'], { from: a });

    const pending = await wallet.value('getPendingTransactions');
    assertBigIntEqual(pending.length, 2n);
  });

  it('deploys wallets through the factory', async () => {
    const chain = await EvmChain.create();
    const factory = await chain.deploy('bep20/MultiSigWallet:MultiSigWalletFactory');
    const a = await chain.account('a');
    const b = await chain.account('b');

    const { logs } = await factory.fn('createWallet', [[a, b], 2n]);
    const created = logs.find((l) => l.name === 'WalletCreated');
    assert(created, 'WalletCreated was not emitted');

    const { Contract, loadArtifact } = await import('../evm-chain.mjs');
    const artifact = await loadArtifact('bep20/MultiSigWallet:MultiSigWallet');
    const wallet = new Contract(chain, created.args[0], artifact.abi, 'MultiSigWallet');
    assertBigIntEqual(await wallet.value('required'), 2n);
    assertEqual(await wallet.value('owners', a), true);
  });
});

describe('Treasury', () => {
  async function deploy(chain) {
    const a = await chain.account('signer-a');
    const b = await chain.account('signer-b');
    const c = await chain.account('signer-c');
    const treasury = await chain.deploy('treasury/Treasury:Treasury', [[a, b, c], 2n]);
    await chain.fund(treasury.address, 10n ** 18n);
    return { treasury, a, b, c };
  }

  it('records the signers and the confirmation requirement', async () => {
    const chain = await EvmChain.create();
    const { treasury, a } = await deploy(chain);

    assertBigIntEqual(await treasury.value('getSignersCount'), 3n);
    assertBigIntEqual(await treasury.value('requiredConfirmations'), 2n);
    assertEqual(await treasury.value('isSigner', a), true);
    assertBigIntEqual(await treasury.value('getTransactionCount'), 0n);
  });

  it('rejects a deposit from a non-signer', async () => {
    const chain = await EvmChain.create();
    const { treasury, a } = await deploy(chain);
    const mallory = await chain.account('mallory');

    await assertReverts(
      treasury.fnFrom('submitTransaction', [a, 1n, '0x'], { from: mallory }),
      'Not a signer',
    );
  });

  it('executes a payment after the required confirmations', async () => {
    const chain = await EvmChain.create();
    const { treasury, a, b } = await deploy(chain);
    const recipient = await chain.account('recipient');

    const before = await chain.balanceOf(recipient);
    await treasury.fnFrom('submitTransaction', [recipient, 10n ** 17n, '0x'], { from: a });
    assertBigIntEqual(await treasury.value('getConfirmationCount', 0n), 1n);

    await assertReverts(
      treasury.fnFrom('executeTransaction', [0n], { from: a }),
      'Not enough confirmations',
    );

    await treasury.fnFrom('confirmTransaction', [0n], { from: b });
    await treasury.fnFrom('executeTransaction', [0n], { from: a });

    assertBigIntEqual((await chain.balanceOf(recipient)) - before, 10n ** 17n);
  });

  it('revokes a confirmation and blocks execution again', async () => {
    const chain = await EvmChain.create();
    const { treasury, a, b } = await deploy(chain);

    await treasury.fnFrom('submitTransaction', [b, 10n ** 17n, '0x'], { from: a });
    await treasury.fnFrom('confirmTransaction', [0n], { from: b });
    await treasury.fnFrom('revokeConfirmation', [0n], { from: b });

    assertBigIntEqual(await treasury.value('getConfirmationCount', 0n), 1n);
    await assertReverts(
      treasury.fnFrom('executeTransaction', [0n], { from: a }),
      'Not enough confirmations',
    );
  });

  it('blocks a double confirmation and a double execution', async () => {
    const chain = await EvmChain.create();
    const { treasury, a, b } = await deploy(chain);

    await treasury.fnFrom('submitTransaction', [b, 10n ** 17n, '0x'], { from: a });
    await assertReverts(
      treasury.fnFrom('confirmTransaction', [0n], { from: a }),
      'Already confirmed',
    );

    await treasury.fnFrom('confirmTransaction', [0n], { from: b });
    await treasury.fnFrom('executeTransaction', [0n], { from: a });
    await assertReverts(
      treasury.fnFrom('executeTransaction', [0n], { from: b }),
      'Already executed',
    );
  });

  it('reports the signer list and the confirmation set', async () => {
    const chain = await EvmChain.create();
    const { treasury, a, b } = await deploy(chain);

    const signers = await treasury.value('getSigners');
    assertBigIntEqual(signers.length, 3n);

    await treasury.fnFrom('submitTransaction', [b, 10n ** 17n, '0x'], { from: a });
    const confirmations = await treasury.value('getConfirmations', 0n);
    assertBigIntEqual(confirmations.length, 1n);
    assertAddressEqual(confirmations[0], a);
  });

  it('rejects a transaction that carries neither value nor data', async () => {
    const chain = await EvmChain.create();
    const { treasury, a } = await deploy(chain);

    await assertReverts(
      treasury.fnFrom('submitTransaction', [a, 0n, '0x'], { from: a }),
      'Invalid transaction',
    );
  });
});
