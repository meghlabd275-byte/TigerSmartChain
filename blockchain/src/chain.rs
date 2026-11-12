//! Blockchain chain

use crate::block::{block_hash, validate_block, ValidationError};
use crate::types::*;
use std::collections::{BTreeMap, HashMap};

// =============================================================================
// CHAIN
// =============================================================================

/// A block tree with one canonical branch.
///
/// Keying blocks by number alone, as a single `number -> block` map does, makes
/// a competing fork unrepresentable: a second block at a height would overwrite
/// the first. A node built that way can only accept blocks that extend its
/// current head, so once two nodes disagree about a block they can never agree
/// again. Real chains keep every block they have seen, mark one branch
/// canonical, and switch branches when a better one arrives.
///
/// So blocks are held by hash, and a separate map records which hash is
/// canonical at each height. A side branch is a set of blocks present in the
/// tree but absent from the canonical map.
pub struct Blockchain {
    config: ChainConfig,
    /// Every block ever accepted, canonical or not, keyed by lower-case hash.
    blocks: HashMap<String, Block>,
    /// The canonical branch: height -> hash.
    canonical: BTreeMap<u64, String>,
    genesis: Option<Genesis>,
    /// Receipts per block, in transaction order, keyed by block hash. Keyed by
    /// hash rather than height so a reorg does not discard the receipts of the
    /// branch it abandoned: if that branch wins later they are still correct.
    receipts: HashMap<String, Vec<crate::receipt::Receipt>>,
    /// Cumulative work per block, the fork-choice score. A block whose parent
    /// is not in the tree starts from its own difficulty.
    total_difficulty: HashMap<String, u128>,
}

/// What happened when a block was offered to the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    /// The block extended the canonical branch and is now the head.
    Extended,
    /// The block was stored on a side branch. It is not canonical, so its
    /// transactions are not confirmed.
    SideBranch,
    /// The block was already known.
    Known,
    /// The block made a side branch heavier than the canonical one, so the
    /// canonical branch was switched. The old and new heads are reported along
    /// with the height at which the branches diverge, which is where a caller
    /// holding derived state must rewind to.
    Reorged {
        old_head: String,
        new_head: String,
        common_ancestor: u64,
    },
}

impl Blockchain {
    pub fn new(config: ChainConfig) -> Self {
        Self {
            config,
            blocks: HashMap::new(),
            canonical: BTreeMap::new(),
            genesis: None,
            receipts: HashMap::new(),
            total_difficulty: HashMap::new(),
        }
    }

    /// The chain configuration.
    pub fn config(&self) -> &ChainConfig {
        &self.config
    }

    /// Install the genesis specification.
    pub fn set_genesis(&mut self, genesis: Genesis) {
        self.genesis = Some(genesis);
    }

    /// The genesis specification, if one was installed.
    pub fn genesis(&self) -> Option<&Genesis> {
        self.genesis.as_ref()
    }

    /// The canonical block at `number`.
    pub fn get_block(&self, number: u64) -> Option<&Block> {
        let hash = self.canonical.get(&number)?;
        self.blocks.get(hash)
    }

    /// Any block in the tree by hash, canonical or not.
    pub fn get_block_by_hash(&self, hash: &str) -> Option<&Block> {
        self.blocks.get(&hash.to_ascii_lowercase())
    }

    /// Whether `hash` names a block in the tree.
    pub fn contains_block(&self, hash: &str) -> bool {
        self.blocks.contains_key(&hash.to_ascii_lowercase())
    }

    /// Whether `hash` is on the canonical branch.
    pub fn is_canonical(&self, hash: &str) -> bool {
        self.canonical.values().any(|h| h.eq_ignore_ascii_case(hash))
    }

    /// The canonical block at the current height.
    pub fn latest_block(&self) -> Option<&Block> {
        self.canonical
            .values()
            .next_back()
            .and_then(|hash| self.blocks.get(hash))
    }

