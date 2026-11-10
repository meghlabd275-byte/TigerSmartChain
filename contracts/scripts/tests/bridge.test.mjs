// Bridge suites: TokenHub (lock/mint with relayer signatures) and
// TGRAutoBridge (liquidity-backed escrow with oracle settlement).
//
// Both contracts had holes that let value move without authorisation, so the
// negative cases here are the point of the suite.

import { ethers } from 'ethers';
import {
  describe,
  it,
  assert,
  assertEqual,
  assertBigIntEqual,
  assertReverts,
} from '../test-harness.mjs';
import { EvmChain } from '../evm-chain.mjs';

const SUPPLY = 10_000_000n * 10n ** 18n;
const DAY = 86_400n;
const NATIVE = '0x' + '00'.repeat(20);

async function deployToken(chain, name = 'Bridge Token', symbol = 'BRG') {
  return chain.deploy('TEP126/TEP126:TEP126', [name, symbol, 18, SUPPLY, SUPPLY, '1']);
}

/** Signs a TokenHub inbound transfer exactly as the contract does. */
async function signRelay(chain, hub, signerKey, params) {
  const message = ethers.keccak256(
    ethers.solidityPacked(
      ['address', 'address', 'address', 'uint256', 'uint16', 'uint64', 'bytes32', 'uint256'],
      [
        params.token,
        params.sender,
        params.recipient,
        params.amount,
        params.fromChain,
        params.nonce,
        params.transferId,
        BigInt(chain.common.chainId()),
      ],
    ),
  );
  return new ethers.Wallet(signerKey).signMessage(ethers.getBytes(message));
}

