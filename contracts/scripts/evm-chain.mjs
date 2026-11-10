// In-process EVM used by the contract test suite.
//
// Transactions are ABI-encoded with ethers, signed with a fixed dev key, and
// executed through the @ethereumjs VM. This exercises the real deployed
// bytecode: storage writes, event logs, reverts and gas accounting all come
// from an EVM interpreter rather than a hand-written model of one.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createVM, runTx } from '@ethereumjs/vm';
import { createBlock } from '@ethereumjs/block';
import { createCustomCommon, Hardfork, Mainnet } from '@ethereumjs/common';
import { createLegacyTx } from '@ethereumjs/tx';
import {
  Account,
  bytesToHex,
  createAddressFromString,
  hexToBytes,
  privateToAddress,
} from '@ethereumjs/util';
import { ethers } from 'ethers';

export const CHAIN_ID = 6666;

const GAS_LIMIT = 30_000_000n;

// Must exceed the Cancun base fee that the VM's default genesis block carries.
const GAS_PRICE = 1_000_000_000n;

const DEV_KEY = hexToBytes('0x' + '22'.repeat(32));

const BUILD_DIR = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
  'build',
);

// Contract names repeat across sources (there are two `TEP20` contracts and
// three `TokenFactory` contracts), so artifacts are addressed as
// "sourcePath:ContractName". A bare name is accepted when it is unambiguous.
export async function loadArtifact(spec) {
  const index = JSON.parse(
    fs.readFileSync(path.join(BUILD_DIR, 'index.json'), 'utf8'),
  );
  const key = spec.includes(':') ? spec : findUnique(index, spec);
  const relative = index[key];
  if (!relative) {
    throw new Error(
      `artifact not found: ${spec} (known: ${Object.keys(index).join(', ')})`,
    );
  }
  return JSON.parse(fs.readFileSync(path.join(BUILD_DIR, relative), 'utf8'));
}

function findUnique(index, name) {
  const matches = Object.keys(index).filter(
    (key) => key.split(':')[1] === name,
  );
  if (matches.length === 1) return matches[0];
  if (matches.length === 0) {
    throw new Error(`artifact not found: ${name} (run npm run compile first)`);
  }
  throw new Error(
    `artifact name ${name} is ambiguous; use one of: ${matches.join(', ')}`,
  );
}

export class EvmChain {
  constructor(vm, common, owner) {
    this.vm = vm;
    this.common = common;
    this.owner = owner;
    this.blockNumber = 1n;
    this.timestamp = 1_800_000_000n;
    this.accounts = new Map([[owner, DEV_KEY]]);
    this.block = createBlock(
      {
        header: {
          number: this.blockNumber,
          timestamp: this.timestamp,
          gasLimit: GAS_LIMIT,
        },
      },
      { common },
    );
  }

  static async create() {
    const common = createCustomCommon(
      { name: 'tigersmartchain', networkId: CHAIN_ID, chainId: CHAIN_ID },
      Mainnet,
      { hardfork: Hardfork.Cancun },
    );
    const vm = await createVM({ common });
    const owner = bytesToHex(privateToAddress(DEV_KEY));
    const chain = new EvmChain(vm, common, owner);
    await chain.fund(owner, 10n ** 24n);
    return chain;
  }

  /**
   * Returns a deterministic funded account. Tests that need several distinct
   * signers (multisig, governance, staking) address them by name.
   */
  async account(name) {
    const key = hexToBytes(
      ethers.keccak256(ethers.toUtf8Bytes(`tigersmartchain:${name}`)),
    );
    const address = bytesToHex(privateToAddress(key));
    if (!this.accounts.has(address)) {
      this.accounts.set(address, key);
      await this.fund(address, 10n ** 22n);
    }
    return address;
  }

  async fund(address, amount) {
    const target = createAddressFromString(address);
    const existing = await this.vm.stateManager.getAccount(target);
    if (existing) {
      // Mutate the stored account; writing a fresh Account would reset the
      // code hash and the storage root, destroying a deployed contract.
      existing.balance += amount;
      await this.vm.stateManager.putAccount(target, existing);
      return;
    }
    await this.vm.stateManager.putAccount(target, new Account(0n, amount));
  }