    /// Insert a block and its receipts, running fork choice.
    ///
    /// The block is stored whether or not it is canonical: a side branch may
    /// win later, and a block that was discarded cannot be used to build on.
    /// The returned [`InsertOutcome`] says whether the canonical branch moved.
    pub fn insert_block_with_receipts(
        &mut self,
        block: Block,
        receipts: Vec<crate::receipt::Receipt>,
    ) -> Result<InsertOutcome, ValidationError> {
        let hash = block_hash(&block.header)?;
        let key = hash.to_ascii_lowercase();

        if self.blocks.contains_key(&key) {
            return Ok(InsertOutcome::Known);
        }

        // Cumulative work: the parent's, plus this block's own difficulty.
        let difficulty = decode_difficulty(&block.header.difficulty);
        let parent_work = self
            .total_difficulty
            .get(&block.header.parent_hash.to_ascii_lowercase())
            .copied()
            .unwrap_or(0);
        self.total_difficulty
            .insert(key.clone(), parent_work.saturating_add(difficulty));
        self.receipts.insert(key.clone(), receipts);
        self.blocks.insert(key.clone(), block.clone());

        let number = block.header.number;
        let parent_hash = block.header.parent_hash.to_ascii_lowercase();

        // Genesis seeds the canonical branch.
        if self.canonical.is_empty() && number == 0 {
            self.canonical.insert(0, key);
            return Ok(InsertOutcome::Extended);
        }

        // A block extends the canonical branch only when it builds directly on
        // the current head. Testing only that its parent is canonical is not
        // enough: at height 1 a competing block's parent is genesis, which is
        // canonical, so it would be mistaken for an extension and would
        // overwrite the head.
        let head_hash = self.canonical.values().next_back().cloned();
        let extends_canonical = match &head_hash {
            Some(head) => head == &parent_hash && number == self.height() + 1,
            None => false,
        };

        if extends_canonical {
            self.canonical.insert(number, key);
            return Ok(InsertOutcome::Extended);
        }

        // A side branch. Whether it wins is decided by fork choice, which for a
        // proof-of-staked-authority chain is the longest chain: the branch that
        // reaches the greatest height is the chain. Cumulative difficulty is
        // the second term, so a chain that assigns difficulty to its blocks is
        // also handled, and the block hash is the final tie-break, so two
        // branches of equal length cannot leave nodes split indefinitely on
        // whichever they happened to see first.
        let head_key = self
            .canonical
            .values()
            .next_back()
            .cloned()
            .unwrap_or_default();
        let head_height = self.height();
        let head_work = self
            .total_difficulty
            .get(&head_key)
            .copied()
            .unwrap_or(0);
        let this_work = self.total_difficulty.get(&key).copied().unwrap_or(0);

        let candidate_wins = number > head_height
            || (number == head_height && this_work > head_work)
            || (number == head_height && this_work == head_work && key < head_key);

        if !candidate_wins {
            return Ok(InsertOutcome::SideBranch);
        }

        let old_head = head_key;
        let common_ancestor = self.switch_canonical_to(&key)?;
        Ok(InsertOutcome::Reorged {
            old_head,
            new_head: hash,
            common_ancestor,
        })
    }

    /// Insert a block with no receipts, running fork choice.
    pub fn insert_block(&mut self, block: Block) -> Result<InsertOutcome, ValidationError> {
        self.insert_block_with_receipts(block, Vec::new())
    }

    /// Make `target`'s branch canonical, returning the height at which it
    /// diverges from the branch it replaces.
    ///
    /// Walking back from `target` until a block already on the canonical branch
    /// is reached yields the new branch and the common ancestor. Everything
    /// above the ancestor is replaced in one pass, so the canonical map is
    /// never observed half-switched.
    fn switch_canonical_to(&mut self, target: &str) -> Result<u64, ValidationError> {
        let mut branch = Vec::new();
        let mut cursor = target.to_string();
        let ancestor;

        loop {
            if self.is_canonical(&cursor) {
                ancestor = self
                    .blocks
                    .get(&cursor)
                    .map(|b| b.header.number)
                    .unwrap_or(0);
                break;
            }
            let block = self.blocks.get(&cursor).ok_or(ValidationError::MalformedField(
                "fork choice: missing block in tree",
            ))?;
            let number = block.header.number;
            let parent = block.header.parent_hash.to_ascii_lowercase();
            branch.push((number, cursor.clone()));
            if number == 0 {
                ancestor = 0;
                break;
            }
            cursor = parent;
        }

        // The branch was collected head-first; inserting it reversed keeps the
        // map ordered at every step.
        self.canonical.retain(|height, _| *height <= ancestor);
        for (number, hash) in branch.into_iter().rev() {
            self.canonical.insert(number, hash);
        }
        Ok(ancestor)
    }

