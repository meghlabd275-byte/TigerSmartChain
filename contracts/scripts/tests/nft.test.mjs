// NFT suites: BEP721 (ERC-721 with lazy-mint vouchers) and BEP1155
// (multi-token). The lazy-mint case is a regression test for a voucher that
// used to be accepted without ever checking the signature.

import { ethers } from 'ethers';
import { describe, it, assert, assertEqual, assertBigIntEqual, assertAddressEqual, assertReverts } from '../test-harness.mjs';
import { EvmChain } from '../evm-chain.mjs';

describe('BEP721Collection', () => {
  async function deploy(chain) {
    return chain.deploy('bep721/BEP721:BEP721Collection', ['Tiger NFT', 'TNFT', '']);
  }

  it('mints sequential ids and records the creator', async () => {
    const chain = await EvmChain.create();
    const nft = await deploy(chain);
    const alice = await chain.account('alice');

    await nft.fn('mint', alice, 'ipfs://one');
    await nft.fn('mint', alice, 'ipfs://two');

    assertBigIntEqual(await nft.value('totalSupply'), 2n);
    assertAddressEqual(await nft.value('ownerOf', 1n), alice);
    assertAddressEqual(await nft.value('ownerOf', 2n), alice);
    assertEqual(await nft.value('tokenURI', 1n), 'ipfs://one');
    assertEqual(await nft.value('tokenURI', 2n), 'ipfs://two');
    assertAddressEqual(await nft.value('creatorOf', 1n), chain.owner);

    const owned = await nft.value('tokensOfOwner', alice);
    assertEqual(owned.map(String).join(','), '1,2');
  });

  it('mints a batch in one transaction', async () => {
    const chain = await EvmChain.create();
    const nft = await deploy(chain);
    const alice = await chain.account('alice');

    const { logs } = await nft.fnFrom('batchMint', [alice, ['a', 'b', 'c']]);
    const batch = logs.find((l) => l.name === 'BatchMinted');
    assert(batch, 'BatchMinted was not emitted');
    assertEqual(batch.args[1].map(String).join(','), '1,2,3');
    assertBigIntEqual(await nft.value('totalSupply'), 3n);
  });

  it('rejects a mint from an address without the minter role', async () => {
    const chain = await EvmChain.create();
    const nft = await deploy(chain);
    const mallory = await chain.account('mallory');

    await assertReverts(
      nft.fnFrom('mint', [mallory, 'x'], { from: mallory }),
      'Must have minter role',
    );
  });

  it('transfers a token and moves the balance', async () => {
    const chain = await EvmChain.create();
    const nft = await deploy(chain);
    const alice = await chain.account('alice');
    const bob = await chain.account('bob');

    await nft.fn('mint', alice, 'ipfs://one');
    await nft.fnFrom('transferFrom', [alice, bob, 1n], { from: alice });

    assertAddressEqual(await nft.value('ownerOf', 1n), bob);
    assertBigIntEqual(await nft.value('balanceOf', bob), 1n);
    assertBigIntEqual(await nft.value('balanceOf', alice), 0n);
  });

  it('burns a token the caller owns', async () => {
    const chain = await EvmChain.create();
    const nft = await deploy(chain);
    const alice = await chain.account('alice');

    await nft.fn('mint', alice, 'ipfs://one');
    await nft.fnFrom('burn', [1n], { from: alice });

    assertBigIntEqual(await nft.value('totalSupply'), 0n);
    await assertReverts(nft.value('ownerOf', 1n), 'ERC721NonexistentToken');
  });

  describe('lazy mint', () => {
    async function setup() {
      const chain = await EvmChain.create();
      const nft = await deploy(chain);
      const signer = await chain.account('signer');
      await nft.fn('setLazyMintSigner', signer, true);
      return { chain, nft, signer };
    }

    async function voucher(nft, chain, signerKey, { to, uri, tokenId, deadline }) {
      const hash = await nft.value('lazyMintHash', to, uri, tokenId, deadline);
      const digest = ethers.hashMessage(ethers.getBytes(hash));
      const signature = new ethers.Wallet(signerKey).signingKey.sign(digest);
      // ECDSA.recover expects r || s || v with v as a single byte.
      const v = ethers.getBytes('0x' + signature.v.toString(16).padStart(2, '0'));
      return ethers.concat([signature.r, signature.s, v]);
    }

    it('mints against a voucher signed by an authorised signer', async () => {
      const { chain, nft, signer } = await setup();
      const buyer = await chain.account('buyer');
      const signerKey = await chain.privateKeyOf(signer);
      const deadline = chain.timestamp + 3600n;

      const signature = await voucher(nft, chain, signerKey, {
        to: buyer,
        uri: 'ipfs://lazy',
        tokenId: 42n,
        deadline,
      });

      const { logs } = await nft.fnFrom(
        'lazyMint',
        ['ipfs://lazy', 42n, deadline, signature],
        { from: buyer },
      );

      const minted = logs.find((l) => l.name === 'LazyMinted');
      assert(minted, 'LazyMinted was not emitted');
      assertAddressEqual(minted.args[0], buyer);
      assertBigIntEqual(minted.args[1], 42n);
      assertAddressEqual(await nft.value('ownerOf', 42n), buyer);
      assertEqual(await nft.value('tokenURI', 42n), 'ipfs://lazy');
    });

    it('rejects a voucher with no signature', async () => {
      const { chain, nft } = await setup();
      const buyer = await chain.account('buyer');
      const deadline = chain.timestamp + 3600n;

      await assertReverts(
        nft.fnFrom('lazyMint', ['ipfs://x', 1n, deadline, '0x'], { from: buyer }),
        'BEP721: bad signature length',
      );
    });

    it('rejects a voucher signed by an address that is not authorised', async () => {
      const { chain, nft } = await setup();
      const buyer = await chain.account('buyer');
      const impostorKey = await chain.privateKeyOf(await chain.account('impostor'));
      const deadline = chain.timestamp + 3600n;

      const signature = await voucher(nft, chain, impostorKey, {
        to: buyer,
        uri: 'ipfs://x',
        tokenId: 1n,
        deadline,
      });

      await assertReverts(
        nft.fnFrom('lazyMint', ['ipfs://x', 1n, deadline, signature], { from: buyer }),
        'BEP721: unauthorised signer',
      );
    });

    it('rejects a voucher whose signature was issued for another buyer', async () => {
      const { chain, nft, signer } = await setup();
      const buyer = await chain.account('buyer');
      const attacker = await chain.account('attacker');
      const signerKey = await chain.privateKeyOf(signer);
      const deadline = chain.timestamp + 3600n;

      const signature = await voucher(nft, chain, signerKey, {
        to: buyer,
        uri: 'ipfs://x',
        tokenId: 7n,
        deadline,
      });

      await assertReverts(
        nft.fnFrom('lazyMint', ['ipfs://x', 7n, deadline, signature], { from: attacker }),
        'BEP721: unauthorised signer',
      );
    });

    it('rejects a voucher after its deadline', async () => {
      const { chain, nft, signer } = await setup();
      const buyer = await chain.account('buyer');
      const signerKey = await chain.privateKeyOf(signer);
      const deadline = chain.timestamp + 60n;

      const signature = await voucher(nft, chain, signerKey, {
        to: buyer,
        uri: 'ipfs://x',
        tokenId: 9n,
        deadline,
      });

      await chain.mine(1, { secondsPerBlock: 3600 });
      await assertReverts(
        nft.fnFrom('lazyMint', ['ipfs://x', 9n, deadline, signature], { from: buyer }),
        'BEP721: voucher expired',
      );
    });

    it('rejects a replayed voucher', async () => {
      const { chain, nft, signer } = await setup();
      const buyer = await chain.account('buyer');
      const signerKey = await chain.privateKeyOf(signer);
      const deadline = chain.timestamp + 3600n;

      const signature = await voucher(nft, chain, signerKey, {
        to: buyer,
        uri: 'ipfs://x',
        tokenId: 11n,
        deadline,
      });
      const args = ['ipfs://x', 11n, deadline, signature];

      await nft.fnFrom('lazyMint', args, { from: buyer });
      await assertReverts(
        nft.fnFrom('lazyMint', args, { from: buyer }),
        'BEP721: voucher redeemed',
      );
    });

    it('only lets an admin register a signer', async () => {
      const chain = await EvmChain.create();
      const nft = await deploy(chain);
      const mallory = await chain.account('mallory');

      await assertReverts(
        nft.fnFrom('setLazyMintSigner', [mallory, true], { from: mallory }),
        'AccessControlUnauthorizedAccount',
      );
      assertEqual(await nft.value('isLazyMintSigner', mallory), false);
    });
  });
});

