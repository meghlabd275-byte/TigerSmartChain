// Governance suites: Governor (token-weighted proposals, block-based voting,
// timelock queue and execution) and ParameterGovernor (owner-managed chain
// parameters with a propose/vote/execute cycle).

import { ethers } from 'ethers';
import { describe, it, assert, assertBigIntEqual, assertAddressEqual, assertReverts } from '../test-harness.mjs';
import { EvmChain } from '../evm-chain.mjs';

const SUPPLY = 1_000_000n * 10n ** 18n;
const DAY = 86_400n;

// Parameter keys are right-padded bytes32 strings, not keccak hashes.
const paramId = (name) => ethers.encodeBytes32String(name);

describe('Governor', () => {
  async function deploy(chain) {
    const token = await chain.deploy('TEP126/TEP126:TEP126', [
      'Gov Token',
      'GOV',
      18,
      SUPPLY,
      SUPPLY,
      '1',
    ]);
    const timelock = await chain.deploy('governance/Governor:Timelock', [2n * DAY]);
    const governor = await chain.deploy('governance/Governor:Governor', [
      token.address,
      timelock.address,
      1n, // votingDelay (blocks)
      50n, // votingPeriod (blocks)
      1n, // proposalThreshold
      1_000n * 10n ** 18n, // quorumVotes
    ]);
    return { token, timelock, governor };
  }

  it('stores the constructor configuration', async () => {
    const chain = await EvmChain.create();
    const { token, timelock, governor } = await deploy(chain);

    assertAddressEqual(await governor.value('token'), token.address);
    assertAddressEqual(await governor.value('timelock'), timelock.address);
    assertBigIntEqual(await governor.value('votingDelay'), 1n);
    assertBigIntEqual(await governor.value('votingPeriod'), 50n);
    assertBigIntEqual(await governor.value('quorumVotes'), 1_000n * 10n ** 18n);
    assertBigIntEqual(await governor.value('timelockDelay'), 2n * DAY);
    assertBigIntEqual(await governor.value('proposalCount'), 0n);
  });

  it('rejects a zero token or timelock address', async () => {
    const chain = await EvmChain.create();
    const { token, timelock } = await deploy(chain);
    const zero = '0x' + '00'.repeat(20);

    await assertReverts(
      chain.deploy('governance/Governor:Governor', [zero, timelock.address, 1n, 1n, 0n, 0n]),
      'Invalid token',
    );
    await assertReverts(
      chain.deploy('governance/Governor:Governor', [token.address, zero, 1n, 1n, 0n, 0n]),
      'Invalid timelock',
    );
  });

  it('records a proposal and starts it pending', async () => {
    const chain = await EvmChain.create();
    const { governor } = await deploy(chain);

    const { logs } = await governor.fn('propose', [
      [chain.owner],
      [0n],
      [''],
      ['0x'],
      'do something',
    ]);

    const created = logs.find((l) => l.name === 'ProposalCreated');
    assert(created, 'ProposalCreated was not emitted');
    assertBigIntEqual(created.args[0], 1n);
    assertBigIntEqual(await governor.value('proposalCount'), 1n);
    assertBigIntEqual(await governor.value('state', 1n), 0n); // Pending
  });

  it('rejects a proposal from an account below the threshold', async () => {
    const chain = await EvmChain.create();
    const { governor } = await deploy(chain);
    await governor.fn('setProposalThreshold', 10n ** 18n);

    const poor = await chain.account('poor');
    await assertReverts(
      governor.fnFrom('propose', [[chain.owner], [0n], [''], ['0x'], 'x'], { from: poor }),
      'Votes below threshold',
    );
  });

  it('rejects a proposal with mismatched or empty action arrays', async () => {
    const chain = await EvmChain.create();
    const { governor } = await deploy(chain);

    await assertReverts(
      governor.fn('propose', [[chain.owner], [], [''], ['0x'], 'x']),
      'Length mismatch',
    );
    await assertReverts(governor.fn('propose', [[], [], [], [], 'x']), 'No actions');
  });

  it('moves a proposal through pending, active, succeeded and executed', async () => {
    const chain = await EvmChain.create();
    const { governor } = await deploy(chain);

    // The action is a self-call that changes the quorum, so execution is
    // observable through contract state. The governor needs the admin role on
    // itself for the call to be permitted.
    await governor.fn('grantRole', await governor.value('DEFAULT_ADMIN_ROLE'), governor.address);
    const call = governor.encode('setQuorumVotes', [12345n]);
    await governor.fn('propose', [[governor.address], [0n], [''], [call], 'set quorum']);

    assertBigIntEqual(await governor.value('state', 1n), 0n); // Pending

    await chain.mine(2);
    assertBigIntEqual(await governor.value('state', 1n), 1n); // Active

    await governor.fn('castVote', 1n, true);
    await chain.mine(50);
    assertBigIntEqual(await governor.value('state', 1n), 4n); // Succeeded

    await governor.fn('queue', 1n);
    assertBigIntEqual(await governor.value('state', 1n), 5n); // Queued

    await chain.mine(1, { secondsPerBlock: Number(3n * DAY) });
    await governor.fn('execute', 1n);
    assertBigIntEqual(await governor.value('state', 1n), 6n); // Executed
    assertBigIntEqual(await governor.value('quorumVotes'), 12345n);
  });

  it('counts for and against votes separately', async () => {
    const chain = await EvmChain.create();
    const { token, governor } = await deploy(chain);
    const alice = await chain.account('alice');

    await token.fn('transfer', alice, 5_000n * 10n ** 18n);
    await governor.fn('propose', [[chain.owner], [0n], [''], ['0x'], 'x']);
    await chain.mine(2);

    await governor.fn('castVote', 1n, true);
    await governor.fnFrom('castVote', [1n, false], { from: alice });

    const proposal = await governor.values('proposals', 1n);
    assertBigIntEqual(proposal[4], SUPPLY - 5_000n * 10n ** 18n);
    assertBigIntEqual(proposal[5], 5_000n * 10n ** 18n);
  });

  it('defeats a proposal when the against votes win', async () => {
    const chain = await EvmChain.create();
    const { token, governor } = await deploy(chain);
    const alice = await chain.account('alice');

    // Alice ends up with the whole supply, so her "against" vote decides it.
    await token.fn('transfer', alice, SUPPLY);
    await governor.fnFrom('propose', [[chain.owner], [0n], [''], ['0x'], 'x'], {
      from: alice,
    });
    await chain.mine(2);
    await governor.fnFrom('castVote', [1n, false], { from: alice });

    await chain.mine(50);
    assertBigIntEqual(await governor.value('state', 1n), 3n); // Defeated
    await assertReverts(governor.fn('execute', 1n), 'Not executable');
  });

  it('defeats a proposal that misses quorum', async () => {
    const chain = await EvmChain.create();
    const { governor } = await deploy(chain);
    await governor.fn('setQuorumVotes', SUPPLY * 2n);

    await governor.fn('propose', [[chain.owner], [0n], [''], ['0x'], 'x']);
    await chain.mine(2);
    await governor.fn('castVote', 1n, true);
    await chain.mine(50);

    assertBigIntEqual(await governor.value('state', 1n), 3n); // Defeated
  });

  it('blocks a second vote from the same account', async () => {
    const chain = await EvmChain.create();
    const { governor } = await deploy(chain);

    await governor.fn('propose', [[chain.owner], [0n], [''], ['0x'], 'x']);
    await chain.mine(2);
    await governor.fn('castVote', 1n, true);

    await assertReverts(governor.fn('castVote', 1n, true), 'Already voted');
  });

  it('rejects a vote outside the voting window', async () => {
    const chain = await EvmChain.create();
    const { governor } = await deploy(chain);

    await governor.fn('propose', [[chain.owner], [0n], [''], ['0x'], 'x']);
    await assertReverts(governor.fn('castVote', 1n, true), 'Not active');
  });

  it('cancels a proposal through the proposer', async () => {
    const chain = await EvmChain.create();
    const { governor } = await deploy(chain);

    await governor.fn('propose', [[chain.owner], [0n], [''], ['0x'], 'x']);
    await governor.fn('cancel', 1n);
    assertBigIntEqual(await governor.value('state', 1n), 2n); // Canceled
    await assertReverts(governor.fn('castVote', 1n, true), 'Not active');
  });

  it('refuses to cancel a proposal from an unrelated account', async () => {
    const chain = await EvmChain.create();
    const { governor } = await deploy(chain);
    const mallory = await chain.account('mallory');

    await governor.fn('propose', [[chain.owner], [0n], [''], ['0x'], 'x']);
    await assertReverts(governor.fnFrom('cancel', [1n], { from: mallory }), 'Not authorized');
  });

  it('cannot queue a proposal that has not succeeded', async () => {
    const chain = await EvmChain.create();
    const { governor } = await deploy(chain);

    await governor.fn('propose', [[chain.owner], [0n], [''], ['0x'], 'x']);
    await assertReverts(governor.fn('queue', 1n), 'Not succeeded');
  });

  it('cannot execute the same proposal twice', async () => {
    const chain = await EvmChain.create();
    const { governor } = await deploy(chain);

    await governor.fn('propose', [[chain.owner], [0n], [''], ['0x'], 'x']);
    await chain.mine(2);
    await governor.fn('castVote', 1n, true);
    await chain.mine(50);
    await governor.fn('execute', 1n);

    await assertReverts(governor.fn('execute', 1n), 'Not executable');
  });

  it('restricts the configuration surface to the admin', async () => {
    const chain = await EvmChain.create();
    const { governor } = await deploy(chain);
    const mallory = await chain.account('mallory');

    await assertReverts(
      governor.fnFrom('setVotingDelay', [5n], { from: mallory }),
      'AccessControlUnauthorizedAccount',
    );
    await assertReverts(
      governor.fnFrom('setQuorumVotes', [1n], { from: mallory }),
      'AccessControlUnauthorizedAccount',
    );
  });

  it('accepts a vote signed off-chain', async () => {
    const chain = await EvmChain.create();
    const { token, governor } = await deploy(chain);
    const alice = await chain.account('alice');

    await token.fn('transfer', alice, 1_000n * 10n ** 18n);
    await governor.fn('propose', [[chain.owner], [0n], [''], ['0x'], 'x']);
    await chain.mine(2);

    const { v, r, s } = await signVote(chain, governor, await chain.privateKeyOf(alice), {
      proposalId: 1n,
      support: true,
    });
    await governor.fn('castVoteBySig', [1n, true, v, r, s]);

    const receipt = await governor.values('proposalReceipts', 1n, alice);
    assert(receipt[0], 'the signature should have been credited to alice');
  });

  it('rejects a vote signature that recovers to the zero address', async () => {
    const chain = await EvmChain.create();
    const { governor } = await deploy(chain);

    await governor.fn('propose', [[chain.owner], [0n], [''], ['0x'], 'x']);
    await chain.mine(2);

    await assertReverts(
      governor.fn('castVoteBySig', [
        1n,
        true,
        0,
        '0x' + '00'.repeat(32),
        '0x' + '00'.repeat(32),
      ]),
      'Invalid signature',
    );
  });
});