    /// The canonical branch as `(height, block)` pairs, ascending.
    pub fn canonical_blocks(&self) -> Vec<(u64, &Block)> {
        self.canonical
            .iter()
            .filter_map(|(height, hash)| self.blocks.get(hash).map(|b| (*height, b)))
            .collect()
    }

    /// The canonical receipts for a block number, in transaction order.
    pub fn receipts(&self, number: u64) -> Option<&[crate::receipt::Receipt]> {
        let hash = self.canonical.get(&number)?;
        self.receipts.get(hash).map(|r| r.as_slice())
    }

    /// The receipts for a block named by hash, canonical or not.
    pub fn receipts_by_hash(&self, hash: &str) -> Option<&[crate::receipt::Receipt]> {
        self.receipts
            .get(&hash.to_ascii_lowercase())
            .map(|r| r.as_slice())
    }

    /// Attach receipts to the canonical block at `number`.
    ///
    /// Used when restoring a persisted chain, where receipts are loaded rather
    /// than recomputed.
    pub fn set_receipts(&mut self, number: u64, receipts: Vec<crate::receipt::Receipt>) {
        if let Some(hash) = self.canonical.get(&number) {
            self.receipts.insert(hash.clone(), receipts);
        }
    }

    /// Attach receipts to a block named by hash.
    pub fn set_receipts_by_hash(&mut self, hash: &str, receipts: Vec<crate::receipt::Receipt>) {
        self.receipts.insert(hash.to_ascii_lowercase(), receipts);
    }

    /// Locate a canonical transaction by hash: its block, its index in that
    /// block, and the block hash.
    ///
    /// Only the canonical branch is searched. A transaction that appears only
    /// on an abandoned branch is not part of the chain and must not be reported
    /// as if it were, or a wallet would see a confirmation that never happened.
    pub fn find_transaction(&self, hash: &str) -> Option<(&Block, usize, String)> {
        for block_hash in self.canonical.values().rev() {
            let block = self.blocks.get(block_hash)?;
            if let Some(index) = block
                .transactions
                .iter()
                .position(|tx| tx.hash.eq_ignore_ascii_case(hash))
            {
                return Some((block, index, block_hash.clone()));
            }
        }
        None
    }

    /// The current height.
    ///
    /// An empty chain reports 0, which is also the number of the genesis
    /// block; use [`Blockchain::is_empty`] to distinguish the two.
    pub fn height(&self) -> u64 {
        self.canonical.keys().next_back().copied().unwrap_or(0)
    }

    /// Whether the chain holds no canonical blocks.
    pub fn is_empty(&self) -> bool {
        self.canonical.is_empty()
    }

    /// The canonical header at `number`.
    pub fn get_header(&self, number: u64) -> Option<&BlockHeader> {
        self.get_block(number).map(|b| &b.header)
    }

    /// How many blocks the tree holds, canonical and side branches alike.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// The canonical hash at `number`, if the branch reaches it.
    pub fn canonical_hash(&self, number: u64) -> Option<&str> {
        self.canonical.get(&number).map(|h| h.as_str())
    }

    /// Every block in the tree with its hash, canonical and side branches
    /// alike. A persistent store writes the whole tree, because a side branch
    /// that is not on disk cannot be built on after a restart.
    pub fn all_blocks(&self) -> Vec<(String, &Block)> {
        self.blocks.iter().map(|(h, b)| (h.clone(), b)).collect()
    }

    /// Cumulative work up to and including the block named by `hash`.
    pub fn total_difficulty_of(&self, hash: &str) -> Option<u128> {
        self.total_difficulty
            .get(&hash.to_ascii_lowercase())
            .copied()
    }

    /// Replace the canonical branch with `entries`.
    ///
    /// Used when loading a chain from disk, where the stored map is the
    /// authority on which branch was canonical. Re-deriving it by re-running
    /// fork choice would depend on the order blocks are offered in and could
    /// pick a different branch than the one the node was following.
    pub fn restore_canonical(&mut self, entries: &[(u64, String)]) {
        self.canonical.clear();
        for (height, hash) in entries {
            self.canonical
                .insert(*height, hash.to_ascii_lowercase());
        }
    }