describe('BEP1155Token', () => {
  async function deploy(chain) {
    return chain.deploy('bep1155/BEP1155:BEP1155Token');
  }

  it('creates a token type and mints balances', async () => {
    const chain = await EvmChain.create();
    const token = await deploy(chain);
    const alice = await chain.account('alice');

    await token.fn('create', 1n, 'ipfs://kind');
    assertEqual(await token.value('uri', 1n), 'ipfs://kind');

    await token.fn('mint', alice, 1n, 500n);
    assertBigIntEqual(await token.value('balanceOf', alice, 1n), 500n);
  });

  it('transfers a single token type', async () => {
    const chain = await EvmChain.create();
    const token = await deploy(chain);
    const alice = await chain.account('alice');
    const bob = await chain.account('bob');

    await token.fn('create', 1n, 'ipfs://kind');
    await token.fn('mint', alice, 1n, 100n);
    await token.fnFrom('safeTransferFrom', [alice, bob, 1n, 40n, '0x'], { from: alice });

    assertBigIntEqual(await token.value('balanceOf', alice, 1n), 60n);
    assertBigIntEqual(await token.value('balanceOf', bob, 1n), 40n);
  });

  it('transfers several token types in one batch', async () => {
    const chain = await EvmChain.create();
    const token = await deploy(chain);
    const alice = await chain.account('alice');
    const bob = await chain.account('bob');

    await token.fn('create', 1n, 'a');
    await token.fn('create', 2n, 'b');
    await token.fn('mint', alice, 1n, 10n);
    await token.fn('mint', alice, 2n, 20n);

    await token.fnFrom(
      'safeBatchTransferFrom',
      [alice, bob, [1n, 2n], [5n, 6n], '0x'],
      { from: alice },
    );

    assertBigIntEqual(await token.value('balanceOf', alice, 1n), 5n);
    assertBigIntEqual(await token.value('balanceOf', alice, 2n), 14n);
    assertBigIntEqual(await token.value('balanceOf', bob, 1n), 5n);
    assertBigIntEqual(await token.value('balanceOf', bob, 2n), 6n);
  });

  it('rejects a transfer larger than the balance', async () => {
    const chain = await EvmChain.create();
    const token = await deploy(chain);
    const alice = await chain.account('alice');
    const bob = await chain.account('bob');

    await token.fn('create', 1n, 'a');
    await token.fn('mint', alice, 1n, 1n);

    await assertReverts(
      token.fnFrom('safeTransferFrom', [alice, bob, 1n, 2n, '0x'], { from: alice }),
      'insufficient balance',
    );
  });

  it('burns a balance', async () => {
    const chain = await EvmChain.create();
    const token = await deploy(chain);
    const alice = await chain.account('alice');

    await token.fn('create', 1n, 'a');
    await token.fn('mint', alice, 1n, 10n);
    await token.fn('burn', alice, 1n, 4n);

    assertBigIntEqual(await token.value('balanceOf', alice, 1n), 6n);
  });
});