describe('Governor Timelock', () => {
  it('enforces the delay and then executes a scheduled call', async () => {
    const chain = await EvmChain.create();
    const target = await chain.deploy('TEP126/TEP126:TEP126', [
      'T',
      'T',
      18,
      1000n,
      1000n,
      '1',
    ]);
    const timelock = await chain.deploy('governance/Governor:Timelock', [2n * DAY]);

    await target.fn('burn', 1000n);

    // The timelock must hold the token admin role for the call to succeed.
    await target.fn('grantRole', await target.value('DEFAULT_ADMIN_ROLE'), timelock.address);

    const data = target.encode('updateCap', [999n]);
    const eta = chain.timestamp + 3n * DAY;

    const { logs } = await timelock.fn('schedule', [target.address, 0n, data, eta]);
    assert(logs.some((l) => l.name === 'CallScheduled'), 'schedule did not emit');

    await assertReverts(
      timelock.fn('execute', [target.address, 0n, data, eta]),
      'Transaction locked',
    );

    await chain.mine(1, { secondsPerBlock: Number(4n * DAY) });
    await timelock.fn('execute', [target.address, 0n, data, eta]);
    assertBigIntEqual(await target.value('cap'), 999n);
  });

  it('rejects an eta below the minimum delay', async () => {
    const chain = await EvmChain.create();
    const timelock = await chain.deploy('governance/Governor:Timelock', [2n * DAY]);

    await assertReverts(
      timelock.fn('schedule', [chain.owner, 0n, '0x', chain.timestamp + 60n]),
      'Delay not met',
    );
  });

  it('rejects a repeated schedule for the same transaction', async () => {
    const chain = await EvmChain.create();
    const timelock = await chain.deploy('governance/Governor:Timelock', [2n * DAY]);
    const eta = chain.timestamp + 3n * DAY;

    await timelock.fn('schedule', [chain.owner, 0n, '0x', eta]);
    await assertReverts(
      timelock.fn('schedule', [chain.owner, 0n, '0x', eta]),
      'Already queued',
    );
  });

  it('cancels a queued transaction', async () => {
    const chain = await EvmChain.create();
    const timelock = await chain.deploy('governance/Governor:Timelock', [2n * DAY]);
    const eta = chain.timestamp + 3n * DAY;

    await timelock.fn('schedule', [chain.owner, 0n, '0x', eta]);
    await timelock.fn('cancel', [timelockHash(chain.owner, 0n, '0x', eta)]);

    await chain.mine(1, { secondsPerBlock: Number(4n * DAY) });
    await assertReverts(
      timelock.fn('execute', [chain.owner, 0n, '0x', eta]),
      'Not queued',
    );
  });

  it('restricts scheduling to the admin role', async () => {
    const chain = await EvmChain.create();
    const timelock = await chain.deploy('governance/Governor:Timelock', [2n * DAY]);
    const mallory = await chain.account('mallory');

    await assertReverts(
      timelock.fnFrom('schedule', [chain.owner, 0n, '0x', chain.timestamp + 3n * DAY], {
        from: mallory,
      }),
      'AccessControlUnauthorizedAccount',
    );
  });
});