    /// Restore a block's cumulative work, for a chain being loaded from disk.
    ///
    /// Fork choice compares branches by this value, so a reloaded chain that
    /// did not restore it would score every block at zero and could pick a
    /// different branch than the one it had before the restart.
    pub fn set_total_difficulty(&mut self, hash: &str, work: u128) {
        self.total_difficulty
            .insert(hash.to_ascii_lowercase(), work);
    }
}

/// Decode a header's `difficulty` field, which may be decimal or hex.
fn decode_difficulty(value: &str) -> u128 {
    let trimmed = value.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u128::from_str_radix(hex, 16).unwrap_or(0)
    } else {
        trimmed.parse::<u128>().unwrap_or(0)
    }
}

impl Default for Blockchain {
    fn default() -> Self {
        Self::new(ChainConfig::default())
    }
}

// =============================================================================
// CHAIN MANAGER
// =============================================================================

/// Chain Manager
pub struct ChainManager {
    chain: Blockchain,
}

impl ChainManager {
    pub fn new(config: ChainConfig) -> Self {
        Self {
            chain: Blockchain::new(config),
        }
    }

    /// Get chain
    pub fn chain(&self) -> &Blockchain {
        &self.chain
    }

    /// Get chain mut
    pub fn chain_mut(&mut self) -> &mut Blockchain {
        &mut self.chain
    }

    /// Validate a block against the current chain head.
    ///
    /// A block whose number is 0 is treated as genesis and validated without a
    /// parent; any other block is checked against the block at number - 1.
    pub fn validate_block(&self, block: &Block) -> Result<(), ValidationError> {
        let parent = if block.header.number == 0 {
            None
        } else {
            self.chain.get_block(block.header.number - 1)
        };
        validate_block(block, parent, &self.chain.config)?;
        // A block is only acceptable if the validator it names actually sealed
        // it. Checking the `miner` field alone would let anyone produce a block
        // in any validator's name.
        crate::seal::verify_seal(&block.header)
    }

    /// Validate a block against the block it names as its parent, wherever that
    /// parent sits in the tree.
    ///
    /// [`ChainManager::validate_block`] compares against the block at
    /// `number - 1` on the canonical branch, which is right for a block that
    /// extends the head but wrong for one on a side branch: its parent is a
    /// different block at the same height. Validating against the named parent
    /// keeps a fork's blocks checked against their own ancestors.
    pub fn validate_block_against_tree(&self, block: &Block) -> Result<(), ValidationError> {
        let parent = if block.header.number == 0 {
            None
        } else {
            self.chain.get_block_by_hash(&block.header.parent_hash)
        };
        // A block whose parent is unknown cannot be linked yet, but it is still
        // well-formed, and the seal and internal consistency are checked
        // either way, so a malformed block is rejected even when it arrives
        // before its parent.
        validate_block(block, parent, self.chain.config())?;
        crate::seal::verify_seal(&block.header)
    }

    /// Check a block's declared `receiptsRoot` and `logsBloom` against the
    /// receipts its execution produced.
    pub fn check_receipts(
        &self,
        block: &Block,
        receipts: &[crate::receipt::Receipt],
    ) -> Result<(), ValidationError> {
        if receipts.len() != block.transactions.len() {
            return Err(ValidationError::ReceiptsRootMismatch {
                expected: format!("{} receipts", block.transactions.len()),
                found: format!("{}", receipts.len()),
            });
        }

        let expected_root = crate::receipt::receipts_root(receipts);
        let declared_root = crate::block::decode_hex_field(&block.header.receipts_root, Some(32))?;
        if declared_root != expected_root {
            return Err(ValidationError::ReceiptsRootMismatch {
                expected: crate::block::hex_encode(&expected_root),
                found: block
                    .header
                    .receipts_root
                    .trim_start_matches("0x")
                    .to_string(),
            });
        }

        let expected_bloom = crate::receipt::logs_bloom(receipts);
        let declared_bloom = crate::block::decode_hex_field(&block.header.logs_bloom, Some(256))?;
        if declared_bloom != expected_bloom {
            return Err(ValidationError::LogsBloomMismatch);
        }

        Ok(())
    }

