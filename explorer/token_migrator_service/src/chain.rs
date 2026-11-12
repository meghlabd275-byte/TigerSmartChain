//! Chain access: ERC-20 `Transfer` log retrieval over JSON-RPC.

use ethers::providers::{Http, Middleware, Provider};
use ethers::types::{Address, Filter, Log, ValueOrArray, H256, U256};

use crate::errors::{Error, Result};
use crate::types::TransferLog;

/// `keccak256("Transfer(address,address,uint256)")`.
pub const TRANSFER_TOPIC: &str =
    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

/// Reads ERC-20 transfers from an EVM node.
pub struct ChainClient {
    provider: Provider<Http>,
    chunk_size: u64,
}

impl ChainClient {
    pub fn new(rpc_url: &str, chunk_size: u64) -> Result<Self> {
        if rpc_url.trim().is_empty() {
            return Err(Error::Invalid("rpc_url must not be empty".to_string()));
        }

        let provider = Provider::<Http>::try_from(rpc_url)
            .map_err(|error| Error::Rpc(format!("cannot build provider for {rpc_url}: {error}")))?;

        Ok(Self {
            provider,
            chunk_size: chunk_size.max(1),
        })
    }

    /// Latest block number.
    pub async fn block_number(&self) -> Result<u64> {
        self.provider
            .get_block_number()
            .await
            .map(|number| number.as_u64())
            .map_err(|error| Error::Rpc(error.to_string()))
    }