describe('ParameterGovernor', () => {
  async function deploy(chain) {
    return chain.deploy('governance/ParameterGovernor:ParameterGovernor');
  }

  const MIN_STAKE = paramId('MIN_STAKE');

  it('ships with the default chain parameters', async () => {
    const chain = await EvmChain.create();
    const governor = await deploy(chain);

    assertBigIntEqual(await governor.value('votingPeriod'), 3n * DAY);
    assertBigIntEqual(await governor.value('votingThreshold'), 100n * 10n ** 18n);
    assertBigIntEqual(await governor.value('executionDelay'), DAY);

    assertBigIntEqual(await governor.value('getParameter', MIN_STAKE), 100n * 10n ** 18n);

    const [details] = await governor.values('getParameterDetails', MIN_STAKE);
    assertBigIntEqual(details.value, 100n * 10n ** 18n);
    assertBigIntEqual(details.minValue, 1n * 10n ** 18n);
    assertBigIntEqual(details.maxValue, 10_000n * 10n ** 18n);
  });

  it('lets the owner set a parameter inside its bounds', async () => {
    const chain = await EvmChain.create();
    const governor = await deploy(chain);

    const { logs } = await governor.fn('setParameter', [MIN_STAKE, 500n * 10n ** 18n]);
    assert(logs.some((l) => l.name === 'ParameterUpdated'), 'ParameterUpdated missing');

    assertBigIntEqual(await governor.value('getParameter', MIN_STAKE), 500n * 10n ** 18n);
  });

  it('rejects a value outside the parameter bounds', async () => {
    const chain = await EvmChain.create();
    const governor = await deploy(chain);

    await assertReverts(governor.fn('setParameter', [MIN_STAKE, 10n ** 17n]), 'InvalidValue');
    await assertReverts(
      governor.fn('setParameter', [MIN_STAKE, 20_000n * 10n ** 18n]),
      'InvalidValue',
    );
  });

  it('rejects a parameter that was never initialised', async () => {
    const chain = await EvmChain.create();
    const governor = await deploy(chain);

    await assertReverts(
      governor.fn('setParameter', [paramId('DOES_NOT_EXIST'), 1n]),
      'Parameter not initialized',
    );
  });

  it('restricts direct parameter writes to the owner', async () => {
    const chain = await EvmChain.create();
    const governor = await deploy(chain);
    const mallory = await chain.account('mallory');

    await assertReverts(
      governor.fnFrom('setParameter', [MIN_STAKE, 500n * 10n ** 18n], { from: mallory }),
      'Unauthorized',
    );
  });

  it('runs a full propose, vote and execute cycle', async () => {
    const chain = await EvmChain.create();
    const governor = await deploy(chain);

    await governor.fn('setVotingThreshold', 100n * 10n ** 18n);
    await governor.fn('setVotingPower', [chain.owner, 1_000n * 10n ** 18n]);

    const { logs } = await governor.fn('proposeParameterChange', [
      MIN_STAKE,
      500n * 10n ** 18n,
    ]);
    assert(logs.some((l) => l.name === 'ParameterProposed'), 'ParameterProposed missing');

    const proposals = await governor.value('getActiveProposals');
    assertBigIntEqual(proposals.length, 1n);
    const proposalId = proposals[0];

    await governor.fn('vote', [proposalId]);
    await chain.mine(1, { secondsPerBlock: Number(5n * DAY) });
    await governor.fn('executeProposal', [proposalId]);

    assertBigIntEqual(await governor.value('getParameter', MIN_STAKE), 500n * 10n ** 18n);
    assertBigIntEqual(await governor.value('getActiveProposalsCount'), 0n);
  });

  it('rejects a proposal from an account without voting power', async () => {
    const chain = await EvmChain.create();
    const governor = await deploy(chain);
    const poor = await chain.account('poor');

    await assertReverts(
      governor.fnFrom('proposeParameterChange', [MIN_STAKE, 500n * 10n ** 18n], {
        from: poor,
      }),
      'Insufficient voting power',
    );
  });

  it('blocks a double vote on the same proposal', async () => {
    const chain = await EvmChain.create();
    const governor = await deploy(chain);

    await governor.fn('setVotingPower', [chain.owner, 1_000n * 10n ** 18n]);
    await governor.fn('proposeParameterChange', [MIN_STAKE, 500n * 10n ** 18n]);
    const [proposalId] = await governor.value('getActiveProposals');

    await governor.fn('vote', [proposalId]);
    await assertReverts(governor.fn('vote', [proposalId]), 'AlreadyVoted');
  });

  it('refuses to execute before the voting period and delay elapse', async () => {
    const chain = await EvmChain.create();
    const governor = await deploy(chain);

    await governor.fn('setVotingPower', [chain.owner, 1_000n * 10n ** 18n]);
    await governor.fn('proposeParameterChange', [MIN_STAKE, 500n * 10n ** 18n]);
    const [proposalId] = await governor.value('getActiveProposals');

    await assertReverts(governor.fn('executeProposal', [proposalId]), 'VotingNotActive');

    // Past the voting period but still inside the execution delay.
    await chain.mine(1, { secondsPerBlock: Number(3n * DAY + 60n) });
    await assertReverts(governor.fn('executeProposal', [proposalId]), 'ExecutionNotReady');

    await chain.mine(1, { secondsPerBlock: Number(DAY) });
    await assertReverts(governor.fn('executeProposal', [proposalId]), 'Proposal did not pass');
  });

  it('refuses to execute a proposal twice', async () => {
    const chain = await EvmChain.create();
    const governor = await deploy(chain);

    await governor.fn('setVotingPower', [chain.owner, 1_000n * 10n ** 18n]);
    await governor.fn('proposeParameterChange', [MIN_STAKE, 500n * 10n ** 18n]);
    const [proposalId] = await governor.value('getActiveProposals');
    await governor.fn('vote', [proposalId]);

    await chain.mine(1, { secondsPerBlock: Number(5n * DAY) });
    await governor.fn('executeProposal', [proposalId]);
    await assertReverts(
      governor.fn('executeProposal', [proposalId]),
      'ProposalAlreadyExecuted',
    );
  });

  it('sets voting power for a batch of accounts', async () => {
    const chain = await EvmChain.create();
    const governor = await deploy(chain);
    const a = await chain.account('a');
    const b = await chain.account('b');

    await governor.fn('batchSetVotingPower', [[a, b], [100n, 200n]]);
    assertBigIntEqual(await governor.value('votingPower', a), 100n);
    assertBigIntEqual(await governor.value('votingPower', b), 200n);
    assertBigIntEqual(await governor.value('totalVotingPower'), 300n);
  });

  it('rejects a batch with mismatched array lengths', async () => {
    const chain = await EvmChain.create();
    const governor = await deploy(chain);

    await assertReverts(
      governor.fn('batchSetVotingPower', [[chain.owner], [1n, 2n]]),
      'Length mismatch',
    );
  });
});