    /// Validate then insert a block.
    ///
    /// Validation happens before insertion so a rejected block cannot leave the
    /// chain in a partially updated state.
    pub fn process_block(&mut self, block: Block) -> Result<(), ValidationError> {
        self.validate_block(&block)?;
        self.chain.insert_block(block)?;
        Ok(())
    }

    /// Validate, insert, and report what happened to the canonical branch.
    ///
    /// A block on a losing side branch is accepted into the tree but does not
    /// become the head. A caller that maintains derived state (a world state,
    /// an index) must use this and act on [`InsertOutcome::Reorged`], because
    /// only it knows how to rewind.
    pub fn process_block_with_receipts_outcome(
        &mut self,
        block: Block,
        receipts: Vec<crate::receipt::Receipt>,
    ) -> Result<InsertOutcome, ValidationError> {
        self.validate_block_against_tree(&block)?;
        self.check_receipts(&block, &receipts)?;
        self.chain.insert_block_with_receipts(block, receipts)
    }

    /// Validate and insert a block together with the receipts its execution
    /// produced.
    ///
    /// The declared `receiptsRoot` and `logsBloom` are checked against the
    /// receipts before insertion: a block whose header commits to different
    /// receipts than the ones it actually produced is rejected, exactly as a
    /// real client rejects it. This is what makes the receipts part of
    /// consensus rather than a local annotation.
    pub fn process_block_with_receipts(
        &mut self,
        block: Block,
        receipts: Vec<crate::receipt::Receipt>,
    ) -> Result<(), ValidationError> {
        self.validate_block(&block)?;
        self.check_receipts(&block, &receipts)?;
        self.chain.insert_block_with_receipts(block, receipts)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{hex_encode, transactions_root};
    use crate::types::{BlockHeader, Transaction};

    fn h(byte: u8) -> String {
        format!("0x{}", hex_encode(&[byte; 32]))
    }

    fn tx() -> Transaction {
        Transaction {
            hash: String::new(),
            nonce: 1,
            block_hash: None,
            block_number: None,
            transaction_index: None,
            from: format!("0x{}", hex_encode(&[0x11; 20])),
            to: Some(format!("0x{}", hex_encode(&[0xdd; 20]))),
            value: "0x3e8".to_string(),
            gas_price: 2_000_000_000,
            gas: 21_000,
            input: "0x".to_string(),
            v: "0x25".to_string(),
            r: "0x01".to_string(),
            s: "0x02".to_string(),
            tx_type: 0,
            access_list: None,
            max_priority_fee_per_gas: None,
            chain_id: None,
        }
    }

    fn genesis() -> Block {
        Block {
            header: BlockHeader {
                parent_hash: h(0x00),
                sha3_uncles: h(0xbb),
                miner: format!("0x{}", hex_encode(&[0xcc; 20])),
                state_root: h(0xdd),
                transactions_root: format!("0x{}", hex_encode(&transactions_root(&[]).unwrap())),
                receipts_root: h(0xff),
                logs_bloom: format!("0x{}", hex_encode(&[0x00; 256])),
                difficulty: "0x1".to_string(),
                number: 0,
                gas_limit: 30_000_000,
                gas_used: 0,
                timestamp: 1_700_000_000,
                extra_data: "0x".to_string(),
                mix_hash: h(0x12),
                nonce: "0x0000000000000000".to_string(),
                base_fee_per_gas: Some(1_000_000_000),
                withdrawals_root: None,
            },
            transactions: Vec::new(),
            uncles: Vec::new(),
        }
    }

    /// The key every hand-built test block is sealed with. Real blocks are
    /// sealed by the proposing validator's key; a test needs one it controls.
    const TEST_KEY: [u8; 32] = [0x5c; 32];

    /// Seal `block` with [`TEST_KEY`] and set its `miner` to the matching
    /// address, which is what a real producer does.
    fn seal(block: &mut Block) {
        let address = tiger_crypto::private_key_to_address(&TEST_KEY).unwrap();
        block.header.miner = format!("0x{}", hex_encode(&address));
        block.header.extra_data = crate::seal::seal_header(&block.header, &TEST_KEY).unwrap();
    }

    fn child_of(parent: &Block, timestamp: u64) -> Block {
        let mut block = genesis();
        block.header.number = parent.header.number + 1;
        block.header.parent_hash = crate::block::block_hash(&parent.header).unwrap();
        block.header.timestamp = timestamp;
        block.header.base_fee_per_gas = Some(crate::block::next_base_fee(
            parent.header.base_fee_per_gas.unwrap_or(0),
            parent.header.gas_used,
            parent.header.gas_limit,
        ));
        seal(&mut block);
        block
    }

    #[test]
    fn empty_chain_reports_zero_height_and_is_empty() {
        let chain = Blockchain::new(ChainConfig::default());
        assert_eq!(chain.height(), 0);
        assert!(chain.is_empty());
    }

    #[test]
    fn genesis_can_be_installed_and_read_back() {
        let mut chain = Blockchain::new(ChainConfig::default());
        assert!(chain.genesis().is_none());
        chain.set_genesis(Genesis {
            config: ChainConfig::default(),
            nonce: 0,
            timestamp: "0x0".to_string(),
            extra_data: "0x".to_string(),
            gas_limit: "0x1c9c380".to_string(),
            difficulty: "0x1".to_string(),
            mix_hash: "0x0".to_string(),
            coinbase: format!("0x{}", hex_encode(&[0u8; 20])),
            alloc: std::collections::HashMap::new(),
        });
        assert!(chain.genesis().is_some());
    }

    #[test]
    fn process_block_rejects_invalid_and_accepts_valid() {
        let mut manager = ChainManager::new(ChainConfig {
            chain_id: 6666,
            london_block: Some(0),
            ..Default::default()
        });

        let genesis = genesis();
        manager.process_block(genesis.clone()).unwrap();
        assert_eq!(manager.chain().height(), 0);
        assert!(!manager.chain().is_empty());

        // A block with a bogus parent hash must be rejected and not inserted.
        let mut bad = child_of(&genesis, 1_700_000_012);
        bad.header.parent_hash = h(0x99);
        assert!(manager.process_block(bad).is_err());
        assert_eq!(manager.chain().height(), 0);

        // A correctly linked block must be accepted.
        let good = child_of(&genesis, 1_700_000_012);
        manager.process_block(good).unwrap();
        assert_eq!(manager.chain().height(), 1);
    }

    #[test]
    fn lookup_by_hash_finds_the_block() {
        let mut manager = ChainManager::new(ChainConfig::default());
        let genesis = genesis();
        let hash = crate::block::block_hash(&genesis.header).unwrap();
        manager.process_block(genesis).unwrap();
        assert!(manager.chain().get_block_by_hash(&hash).is_some());
        assert!(manager.chain().get_block_by_hash(&h(0xab)).is_none());
    }

    #[test]
    fn transaction_missing_from_the_root_is_detected_on_processing() {
        let mut manager = ChainManager::new(ChainConfig::default());
        manager.process_block(genesis()).unwrap();

        let mut block = child_of(&genesis(), 1_700_000_012);
        block.transactions = vec![tx()];
        // Header still declares the empty transactions root, so the block
        // must be rejected.
        assert!(manager.process_block(block).is_err());
    }

    /// Build a sealed block at `number` extending `parent`, with a distinct
    /// timestamp so two candidates at the same height get different hashes.
    fn sealed_child(parent: &Block, number: u64, timestamp: u64) -> Block {
        let mut block = child_of(parent, timestamp);
        block.header.number = number;
        seal(&mut block);
        block
    }

    /// Build a sealed sibling of `reference` — same parent, same height — whose
    /// hash is strictly greater, so fork choice is guaranteed to prefer
    /// `reference` over it.
    ///
    /// The timestamp is varied until the hash ordering comes out as wanted.
    /// Which timestamp does that is not knowable in advance, so it is searched
    /// rather than guessed.
    fn sealed_sibling_losing_to(parent: &Block, reference: &Block, base_timestamp: u64) -> Block {
        let reference_hash = crate::block::block_hash(&reference.header).unwrap();
        for step in 0..10_000u64 {
            let mut block = child_of(parent, base_timestamp + step);
            block.header.number = reference.header.number;
            seal(&mut block);
            if crate::block::block_hash(&block.header).unwrap() > reference_hash {
                return block;
            }
        }
        panic!("could not build a sibling that loses the hash tie-break");
    }

    #[test]
    fn a_competing_fork_is_kept_as_a_side_branch() {
        let mut manager = ChainManager::new(ChainConfig::default());
        let g = genesis();
        manager.process_block(g.clone()).unwrap();

        let a1 = sealed_child(&g, 1, 1_700_000_100);
        assert_eq!(
            manager.chain_mut().insert_block(a1.clone()).unwrap(),
            InsertOutcome::Extended
        );

        // A different block at the same height, built on genesis, whose hash
        // is greater so it must lose the tie-break.
        let b1 = sealed_sibling_losing_to(&g, &a1, 1_700_000_200);
        assert_eq!(
            manager.chain_mut().insert_block(b1.clone()).unwrap(),
            InsertOutcome::SideBranch,
            "a competing block at equal height must not become the head"
        );

        let chain = manager.chain();
        // Both branches survive, so a block building on either can be checked.
        assert!(chain.get_block_by_hash(&crate::block::block_hash(&b1.header).unwrap()).is_some());
        assert_eq!(chain.height(), 1);
        assert_eq!(
            crate::block::block_hash(&chain.get_block(1).unwrap().header).unwrap(),
            crate::block::block_hash(&a1.header).unwrap(),
            "the head must still be a1"
        );
        assert!(!chain.is_canonical(&crate::block::block_hash(&b1.header).unwrap()));
    }

    #[test]
    fn a_heavier_fork_reorganises_the_chain() {
        let mut manager = ChainManager::new(ChainConfig::default());
        let g = genesis();
        manager.process_block(g.clone()).unwrap();

        // Branch A: genesis -> a1, and it is canonical to begin with.
        let a1 = sealed_child(&g, 1, 1_700_000_100);
        manager.chain_mut().insert_block(a1.clone()).unwrap();
        assert!(manager.chain().is_canonical(
            &crate::block::block_hash(&a1.header).unwrap()
        ));
        assert_eq!(manager.chain().height(), 1);

        // Branch B: genesis -> b1 -> b2. Two blocks beat one, so B must win
        // whichever hashes the blocks happen to have.
        let b1 = sealed_sibling_losing_to(&g, &a1, 1_700_000_200);
        assert_eq!(
            manager.chain_mut().insert_block(b1.clone()).unwrap(),
            InsertOutcome::SideBranch,
            "b1 alone is not enough to displace a1"
        );

        let b2 = sealed_child(&b1, 2, 1_700_000_201);
        let outcome = manager.chain_mut().insert_block(b2.clone()).unwrap();

        match outcome {
            InsertOutcome::Reorged {
                common_ancestor,
                new_head,
                ..
            } => {
                assert_eq!(common_ancestor, 0, "the branches diverge at genesis");
                assert_eq!(
                    new_head,
                    crate::block::block_hash(&b2.header).unwrap().to_ascii_lowercase()
                );
            }
            other => panic!("expected a reorg, got {other:?}"),
        }

        let chain = manager.chain();
        assert_eq!(chain.height(), 2);
        // The canonical block at height 1 is now B's, not A's.
        assert_eq!(
            crate::block::block_hash(&chain.get_block(1).unwrap().header).unwrap(),
            crate::block::block_hash(&b1.header).unwrap()
        );
        // The abandoned branch is still in the tree, so it could win later.
        assert!(chain.get_block_by_hash(&crate::block::block_hash(&a1.header).unwrap()).is_some());
        assert!(!chain.is_canonical(&crate::block::block_hash(&a1.header).unwrap()));
    }

    /// Two branches of equal length must not leave a node undecided: the lower
    /// block hash is canonical, whichever order the two arrived in. Without a
    /// tie-break, two nodes that saw the branches in opposite orders would
    /// disagree forever.
    #[test]
    fn equal_length_forks_are_broken_by_hash_identically_in_either_order() {
        let g = genesis();

        let mut forward = ChainManager::new(ChainConfig::default());
        forward.process_block(g.clone()).unwrap();
        let a1 = sealed_child(&g, 1, 1_700_000_300);
        let b1 = sealed_child(&g, 1, 1_700_000_400);
        forward.chain_mut().insert_block(a1.clone()).unwrap();
        forward.chain_mut().insert_block(b1.clone()).unwrap();

        let mut backward = ChainManager::new(ChainConfig::default());
        backward.process_block(g.clone()).unwrap();
        backward.chain_mut().insert_block(b1.clone()).unwrap();
        backward.chain_mut().insert_block(a1.clone()).unwrap();

        let forward_head = crate::block::block_hash(&forward.chain().get_block(1).unwrap().header)
            .unwrap()
            .to_ascii_lowercase();
        let backward_head =
            crate::block::block_hash(&backward.chain().get_block(1).unwrap().header)
                .unwrap()
                .to_ascii_lowercase();

        assert_eq!(
            forward_head, backward_head,
            "both nodes must choose the same branch regardless of arrival order"
        );

        let expected = {
            let ah = crate::block::block_hash(&a1.header).unwrap().to_ascii_lowercase();
            let bh = crate::block::block_hash(&b1.header).unwrap().to_ascii_lowercase();
            if ah < bh { ah } else { bh }
        };
        assert_eq!(forward_head, expected, "the lower hash must win the tie");
    }

    /// After a reorg, a transaction that existed only on the abandoned branch
    /// must no longer be reported as confirmed.
    #[test]
    fn a_reorg_removes_abandoned_transactions_from_lookups() {
        let mut manager = ChainManager::new(ChainConfig::default());
        let g = genesis();
        manager.process_block(g.clone()).unwrap();

        let mut a1 = sealed_child(&g, 1, 1_700_000_100);
        let mut orphan = tx();
        orphan.hash = format!("0x{}", hex_encode(&[0x77; 32]));
        a1.transactions = vec![orphan.clone()];
        manager.chain_mut().insert_block(a1.clone()).unwrap();
        assert!(manager.chain().find_transaction(&orphan.hash).is_some());

        // Reorg onto a heavier branch that does not contain the transaction.
        let b1 = sealed_sibling_losing_to(&g, &a1, 1_700_000_200);
        manager.chain_mut().insert_block(b1.clone()).unwrap();
        let b2 = sealed_child(&b1, 2, 1_700_000_201);
        assert!(matches!(
            manager.chain_mut().insert_block(b2).unwrap(),
            InsertOutcome::Reorged { .. }
        ));

        assert!(
            manager.chain().find_transaction(&orphan.hash).is_none(),
            "a transaction only on an abandoned branch must not look confirmed"
        );
    }

    #[test]
    fn a_side_branch_block_is_validated_against_its_own_parent() {
        let mut manager = ChainManager::new(ChainConfig::default());
        let g = genesis();
        manager.process_block(g.clone()).unwrap();

        let a1 = sealed_child(&g, 1, 1_700_000_100);
        manager.chain_mut().insert_block(a1.clone()).unwrap();
        let b1 = sealed_sibling_losing_to(&g, &a1, 1_700_000_200);
        manager.chain_mut().insert_block(b1.clone()).unwrap();

        // b2 names b1 as its parent. Against the canonical branch the block at
        // height 1 is a1, so a canonical-only check would reject b2 as not
        // linking to its parent; against the tree it is correctly linked.
        let b2 = sealed_child(&b1, 2, b1.header.timestamp + 1);
        assert!(manager.validate_block_against_tree(&b2).is_ok());
        assert!(
            manager.validate_block(&b2).is_err(),
            "the canonical-only check must still reject it, which is why the tree check exists"
        );
    }

    #[test]
    fn re_inserting_a_known_block_is_reported_as_known() {
        let mut manager = ChainManager::new(ChainConfig::default());
        let g = genesis();
        manager.process_block(g.clone()).unwrap();
        let a1 = sealed_child(&g, 1, 1_700_000_100);
        manager.chain_mut().insert_block(a1.clone()).unwrap();

        assert_eq!(
            manager.chain_mut().insert_block(a1).unwrap(),
            InsertOutcome::Known
        );
        assert_eq!(manager.chain().block_count(), 2, "no duplicate is stored");
    }
}
