//! Block and Transaction Visualizer Service

use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use ethers::providers::Http;
use ethers::types::{Address, Block, Transaction};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VisualizerError { #[error("RPC error: {0}")] Rpc(String), #[error("Not found: {0}")] NotFound(String) }

#[derive(Debug, Clone, Deserialize)]
pub struct Config { pub rpc_url: String }
impl Default for Config { fn default() -> Self { Self { rpc_url: std::env::var("RPC_URL").unwrap_or_else(|_| "http://localhost:8545".to_string()) } } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockVisualization { pub number: u64, pub hash: String, pub timestamp: i64, pub transactions: Vec<TxVisualization>, pub gas_used: u64, pub gas_limit: u64, pub miner: String, pub size: usize }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxVisualization { pub hash: String, pub from: String, pub to: String, pub value: String, pub gas: u64, pub status: bool, pub call_type: String, pub internal_calls: Vec<InternalCall> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalCall { pub from: String, pub to: String, pub value: String, pub call_type: String, pub depth: u32 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionFlow { pub tx_hash: String, pub calls: Vec<CallNode>, pub flow_type: FlowType }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallNode { pub id: u32, pub call_type: String, pub from: String, pub to: String, pub value: String, pub depth: u32, pub children: Vec<u32> }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowType { Transfer, ContractCall, TokenTransfer, NFTTransfer, Swap, Staking, Governance }

pub struct VisualizerService { rpc: ethers::providers::Provider<Http>, state: Arc<RwLock<VisualizerState>> }
#[derive(Debug)]
pub struct VisualizerState { pub cache: HashMap<u64, BlockVisualization>, pub tx_cache: HashMap<String, TransactionFlow> }

impl VisualizerService {
    pub async fn new(config: Config) -> Result<Self, anyhow::Error> {
        let rpc = ethers::providers::Provider::<Http>::try_from(config.rpc_url)?;
        Ok(Self { rpc, state: Arc::new(RwLock::new(VisualizerState { cache: HashMap::new(), tx_cache: HashMap::new() })) })
    }
    pub async fn get_block_visualization(&self, block_number: u64) -> Result<BlockVisualization, VisualizerError> {
        let block = self.rpc.get_block_with_txs(block_number).await.map_err(|e| VisualizerError::Rpc(e.to_string()))?;
        let block = block.ok_or_else(|| VisualizerError::NotFound(block_number.to_string()))?;
        let txs: Vec<TxVisualization> = block.transactions.iter().map(|tx| TxVisualization {
            hash: format!("{:?}", tx.hash),
            from: format!("{:?}", tx.from),
            to: format!("{:?}", tx.to.unwrap_or_default()),
            value: format!("{}", tx.value),
            gas: tx.gas.unwrap_or_default().as_u64(),
            status: true,
            call_type: "call".to_string(),
            internal_calls: vec![],
        }).collect();
        Ok(BlockVisualization { number: block.number.unwrap_or_default().as_u64(), hash: format!("{:?}", block.hash.unwrap_or_default()), timestamp: block.timestamp.as_u64() as i64, transactions: txs, gas_used: block.gas_used.unwrap_or_default().as_u64(), gas_limit: block.gas_limit.unwrap_or_default().as_u64(), miner: format!("{:?}", block.author.unwrap_or_default()), size: block.size.unwrap_or_default().as_usize() })
    }
    pub async fn get_transaction_flow(&self, tx_hash: &str) -> Result<TransactionFlow, VisualizerError> {
        let tx = self.rpc.get_transaction(tx_hash.into()).await.map_err(|e| VisualizerError::Rpc(e.to_string()))?;
        let tx = tx.ok_or_else(|| VisualizerError::NotFound(tx_hash.to_string()))?;
        let call_type = if tx.to.is_none() { "create" } else { "call" }.to_string();
        let call = CallNode { id: 0, call_type: call_type.clone(), from: format!("{:?}", tx.from), to: format!("{:?}", tx.to.unwrap_or_default()), value: format!("{}", tx.value), depth: 0, children: vec![] };
        let flow_type = if tx.value > ethers::types::U256::zero() { FlowType::Transfer } else { FlowType::ContractCall };
        Ok(TransactionFlow { tx_hash: tx_hash.to_string(), calls: vec![call], flow_type })
    }
}