// Mirrors Timelock's transaction hash so the test can cancel a queued call.
function timelockHash(target, value, data, eta) {
  return ethers.keccak256(
    ethers.AbiCoder.defaultAbiCoder().encode(
      ['address', 'uint256', 'bytes', 'uint256'],
      [target, value, data, eta],
    ),
  );
}

// The Governor hashes (proposalId, support) for signature-based voting.
async function signVote(chain, governor, privateKey, { proposalId, support }) {
  const domainSeparator = ethers.keccak256(
    ethers.AbiCoder.defaultAbiCoder().encode(
      ['bytes32', 'bytes32', 'uint256', 'address'],
      [
        ethers.keccak256(
          ethers.toUtf8Bytes(
            'EIP712Domain(string name,uint256 chainId,address verifyingContract)',
          ),
        ),
        ethers.keccak256(ethers.toUtf8Bytes('Governor')),
        BigInt(chain.common.chainId()),
        governor.address,
      ],
    ),
  );

  const structHash = ethers.keccak256(
    ethers.AbiCoder.defaultAbiCoder().encode(['uint256', 'bool'], [proposalId, support]),
  );

  const digest = ethers.keccak256(ethers.concat(['0x1901', domainSeparator, structHash]));

  const signature = new ethers.Wallet(privateKey).signingKey.sign(digest);
  return { v: signature.v, r: signature.r, s: signature.s };
}
