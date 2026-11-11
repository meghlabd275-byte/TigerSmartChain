//! Blockchain chain

use crate::block::{block_hash, validate_block, ValidationError};
use crate::types::*;
use std::collections::HashMap;

// =============================================================================
// CHAIN
// =============================================================================

/// An in-memory view of the chain, keyed by block number.
pub struct Blockchain {
    config: ChainConfig,
    blocks: HashMap<u64, Block>,
    headers: HashMap<u64, BlockHeader>,
    genesis: Option<Genesis>,
    /// Receipts per block, in transaction order. Kept alongside the block
    /// because they are not part of the block body but are needed to answer
    /// `eth_getTransactionReceipt` and `eth_getLogs`.
    receipts: HashMap<u64, Vec<crate::receipt::Receipt>>,
}

impl Blockchain {
    pub fn new(config: ChainConfig) -> Self {
        Self {
            config,
            blocks: HashMap::new(),
            headers: HashMap::new(),
            genesis: None,
            receipts: HashMap::new(),
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

    /// Get a block by number.
    pub fn get_block(&self, number: u64) -> Option<&Block> {
        self.blocks.get(&number)
    }

    /// Get a block by hash.
    pub fn get_block_by_hash(&self, hash: &str) -> Option<&Block> {
        self.blocks
            .values()
            .find(|b| block_hash(&b.header).map(|h| h.eq_ignore_ascii_case(hash)).unwrap_or(false))
    }

    /// The most recent block, if any.
    pub fn latest_block(&self) -> Option<&Block> {
        self.blocks.get(&self.height())
    }

    /// Insert a block, indexing both its number and its header.
    pub fn insert_block(&mut self, block: Block) {
        let number = block.header.number;
        self.headers.insert(number, block.header.clone());
        self.blocks.insert(number, block);
    }

    /// Insert a block together with the receipts its transactions produced.
    pub fn insert_block_with_receipts(
        &mut self,
        block: Block,
        receipts: Vec<crate::receipt::Receipt>,
    ) {
        let number = block.header.number;
        self.receipts.insert(number, receipts);
        self.insert_block(block);
    }

    /// The receipts for a block, in transaction order.
    pub fn receipts(&self, number: u64) -> Option<&[crate::receipt::Receipt]> {
        self.receipts.get(&number).map(|r| r.as_slice())
    }

    /// Locate a transaction by hash: its block, its index in that block, and
    /// the block hash. Used to serve `eth_getTransactionByHash`.
    pub fn find_transaction(&self, hash: &str) -> Option<(&Block, usize, String)> {
        for block in self.blocks.values() {
            if let Some(index) = block
                .transactions
                .iter()
                .position(|tx| tx.hash.eq_ignore_ascii_case(hash))
            {
                let block_hash = block_hash(&block.header).ok()?;
                return Some((block, index, block_hash));
            }
        }
        None
    }

    /// Get the current height.
    ///
    /// An empty chain reports 0, which is also the number of the genesis
    /// block; use [`Blockchain::is_empty`] to distinguish the two.
    pub fn height(&self) -> u64 {
        self.blocks.keys().max().copied().unwrap_or(0)
    }

    /// Whether the chain holds no blocks.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Get a header by number.
    pub fn get_header(&self, number: u64) -> Option<&BlockHeader> {
        self.headers.get(&number)
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
        validate_block(block, parent, &self.chain.config)
    }

    /// Validate then insert a block.
    ///
    /// Validation happens before insertion so a rejected block cannot leave the
    /// chain in a partially updated state.
    pub fn process_block(&mut self, block: Block) -> Result<(), ValidationError> {
        self.validate_block(&block)?;
        self.chain.insert_block(block);
        Ok(())
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

        if receipts.len() != block.transactions.len() {
            return Err(ValidationError::ReceiptsRootMismatch {
                expected: format!("{} receipts", block.transactions.len()),
                found: format!("{}", receipts.len()),
            });
        }

        let expected_root = crate::receipt::receipts_root(&receipts);
        let declared_root = crate::block::decode_hex_field(&block.header.receipts_root, Some(32))?;
        if declared_root != expected_root {
            return Err(ValidationError::ReceiptsRootMismatch {
                expected: crate::block::hex_encode(&expected_root),
                found: block.header.receipts_root.trim_start_matches("0x").to_string(),
            });
        }

        let expected_bloom = crate::receipt::logs_bloom(&receipts);
        let declared_bloom = crate::block::decode_hex_field(&block.header.logs_bloom, Some(256))?;
        if declared_bloom != expected_bloom {
            return Err(ValidationError::LogsBloomMismatch);
        }

        self.chain.insert_block_with_receipts(block, receipts);
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

    fn child_of(parent: &Block, timestamp: u64) -> Block {
        let mut block = genesis();
        block.header.number = parent.header.number + 1;
        block.header.parent_hash = crate::block::block_hash(&parent.header).unwrap();
        block.header.timestamp = timestamp;
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
}