  /** Returns the signing key registered for an address. */
  privateKeyOf(address) {
    const key = this.accounts.get(address);
    if (!key) throw new Error(`evm-chain: no key registered for ${address}`);
    return bytesToHex(key);
  }

  async balanceOf(address) {
    const account = await this.vm.stateManager.getAccount(
      createAddressFromString(address),
    );
    return account?.balance ?? 0n;
  }

  /**
   * Advances the chain by `count` blocks. `block.number` and `block.timestamp`
   * both move, which is what governance and vesting schedules read.
   */
  async mine(count = 1, { secondsPerBlock = 12 } = {}) {
    for (let i = 0; i < count; i++) {
      this.blockNumber += 1n;
      this.timestamp += BigInt(secondsPerBlock);
      this.block = createBlock(
        {
          header: {
            number: this.blockNumber,
            timestamp: this.timestamp,
            gasLimit: GAS_LIMIT,
          },
        },
        { common: this.common },
      );
    }
  }

  async #run({ to, data, value = 0n, gasLimit = GAS_LIMIT, from = this.owner }) {
    const key = this.accounts.get(from);
    if (!key) throw new Error(`evm-chain: unknown sender ${from}`);

    const account = await this.vm.stateManager.getAccount(
      createAddressFromString(from),
    );
    const tx = createLegacyTx(
      {
        nonce: account?.nonce ?? 0n,
        gasLimit,
        gasPrice: GAS_PRICE,
        to,
        value,
        data,
      },
      { common: this.common },
    ).sign(key);

    const result = await runTx(this.vm, {
      tx,
      block: this.block,
      skipBalance: false,
      skipNonce: false,
    });
    return result;
  }

  async deploy(name, args = [], { from = this.owner } = {}) {
    const { bytecode, abi } = await loadArtifact(name);
    const iface = new ethers.Interface(abi);
    const encodedArgs =
      iface.deploy.inputs.length > 0 ? iface.encodeDeploy(args).slice(2) : '';

    const result = await this.#run({ data: bytecode + encodedArgs, from });
    if (result.execResult.exceptionError) {
      throw new Error(
        `deploy ${name} failed: ${this.#revertReason(
          result.execResult.returnValue,
          abi,
        )}`,
      );
    }
    return new Contract(this, result.createdAddress.toString(), abi, name);
  }

  /** Message call with a decoded return value. */
  async send(address, abi, fn, args = [], { value = 0n, from = this.owner } = {}) {
    const iface = new ethers.Interface(abi);
    const data = iface.encodeFunctionData(fn, args);
    const result = await this.#run({ to: address, data, value, from });

    if (result.execResult.exceptionError) {
      throw new Error(
        `${fn} reverted: ${this.#revertReason(
          result.execResult.returnValue,
          abi,
        )}`,
      );
    }

    return {
      decoded: this.#decode(iface, fn, result.execResult.returnValue),
      raw: bytesToHex(result.execResult.returnValue),
      gasUsed: result.totalGasSpent,
      logs: this.#decodeLogs(result.execResult.logs ?? [], abi),
    };
  }

  /** Read-only call: no state change and no nonce consumed. */
  async callStatic(address, abi, fn, args = [], { from = this.owner } = {}) {
    const iface = new ethers.Interface(abi);
    const data = iface.encodeFunctionData(fn, args);
    const result = await this.vm.evm.runCall({
      caller: createAddressFromString(from),
      to: createAddressFromString(address),
      data: hexToBytes(data),
      gasLimit: GAS_LIMIT,
      origin: createAddressFromString(from),
      block: this.block,
    });
    if (result.execResult.exceptionError) {
      throw new Error(
        `${fn} reverted: ${this.#revertReason(
          result.execResult.returnValue,
          abi,
        )}`,
      );
    }
    return this.#decode(iface, fn, result.execResult.returnValue);
  }

  async read(address, abi, fn, args = [], opts) {
    const [value] = await this.callStatic(address, abi, fn, args, opts);
    return value;
  }

  async codeAt(address) {
    const code = await this.vm.stateManager.getCode(
      createAddressFromString(address),
    );
    return bytesToHex(code ?? new Uint8Array());
  }

  /** Asserts a call reverts, optionally matching text in the revert reason. */
  async expectRevert(address, abi, fn, args = [], match, opts) {
    try {
      await this.send(address, abi, fn, args, opts);
    } catch (error) {
      if (match && !error.message.includes(match)) {
        throw new Error(
          `expected revert matching "${match}" but got: ${error.message}`,
        );
      }
      return error.message;
    }
    throw new Error(`${fn} was expected to revert but succeeded`);
  }

  #decode(iface, fn, returnValue) {
    const out = bytesToHex(returnValue);
    const fragment = iface.getFunction(fn);
    if (!fragment || fragment.outputs.length === 0) return [];
    return Array.from(iface.decodeFunctionResult(fn, out));
  }

  // Reverts surface either as `Error(string)` from `require`, a panic code,
  // or a contract-specific custom error. Decoding all three lets tests assert
  // on the exact failure instead of matching on a generic "revert".
  #revertReason(returnValue, abi) {
    const hex = bytesToHex(returnValue);

    if (hex.startsWith('0x08c379a0')) {
      try {
        return ethers.AbiCoder.defaultAbiCoder().decode(['string'], '0x' + hex.slice(10))[0];
      } catch {
        return 'Error(string) with undecodable payload';
      }
    }

    if (hex.startsWith('0x4e487b71')) {
      const code = Number(BigInt('0x' + hex.slice(10, 74)));
      const names = {
        1: 'assertion failed',
        17: 'arithmetic overflow or underflow',
        18: 'division or modulo by zero',
        33: 'enum conversion out of range',
        34: 'invalid storage byte array',
        49: 'pop on empty array',
        50: 'array index out of bounds',
        65: 'out of memory',
      };
      return `panic: ${names[code] ?? `code ${code}`}`;
    }

    if (hex.length >= 10) {
      const selector = hex.slice(0, 10);
      const fragment = new ethers.Interface(abi).getError(selector);
      if (fragment) {
        try {
          const args = ethers.AbiCoder.defaultAbiCoder()
            .decode(fragment.inputs, '0x' + hex.slice(10))
            .map((v) => v.toString())
            .join(', ');
          return `${fragment.name}(${args})`;
        } catch {
          return fragment.name;
        }
      }
    }

    return hex === '0x' ? 'revert without reason' : `revert data ${hex}`;
  }

  // The VM reports logs as [address, topics[], data] with hex-encoded fields.
  #decodeLogs(logs, abi) {
    const iface = new ethers.Interface(abi);
    const decoded = [];
    for (const [address, topics, data] of logs) {
      try {
        const parsed = iface.parseLog({
          topics: topics.map((t) => (typeof t === 'string' ? t : bytesToHex(t))),
          data: typeof data === 'string' ? data : bytesToHex(data),
        });
        if (parsed) decoded.push({ ...parsed, address });
      } catch {
        // Logs emitted by other contracts are not decodable with this ABI.
      }
    }
    return decoded;
  }
}