    /// Every `Transfer` log for `token` in `[from, to]`.
    ///
    /// The range is walked in chunks because most nodes cap the number of blocks
    /// a single `eth_getLogs` may cover.
    pub async fn transfers(
        &self,
        token: Address,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<TransferLog>> {
        if to_block < from_block {
            return Err(Error::Invalid(format!(
                "to_block {to_block} is before from_block {from_block}"
            )));
        }

        let topic = H256::from_slice(
            &hex::decode(TRANSFER_TOPIC.trim_start_matches("0x"))
                .map_err(|error| Error::Invalid(format!("bad Transfer topic: {error}")))?,
        );

        let mut transfers = Vec::new();
        let mut start = from_block;

        while start <= to_block {
            let end = (start + self.chunk_size - 1).min(to_block);

            let filter = Filter::new()
                .address(ValueOrArray::Value(token))
                .topic0(ValueOrArray::Value(topic))
                .from_block(start)
                .to_block(end);

            let logs = self
                .provider
                .get_logs(&filter)
                .await
                .map_err(|error| Error::Rpc(error.to_string()))?;

            for log in logs {
                if let Some(transfer) = decode_transfer(&log)? {
                    transfers.push(transfer);
                }
            }

            start = end + 1;
        }

        transfers.sort_by(|a, b| {
            a.block_number
                .cmp(&b.block_number)
                .then(a.log_index.cmp(&b.log_index))
        });

        Ok(transfers)
    }

    /// Every `Transfer` log in `[from, to]` across all contracts.
    pub async fn all_transfers(&self, from_block: u64, to_block: u64) -> Result<Vec<TransferLog>> {
        if to_block < from_block {
            return Err(Error::Invalid(format!(
                "to_block {to_block} is before from_block {from_block}"
            )));
        }

        let topic = H256::from_slice(
            &hex::decode(TRANSFER_TOPIC.trim_start_matches("0x"))
                .map_err(|error| Error::Invalid(format!("bad Transfer topic: {error}")))?,
        );

        let mut transfers = Vec::new();
        let mut start = from_block;

        while start <= to_block {
            let end = (start + self.chunk_size - 1).min(to_block);

            let filter = Filter::new()
                .topic0(ValueOrArray::Value(topic))
                .from_block(start)
                .to_block(end);

            let logs = self
                .provider
                .get_logs(&filter)
                .await
                .map_err(|error| Error::Rpc(error.to_string()))?;

            for log in logs {
                if let Some(transfer) = decode_transfer(&log)? {
                    transfers.push(transfer);
                }
            }

            start = end + 1;
        }

        Ok(transfers)
    }
}

/// Decode one `Transfer` log. Returns `None` for a log that is not a
/// well-formed ERC-20 transfer, so a malformed event cannot abort the scan.
fn decode_transfer(log: &Log) -> Result<Option<TransferLog>> {
    if log.topics.len() < 3 {
        return Ok(None);
    }

    if log.data.len() != 32 {
        return Ok(None);
    }

    let from = Address::from_slice(&log.topics[1].as_bytes()[12..]);
    let to = Address::from_slice(&log.topics[2].as_bytes()[12..]);
    let value = U256::from_big_endian(log.data.as_ref());

    let log_index = log
        .log_index
        .ok_or_else(|| Error::Rpc("transfer log has no index".to_string()))?
        .as_u64();

    let block_number = log
        .block_number
        .ok_or_else(|| Error::Rpc("transfer log has no block number".to_string()))?
        .as_u64();

    Ok(Some(TransferLog {
        token: format!("{:#x}", log.address),
        from: format!("{from:#x}"),
        to: format!("{to:#x}"),
        value: value.to_string(),
        block_number,
        tx_hash: format!("{:#x}", log.transaction_hash.unwrap_or_default()),
        log_index,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::types::Log as EthLog;

    fn transfer_log(from: Address, to: Address, value: u64) -> EthLog {
        let mut data = [0u8; 32];
        U256::from(value).to_big_endian(&mut data);

        EthLog {
            address: Address::from_low_u64_be(0x1234),
            topics: vec![
                H256::from_slice(&hex::decode(TRANSFER_TOPIC.trim_start_matches("0x")).unwrap()),
                H256::from(from),
                H256::from(to),
            ],
            data: data.to_vec().into(),
            block_hash: None,
            block_number: Some(7u64.into()),
            transaction_hash: Some(H256::repeat_byte(0xab)),
            transaction_index: Some(1u64.into()),
            log_index: Some(3u64.into()),
            transaction_log_index: None,
            log_type: None,
            removed: Some(false),
        }
    }

    #[test]
    fn the_transfer_topic_is_the_canonical_one() {
        assert_eq!(
            TRANSFER_TOPIC,
            "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
        );
    }

    #[test]
    fn a_transfer_log_decodes_to_addresses_and_value() {
        let from = Address::from_low_u64_be(0xaaaa);
        let to = Address::from_low_u64_be(0xbbbb);

        let decoded = decode_transfer(&transfer_log(from, to, 1_000)).unwrap().unwrap();

        assert_eq!(decoded.from, format!("{from:#x}"));
        assert_eq!(decoded.to, format!("{to:#x}"));
        assert_eq!(decoded.value, "1000");
        assert_eq!(decoded.block_number, 7);
        assert_eq!(decoded.log_index, 3);
    }

    #[test]
    fn a_log_without_the_value_word_is_skipped() {
        let mut log = transfer_log(Address::zero(), Address::zero(), 1);
        log.data = Vec::new().into();

        assert!(decode_transfer(&log).unwrap().is_none());
    }

    #[test]
    fn a_log_with_too_few_topics_is_skipped() {
        let mut log = transfer_log(Address::zero(), Address::zero(), 1);
        log.topics.truncate(2);

        assert!(decode_transfer(&log).unwrap().is_none());
    }

    #[test]
    fn an_empty_rpc_url_is_rejected() {
        assert!(ChainClient::new("  ", 10).is_err());
    }

    #[test]
    fn a_reversed_block_range_is_rejected() {
        let client = ChainClient::new("http://127.0.0.1:8545", 10).unwrap();
        let token = Address::zero();

        let runtime = tokio::runtime::Runtime::new().unwrap();
        assert!(runtime.block_on(client.transfers(token, 10, 5)).is_err());
    }
}