describe('TokenHub', () => {
  async function deploy(chain) {
    const token = await deployToken(chain);
    const hub = await chain.deploy('bridge/TokenHub:TokenHub', [6666n]);
    const relayer = await chain.account('relayer');
    await hub.fn('setTokenSupport', [token.address, true]);
    await hub.fn('setPeer', [56n, token.address, chain.owner]);
    await hub.fn('addRelayer', relayer);
    return { token, hub, relayer };
  }

  it('records the chain id and the default limits', async () => {
    const chain = await EvmChain.create();
    const { hub } = await deploy(chain);

    assertBigIntEqual(await hub.value('chainId'), 6666n);
    assertBigIntEqual(await hub.value('minTransferAmount'), 1n);
    assertBigIntEqual(await hub.value('relayFee'), 0n);
    assertBigIntEqual(await hub.value('maxTransferAmount'), 2n ** 256n - 1n);
  });

  it('locks tokens on the source chain and emits the transfer id', async () => {
    const chain = await EvmChain.create();
    const { token, hub } = await deploy(chain);
    const alice = await chain.account('alice');
    const amount = 100n * 10n ** 18n;

    await token.fn('transfer', alice, amount);
    await token.fnFrom('approve', [hub.address, amount], { from: alice });

    const { logs } = await hub.fnFrom('sendToken', [token.address, 56n, alice, amount], {
      from: alice,
    });
    const sent = logs.find((l) => l.name === 'TokenSent');
    assert(sent, 'TokenSent was not emitted');

    // The tokens are held by the hub, not burned.
    assertBigIntEqual(await token.value('balanceOf', hub.address), amount);
    assertBigIntEqual(await token.value('balanceOf', alice), 0n);
    assertBigIntEqual(await hub.value('getNonce', alice), 1n);

    const expected = await hub.value(
      'generateTransferId',
      token.address,
      alice,
      alice,
      amount,
      56n,
      0n,
    );
    assertEqual(sent.args[6], expected);
  });

  it('increments the nonce so repeated sends get distinct ids', async () => {
    const chain = await EvmChain.create();
    const { token, hub } = await deploy(chain);
    const alice = await chain.account('alice');
    const amount = 100n * 10n ** 18n;

    await token.fn('transfer', alice, amount * 2n);
    await token.fnFrom('approve', [hub.address, amount * 2n], { from: alice });

    const first = await hub.fnFrom('sendToken', [token.address, 56n, alice, amount], {
      from: alice,
    });
    const second = await hub.fnFrom('sendToken', [token.address, 56n, alice, amount], {
      from: alice,
    });

    const idOf = (r) => r.logs.find((l) => l.name === 'TokenSent').args[6];
    assert(idOf(first) !== idOf(second), 'transfer ids must differ');
  });

  it('rejects an unsupported token, a missing peer and a zero recipient', async () => {
    const chain = await EvmChain.create();
    const { token, hub } = await deploy(chain);
    const other = await deployToken(chain, 'Other', 'OTH');
    const alice = await chain.account('alice');
    const zero = '0x' + '00'.repeat(20);

    await assertReverts(
      hub.fn('sendToken', [other.address, 56n, alice, 1n]),
      'Token not supported',
    );
    await assertReverts(
      hub.fn('sendToken', [token.address, 1n, alice, 1n]),
      'Peer not set',
    );
    await assertReverts(
      hub.fn('sendToken', [token.address, 56n, zero, 1n]),
      'Invalid recipient',
    );
  });

  it('enforces the configured transfer bounds and relay fee', async () => {
    const chain = await EvmChain.create();
    const { token, hub } = await deploy(chain);
    const alice = await chain.account('alice');

    await hub.fn('setMinTransferAmount', 10n);
    await hub.fn('setMaxTransferAmount', 100n);
    await assertReverts(
      hub.fn('sendToken', [token.address, 56n, alice, 9n]),
      'Amount too low',
    );
    await assertReverts(
      hub.fn('sendToken', [token.address, 56n, alice, 101n]),
      'Amount too high',
    );

    await hub.fn('setRelayFee', 10n ** 15n);
    await assertReverts(
      hub.fn('sendToken', [token.address, 56n, alice, 50n]),
      'Insufficient fee',
    );
  });

  it('relays an inbound transfer when the relayer signature is valid', async () => {
    const chain = await EvmChain.create();
    const { token, hub, relayer } = await deploy(chain);
    const alice = await chain.account('alice');
    const amount = 250n * 10n ** 18n;

    // The hub needs a balance to pay the recipient with.
    await token.fn('transfer', hub.address, amount);

    const transferId = ethers.id('transfer-1');
    const params = {
      token: token.address,
      sender: alice,
      recipient: alice,
      amount,
      fromChain: 56n,
      nonce: 0n,
      transferId,
    };
    const signature = await signRelay(chain, hub, await chain.privateKeyOf(relayer), params);

    const { logs } = await hub.fnFrom(
      'relayToken',
      [
        params.token,
        params.sender,
        params.recipient,
        params.amount,
        params.fromChain,
        params.nonce,
        params.transferId,
        signature,
      ],
      { from: relayer },
    );

    assert(logs.some((l) => l.name === 'TokenReceived'), 'TokenReceived was not emitted');
    assertBigIntEqual(await token.value('balanceOf', alice), amount);
    assertEqual(await hub.value('transferRecords', transferId), true);
  });

  it('rejects a relay with no signature and with a forged signature', async () => {
    const chain = await EvmChain.create();
    const { token, hub, relayer } = await deploy(chain);
    const alice = await chain.account('alice');
    const amount = 100n * 10n ** 18n;

    await token.fn('transfer', hub.address, amount);
    const transferId = ethers.id('transfer-2');
    const params = {
      token: token.address,
      sender: alice,
      recipient: alice,
      amount,
      fromChain: 56n,
      nonce: 0n,
      transferId,
    };
    const base = [
      params.token,
      params.sender,
      params.recipient,
      params.amount,
      params.fromChain,
      params.nonce,
      params.transferId,
    ];

    await assertReverts(
      hub.fnFrom('relayToken', [...base, '0x'], { from: relayer }),
      'ECDSA',
    );

    // Signed by an account that is not a relayer.
    const impostor = await chain.account('impostor');
    const forged = await signRelay(chain, hub, await chain.privateKeyOf(impostor), params);
    await assertReverts(
      hub.fnFrom('relayToken', [...base, forged], { from: relayer }),
      'Invalid relayer signature',
    );

    // A relayer signature over different parameters must not authorise this one.
    const mismatched = await signRelay(chain, hub, await chain.privateKeyOf(relayer), {
      ...params,
      amount: amount + 1n,
    });
    await assertReverts(
      hub.fnFrom('relayToken', [...base, mismatched], { from: relayer }),
      'Invalid relayer signature',
    );
  });

  it('blocks a relay from a non-relayer and a replayed transfer id', async () => {
    const chain = await EvmChain.create();
    const { token, hub, relayer } = await deploy(chain);
    const alice = await chain.account('alice');
    const amount = 100n * 10n ** 18n;

    await token.fn('transfer', hub.address, amount * 2n);
    const transferId = ethers.id('transfer-3');
    const params = {
      token: token.address,
      sender: alice,
      recipient: alice,
      amount,
      fromChain: 56n,
      nonce: 0n,
      transferId,
    };
    const signature = await signRelay(chain, hub, await chain.privateKeyOf(relayer), params);
    const args = [
      params.token,
      params.sender,
      params.recipient,
      params.amount,
      params.fromChain,
      params.nonce,
      params.transferId,
      signature,
    ];

    await assertReverts(hub.fnFrom('relayToken', args, { from: alice }), 'AccessControlUnauthorizedAccount');

    await hub.fnFrom('relayToken', args, { from: relayer });
    await assertReverts(
      hub.fnFrom('relayToken', args, { from: relayer }),
      'Already relayed',
    );
  });

  it('stops accepting a removed relayer', async () => {
    const chain = await EvmChain.create();
    const { token, hub, relayer } = await deploy(chain);
    const alice = await chain.account('alice');
    const amount = 100n * 10n ** 18n;

    await token.fn('transfer', hub.address, amount);
    await hub.fn('removeRelayer', relayer);
    assertEqual(await hub.value('relayers', relayer), false);

    const transferId = ethers.id('transfer-4');
    const params = {
      token: token.address,
      sender: alice,
      recipient: alice,
      amount,
      fromChain: 56n,
      nonce: 0n,
      transferId,
    };
    const signature = await signRelay(chain, hub, await chain.privateKeyOf(relayer), params);

    await assertReverts(
      hub.fnFrom(
        'relayToken',
        [
          params.token,
          params.sender,
          params.recipient,
          params.amount,
          params.fromChain,
          params.nonce,
          params.transferId,
          signature,
        ],
        { from: relayer },
      ),
      'AccessControlUnauthorizedAccount',
    );
  });

  it('restricts the administrative surface', async () => {
    const chain = await EvmChain.create();
    const { token, hub } = await deploy(chain);
    const mallory = await chain.account('mallory');

    for (const call of [
      () => hub.fnFrom('setTokenSupport', [token.address, false], { from: mallory }),
      () => hub.fnFrom('addRelayer', [mallory], { from: mallory }),
      () => hub.fnFrom('setRelayFee', [1n], { from: mallory }),
    ]) {
      await assertReverts(call(), 'AccessControlUnauthorizedAccount');
    }
  });
});