/**
 * Accepts both `fn('transfer', to, amount)` and `fn('transfer', [to, amount])`.
 * Callers that already hold the argument list do not have to spread it.
 */
function normalizeArgs(args) {
  if (args.length === 1 && Array.isArray(args[0])) return args[0];
  return args;
}

export class Contract {
  constructor(chain, address, abi, name) {
    this.chain = chain;
    this.address = address;
    this.abi = abi;
    this.name = name;
  }

  fn(name, ...args) {
    return this.chain.send(this.address, this.abi, name, normalizeArgs(args));
  }

  fnFrom(name, args, opts) {
    return this.chain.send(this.address, this.abi, name, args, opts);
  }

  call(name, ...args) {
    return this.chain.callStatic(this.address, this.abi, name, normalizeArgs(args));
  }

  async value(name, ...args) {
    return this.chain.read(this.address, this.abi, name, normalizeArgs(args));
  }

  /** ABI-encodes a call, for use as a proposal or timelock payload. */
  encode(name, args = []) {
    return new ethers.Interface(this.abi).encodeFunctionData(name, args);
  }

  /** Reads every return value, for functions that return a tuple. */
  values(name, ...args) {
    return this.chain.callStatic(this.address, this.abi, name, normalizeArgs(args));
  }

  expectRevert(name, args, match, opts) {
    return this.chain.expectRevert(this.address, this.abi, name, args, match, opts);
  }

  async logs(name, ...args) {
    const { logs } = await this.fn(name, ...args);
    return logs;
  }

  code() {
    return this.chain.codeAt(this.address);
  }
}
