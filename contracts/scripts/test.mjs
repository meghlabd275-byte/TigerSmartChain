#!/usr/bin/env node
// Contract test suite. Every case deploys real bytecode into an in-process EVM
// and asserts on observed state, events and reverts. Run `npm run compile`
// first; the suite reads the artifacts that the compiler writes.

import { describe, it, run } from './test-harness.mjs';

import './tests/tep126.test.mjs';
import './tests/tep20-factory.test.mjs';
import './tests/bep20.test.mjs';
import './tests/nft.test.mjs';
import './tests/staking.test.mjs';
import './tests/governance.test.mjs';
import './tests/multisig.test.mjs';
import './tests/bridge.test.mjs';

const ok = await run();
process.exit(ok ? 0 : 1);

export { describe, it };