describe('TGRAutoBridge', () => {
  async function deploy(chain) {
    const bridge = await chain.deploy('bridge/TGRAutoBridge:TGRAutoBridge');
    const oracle = await chain.account('oracle');
    await bridge.fn('setOracle', oracle);
    return { bridge, oracle };
  }

  it('ships with the documented limits and fee', async () => {
    const chain = await EvmChain.create();
    const { bridge } = await deploy(chain);

    assertBigIntEqual(await bridge.value('MIN_BRIDGE_AMOUNT'), 10n ** 16n);
    assertBigIntEqual(await bridge.value('MAX_BRIDGE_AMOUNT'), 100_000n * 10n ** 18n);
    assertBigIntEqual(await bridge.value('BRIDGE_FEE'), 3n);
    assertBigIntEqual(await bridge.value('getReservedLiquidity'), 0n);
  });

  it('escrows the native amount and reports it as reserved', async () => {
    const chain = await EvmChain.create();
    const { bridge } = await deploy(chain);
    const alice = await chain.account('alice');
    const amount = 10n ** 18n;
    const fee = (amount * 3n) / 1000n;

    await bridge.fnFrom('initializeBridge', [NATIVE, amount, alice, ethers.id('bsc')], {
      from: alice,
      value: amount + fee,
    });

    assertBigIntEqual(await chain.balanceOf(bridge.address), amount + fee);
    assertBigIntEqual(await bridge.value('getReservedLiquidity'), amount);
    assertBigIntEqual(await bridge.value('transferCount'), 1n);
  });

  it('rejects an underfunded or out-of-range transfer', async () => {
    const chain = await EvmChain.create();
    const { bridge } = await deploy(chain);
    const alice = await chain.account('alice');

    await assertReverts(
      bridge.fnFrom('initializeBridge', [NATIVE, 10n ** 15n, alice, ethers.id('bsc')], {
        from: alice,
        value: 10n ** 15n,
      }),
      'Amount too small',
    );
    await assertReverts(
      bridge.fnFrom(
        'initializeBridge',
        [NATIVE, 100_001n * 10n ** 18n, alice, ethers.id('bsc')],
        { from: alice },
      ),
      'Amount too large',
    );
    await assertReverts(
      bridge.fnFrom('initializeBridge', [NATIVE, 10n ** 18n, alice, ethers.id('bsc')], {
        from: alice,
        value: 10n ** 18n,
      }),
      'Insufficient BNB',
    );
    await assertReverts(
      bridge.fnFrom(
        'initializeBridge',
        [NATIVE, 10n ** 18n, '0x' + '00'.repeat(20), ethers.id('bsc')],
        { from: alice, value: 10n ** 18n },
      ),
      'Invalid recipient',
    );
  });

  it('pulls ERC20 deposits into escrow and rejects unsupported tokens', async () => {
    const chain = await EvmChain.create();
    const { bridge } = await deploy(chain);
    const token = await deployToken(chain);
    const alice = await chain.account('alice');
    const amount = 100n * 10n ** 18n;

    await token.fn('transfer', alice, amount);
    await token.fnFrom('approve', [bridge.address, amount], { from: alice });

    await assertReverts(
      bridge.fnFrom('initializeBridge', [token.address, amount, alice, ethers.id('bsc')], {
        from: alice,
      }),
      'Token not supported',
    );

    await bridge.fn('setTokenSupported', [token.address, true]);
    await bridge.fnFrom('initializeBridge', [token.address, amount, alice, ethers.id('bsc')], {
      from: alice,
    });

    assertBigIntEqual(await token.value('balanceOf', bridge.address), amount);
    assertBigIntEqual(await token.value('balanceOf', alice), 0n);
  });

  it('pays the recipient when the oracle completes a native transfer', async () => {
    const chain = await EvmChain.create();
    const { bridge, oracle } = await deploy(chain);
    const alice = await chain.account('alice');
    const recipient = await chain.account('recipient');
    const amount = 10n ** 18n;
    const fee = (amount * 3n) / 1000n;

    const { logs } = await bridge.fnFrom(
      'initializeBridge',
      [NATIVE, amount, recipient, ethers.id('bsc')],
      { from: alice, value: amount + fee },
    );
    const id = logs.find((l) => l.name === 'BridgeInitialized').args[0];

    const before = await chain.balanceOf(recipient);
    await bridge.fnFrom('completeBridge', [id, recipient], { from: oracle });
    const after = await chain.balanceOf(recipient);

    assertBigIntEqual(after - before, amount);
    // The fee stays with the bridge and is no longer reserved.
    assertBigIntEqual(await chain.balanceOf(bridge.address), fee);
    assertBigIntEqual(await bridge.value('getReservedLiquidity'), 0n);
  });

  it('only lets the oracle or governance complete a transfer', async () => {
    const chain = await EvmChain.create();
    const { bridge } = await deploy(chain);
    const alice = await chain.account('alice');
    const recipient = await chain.account('recipient');
    const amount = 10n ** 18n;
    const fee = (amount * 3n) / 1000n;

    const { logs } = await bridge.fnFrom(
      'initializeBridge',
      [NATIVE, amount, recipient, ethers.id('bsc')],
      { from: alice, value: amount + fee },
    );
    const id = logs.find((l) => l.name === 'BridgeInitialized').args[0];

    await assertReverts(
      bridge.fnFrom('completeBridge', [id, recipient], { from: alice }),
      'Not authorized',
    );
  });

  it('cannot complete the same transfer twice', async () => {
    const chain = await EvmChain.create();
    const { bridge, oracle } = await deploy(chain);
    const alice = await chain.account('alice');
    const recipient = await chain.account('recipient');
    const amount = 10n ** 18n;
    const fee = (amount * 3n) / 1000n;

    const { logs } = await bridge.fnFrom(
      'initializeBridge',
      [NATIVE, amount, recipient, ethers.id('bsc')],
      { from: alice, value: amount + fee },
    );
    const id = logs.find((l) => l.name === 'BridgeInitialized').args[0];

    await bridge.fnFrom('completeBridge', [id, recipient], { from: oracle });
    await assertReverts(
      bridge.fnFrom('completeBridge', [id, recipient], { from: oracle }),
      'Not pending',
    );
  });

  it('refunds the sender and the fee after the timeout', async () => {
    const chain = await EvmChain.create();
    const { bridge } = await deploy(chain);
    const alice = await chain.account('alice');
    const recipient = await chain.account('recipient');
    const amount = 10n ** 18n;
    const fee = (amount * 3n) / 1000n;

    const { logs } = await bridge.fnFrom(
      'initializeBridge',
      [NATIVE, amount, recipient, ethers.id('bsc')],
      { from: alice, value: amount + fee },
    );
    const id = logs.find((l) => l.name === 'BridgeInitialized').args[0];

    await assertReverts(
      bridge.fnFrom('cancelBridge', [id], { from: alice }),
      'Not expired',
    );

    await chain.mine(1, { secondsPerBlock: Number(8n * DAY) });
    const before = await chain.balanceOf(alice);
    await bridge.fnFrom('cancelBridge', [id], { from: alice });
    const after = await chain.balanceOf(alice);

    // The refund is the full deposit; the caller also pays gas for the call.
    const refunded = after - before;
    assert(refunded > 0n && refunded <= amount + fee, 'refund must not exceed the deposit');
    assert(
      amount + fee - refunded < 10n ** 16n,
      'the refund should be the deposit less gas',
    );
    assertBigIntEqual(await chain.balanceOf(bridge.address), 0n);
    assertBigIntEqual(await bridge.value('getReservedLiquidity'), 0n);
  });

  it('does not let a liquidity provider withdraw escrowed funds', async () => {
    const chain = await EvmChain.create();
    const { bridge } = await deploy(chain);
    const provider = await chain.account('provider');
    const alice = await chain.account('alice');
    const recipient = await chain.account('recipient');

    await bridge.fnFrom('addLiquidity', [], { from: provider, value: 5n * 10n ** 18n });

    const amount = 4n * 10n ** 18n;
    const fee = (amount * 3n) / 1000n;
    await bridge.fnFrom('initializeBridge', [NATIVE, amount, recipient, ethers.id('bsc')], {
      from: alice,
      value: amount + fee,
    });

    // 5 ether deposited, 4 ether reserved, so only 1 ether is withdrawable.
    await assertReverts(
      bridge.fnFrom('removeLiquidity', [5n * 10n ** 18n], { from: provider }),
      'Insufficient available',
    );
    await assertReverts(
      bridge.fnFrom('removeLiquidity', [2n * 10n ** 18n], { from: provider }),
      'Insufficient available',
    );
    await bridge.fnFrom('removeLiquidity', [1n * 10n ** 18n], { from: provider });
    assertBigIntEqual(await bridge.value('getReservedLiquidity'), amount);
  });

  it('gates the administrative surface and the pause switch', async () => {
    const chain = await EvmChain.create();
    const { bridge } = await deploy(chain);
    const mallory = await chain.account('mallory');
    const alice = await chain.account('alice');

    await assertReverts(
      bridge.fnFrom('pause', [], { from: mallory }),
      'Not governance',
    );
    await assertReverts(
      bridge.fnFrom('setOracle', [mallory], { from: mallory }),
      'Not governance',
    );

    await bridge.fn('pause');
    await assertReverts(
      bridge.fnFrom('initializeBridge', [NATIVE, 10n ** 18n, alice, ethers.id('bsc')], {
        from: alice,
        value: 10n ** 18n,
      }),
      'Paused',
    );
    await bridge.fn('unpause');
  });

  it('transfers governance to a new address', async () => {
    const chain = await EvmChain.create();
    const { bridge } = await deploy(chain);
    const newGov = await chain.account('new-governance');

    await bridge.fn('setGovernance', newGov);
    await assertEqualAddress(await bridge.value('governance'), newGov);

    const mallory = await chain.account('mallory');
    await assertReverts(
      bridge.fnFrom('pause', [], { from: mallory }),
      'Not governance',
    );
    await bridge.fnFrom('pause', [], { from: newGov });
    assertEqual(await bridge.value('paused'), true);
  });
});

function assertEqualAddress(actual, expected) {
  assert(
    String(actual).toLowerCase() === String(expected).toLowerCase(),
    `expected ${expected} but got ${actual}`,
  );
}
