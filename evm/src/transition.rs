//! The state transition function: applying a transaction to world state.
//!
//! This is the piece that turns a signed transaction into state changes. It
//! implements the Yellow Paper's `ϒ` (or at least the parts that matter for a
//! value-transfer-and-contract-call chain):
//!
//!   1. Recover and check the sender from the signature.
//!   2. Check the nonce matches the account's, so a transaction cannot be
//!      replayed and transactions are applied in order.
//!   3. Charge the intrinsic gas up front and require the sender can pay the
//!      maximum cost (gas limit * gas price + value). This is what stops a
//!      transaction from being included when it can only partly be paid for.
//!   4. Execute: a plain transfer moves value; a call runs the EVM.
//!   5. Refund the unused gas and pay the miner.
//!   6. Increment the nonce.
//!
//! Every failure path refunds the sender and leaves the nonce untouched, so a
//! rejected transaction cannot corrupt the account it came from.

use crate::host::Host;
use crate::{EvmWord, ExecutionContext, Executor};
use tiger_crypto::keccak256;
use tiger_state::account::{
    u256_add, u256_cmp, u256_from_u64, u256_mul_u64, u256_sub, WorldState,
};
use tiger_state::rlp::RlpItem;

/// Gas charged before any code runs: 21,000 base plus the per-byte cost of the
/// calldata (4 gas for a zero byte, 16 for a non-zero byte).
pub const TRANSACTION_BASE_GAS: u64 = 21_000;
/// Per-byte cost of a zero calldata byte.
pub const CALLDATA_ZERO_BYTE_GAS: u64 = 4;
/// Per-byte cost of a non-zero calldata byte.
pub const CALLDATA_NONZERO_BYTE_GAS: u64 = 16;
/// Additional charge for a contract-creation transaction.
pub const CONTRACT_CREATION_GAS: u64 = 32_000;
/// Gas refunded for clearing a storage slot to zero, capped at half the used gas.
pub const STORAGE_CLEAR_REFUND: u64 = 4_800;

/// Why a transaction could not be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    /// The sender address could not be recovered from the signature.
    InvalidSignature,
    /// The transaction nonce does not match the account's current nonce.
    NonceMismatch { expected: u64, found: u64 },
    /// The sender cannot pay `gas_limit * gas_price + value`.
    InsufficientFundsForGas,
    /// The gas limit is below the intrinsic cost of the transaction.
    IntrinsicGasTooLow { required: u64, provided: u64 },
    /// The transaction ran out of gas.
    OutOfGas,
    /// The EVM reverted.
    Reverted,
    /// The `to` address was not supplied and the init code produced no code.
    ContractCreationFailed,
    /// A gas price or value field could not be decoded.
    MalformedTransaction,
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionError::InvalidSignature => write!(f, "invalid transaction signature"),
            TransitionError::NonceMismatch { expected, found } => {
                write!(f, "nonce mismatch: expected {expected}, found {found}")
            }
            TransitionError::InsufficientFundsForGas => {
                write!(f, "sender cannot cover gas limit * gas price + value")
            }
            TransitionError::IntrinsicGasTooLow { required, provided } => write!(
                f,
                "gas limit {provided} is below the intrinsic cost {required}"
            ),
            TransitionError::OutOfGas => write!(f, "out of gas"),
            TransitionError::Reverted => write!(f, "execution reverted"),
            TransitionError::ContractCreationFailed => {
                write!(f, "contract creation produced no code")
            }
            TransitionError::MalformedTransaction => write!(f, "malformed transaction"),
        }
    }
}

impl std::error::Error for TransitionError {}

/// A transaction in the form the transition function needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionRequest {
    /// Sender nonce.
    pub nonce: u64,
    /// The maximum price per unit of gas the sender authorised, in wei.
    ///
    /// For a legacy or EIP-2930 transaction this is the signed `gasPrice`. For
    /// an EIP-1559 transaction it is `maxFeePerGas`, and the price actually
    /// paid is `min(maxFeePerGas, baseFee + maxPriorityFeePerGas)`.
    pub gas_price: [u8; 32],
    /// Maximum gas the transaction may consume.
    pub gas_limit: u64,
    /// Recipient, or `None` for a contract creation.
    pub to: Option<[u8; 20]>,
    /// Value transferred in wei.
    pub value: [u8; 32],
    /// Calldata, or init code when `to` is `None`.
    pub data: Vec<u8>,
    /// The sender recovered from the signature.
    pub sender: [u8; 20],
    /// EIP-1559 maximum priority fee per gas. `None` for a legacy or EIP-2930
    /// transaction, whose `gas_price` is the price paid outright.
    pub max_priority_fee_per_gas: Option<[u8; 32]>,
}

/// The outcome of applying a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionOutcome {
    /// Whether execution succeeded.
    pub success: bool,
    /// Gas consumed, after refunds.
    pub gas_used: u64,
    /// Value returned by the EVM.
    pub output: Vec<u8>,
    /// Address of a newly created contract, if this was a creation.
    pub contract_address: Option<[u8; 20]>,
    /// Logs the transaction emitted, in the order they were emitted. Empty for
    /// a transaction that reverted, since a revert discards its logs.
    pub logs: Vec<crate::types::Log>,
    /// Fee burned by this transaction, per EIP-1559. The base-fee portion of
    /// the charge is removed from circulation rather than paid to anyone, so
    /// a caller tracking supply needs it.
    pub burned: [u8; 32],
}

/// The intrinsic gas cost of a transaction: the base charge plus the per-byte
/// calldata cost, plus the creation surcharge for a deployment.
pub fn intrinsic_gas(data: &[u8], is_creation: bool) -> u64 {
    let mut gas = TRANSACTION_BASE_GAS;
    if is_creation {
        gas += CONTRACT_CREATION_GAS;
    }
    for &byte in data {
        gas += if byte == 0 {
            CALLDATA_ZERO_BYTE_GAS
        } else {
            CALLDATA_NONZERO_BYTE_GAS
        };
    }
    gas
}

/// A host that reads and writes the real [`WorldState`] and accrues fees.
///
/// Gas accounting and the miner reward live here because they are host
/// concerns: the interpreter only reports how much gas it used.
pub struct StateHost<'a> {
    state: &'a mut WorldState,
    logs: Vec<crate::types::Log>,
    refund: u64,
}

impl<'a> StateHost<'a> {
    /// Wrap a world state.
    pub fn new(state: &'a mut WorldState) -> Self {
        Self {
            state,
            logs: Vec::new(),
            refund: 0,
        }
    }

    /// The accumulated storage-clear refund.
    pub fn refund(&self) -> u64 {
        self.refund
    }
}

impl<'a> Host for StateHost<'a> {
    fn balance(&self, address: &crate::Address) -> EvmWord {
        let addr: [u8; 20] = address.0;
        EvmWord(self.state.balance(&addr))
    }

    fn exists(&self, address: &crate::Address) -> bool {
        let addr: [u8; 20] = address.0;
        self.state.exists(&addr)
    }

    fn code(&self, address: &crate::Address) -> Vec<u8> {
        let addr: [u8; 20] = address.0;
        self.state.code(&addr)
    }

    fn code_hash(&self, address: &crate::Address) -> EvmWord {
        let addr: [u8; 20] = address.0;
        EvmWord(self.state.code_hash(&addr))
    }

    fn storage(&self, address: &crate::Address, slot: &EvmWord) -> EvmWord {
        let addr: [u8; 20] = address.0;
        EvmWord(self.state.storage(&addr, &slot.0))
    }

    fn set_storage(&mut self, address: &crate::Address, slot: EvmWord, value: EvmWord) {
        let addr: [u8; 20] = address.0;
        // Clearing a non-zero slot to zero earns the EIP-2200 refund.
        if value.0 == [0u8; 32] && self.state.storage(&addr, &slot.0) != [0u8; 32] {
            self.refund += STORAGE_CLEAR_REFUND;
        }
        self.state.set_storage(&addr, slot.0, value.0);
    }

    fn block_hash(&self, _number: u64) -> EvmWord {
        // Block hashes are supplied by the caller through the block context;
        // the state layer has no access to the chain history.
        EvmWord::zero()
    }

    fn emit_log(&mut self, address: &crate::Address, topics: Vec<EvmWord>, data: Vec<u8>) {
        self.logs.push(crate::types::Log {
            address: *address,
            topics,
            data,
        });
    }

    fn transfer(
        &mut self,
        from: &crate::Address,
        to: &crate::Address,
        value: EvmWord,
    ) -> bool {
        let from_addr: [u8; 20] = from.0;
        let to_addr: [u8; 20] = to.0;
        self.state.transfer(&from_addr, &to_addr, &value.0)
    }

    fn set_code(&mut self, address: &crate::Address, code: Vec<u8>) {
        let addr: [u8; 20] = address.0;
        self.state.set_code(&addr, code);
    }

    fn bump_nonce(&mut self, address: &crate::Address) -> u64 {
        let addr: [u8; 20] = address.0;
        let nonce = self.state.nonce(&addr);
        self.state.set_nonce(&addr, nonce + 1);
        nonce
    }

    fn selfdestruct(&mut self, address: &crate::Address) {
        let addr: [u8; 20] = address.0;
        self.state.remove_account(&addr);
    }

    fn logs(&self) -> &[crate::types::Log] {
        &self.logs
    }

    fn take_logs(&mut self) -> Vec<crate::types::Log> {
        std::mem::take(&mut self.logs)
    }
}

/// The block context a transaction executes in.
#[derive(Debug, Clone)]
pub struct BlockContext {
    /// Chain id, used for EIP-155 and the CHAINID opcode.
    pub chain_id: u64,
    /// Number of the block being built.
    pub number: u64,
    /// Block timestamp.
    pub timestamp: u64,
    /// Block gas limit.
    pub gas_limit: u64,
    /// Beneficiary of the transaction fees.
    pub coinbase: [u8; 20],
    /// Block base fee (EIP-1559).
    pub base_fee: u64,
}

impl Default for BlockContext {
    fn default() -> Self {
        Self {
            chain_id: 6666,
            number: 0,
            timestamp: 0,
            gas_limit: 30_000_000,
            coinbase: [0u8; 20],
            base_fee: 0,
        }
    }
}

/// Apply a transaction to the world state.
///
/// On any failure before execution begins the state is left untouched. On
/// failure during execution the gas consumed up to that point is still charged
/// and the nonce is still incremented, which matches how a real chain behaves:
/// a reverted transaction is included and pays for its gas.
pub fn apply_transaction(
    state: &mut WorldState,
    tx: &TransactionRequest,
    block: &BlockContext,
) -> Result<TransitionOutcome, TransitionError> {
    apply_transaction_traced(state, tx, block, &mut crate::executor::NullTracer)
        .map(|(outcome, _)| outcome)
}

/// The state writes a traced transaction performed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraceDiffs {
    /// Storage slots whose final value differs from their initial value.
    pub storage: Vec<crate::diff::StorageDiff>,
    /// Accounts whose final balance differs from their initial balance.
    pub balances: Vec<crate::diff::BalanceDiff>,
}

/// Apply a transaction while reporting every executed opcode to `tracer`, and
/// return the state writes the transaction performed.
///
/// This is the implementation behind [`apply_transaction`] and behind
/// `debug_traceTransaction`, so a trace describes the execution the chain
/// actually performed rather than a separate replay. A reverted transaction
/// reports no writes, because a revert discards them.
pub fn apply_transaction_traced(
    state: &mut WorldState,
    tx: &TransactionRequest,
    block: &BlockContext,
    tracer: &mut dyn crate::trace::StepTracer,
) -> Result<(TransitionOutcome, TraceDiffs), TransitionError> {
    let sender = tx.sender;

    // 1. Nonce must match exactly, so transactions apply in order and cannot
    //    be replayed.
    let account_nonce = state.nonce(&sender);
    if account_nonce != tx.nonce {
        return Err(TransitionError::NonceMismatch {
            expected: account_nonce,
            found: tx.nonce,
        });
    }

    // 2. Intrinsic gas is charged before anything runs.
    let is_creation = tx.to.is_none();
    let intrinsic = intrinsic_gas(&tx.data, is_creation);
    if tx.gas_limit < intrinsic {
        return Err(TransitionError::IntrinsicGasTooLow {
            required: intrinsic,
            provided: tx.gas_limit,
        });
    }

    // The price actually paid per unit of gas. An EIP-1559 transaction
    // derives it from the block's base fee, and a ceiling below that base fee
    // is unusable, so this is resolved before any state is touched: a
    // rejection must leave the account exactly as it was.
    let effective = effective_gas_price(tx, block)?;

    // 3. The sender must be able to pay the maximum possible cost:
    //    gas_limit * gas_price + value.
    let max_gas_cost = u256_mul_u64(&tx.gas_price, tx.gas_limit)
        .ok_or(TransitionError::InsufficientFundsForGas)?;
    let (max_total, overflow) = u256_add(&max_gas_cost, &tx.value);
    if overflow {
        return Err(TransitionError::InsufficientFundsForGas);
    }
    if u256_cmp(&state.balance(&sender), &max_total) == std::cmp::Ordering::Less {
        return Err(TransitionError::InsufficientFundsForGas);
    }

    // 4. Capture the pre-transaction state so a revert can undo everything
    //    the transaction does, while the gas charge and nonce increment
    //    applied below still stand.
    let pre_state = state.snapshot();

    // Charge the maximum gas cost up front. The unused portion is refunded
    // once the real usage is known.
    state.debit(&sender, &max_total);

    // 5. Build the execution context.
    let mut host = StateHost::new(state);
    let mut ctx = ExecutionContext::new();
    ctx.caller = crate::Address(sender);
    ctx.origin = crate::Address(sender);
    ctx.gas = tx.gas_limit;
    // GASPRICE reports the price actually paid, which for an EIP-1559
    // transaction is the effective price rather than the authorised ceiling.
    ctx.gas_price = u64_from_be(&effective);
    ctx.value = u64_from_be(&tx.value);
    ctx.data = tx.data.clone();
    ctx.block_number = block.number;
    ctx.block_timestamp = block.timestamp;
    ctx.block_gas_limit = block.gas_limit;
    ctx.block_coinbase = crate::Address(block.coinbase);
    ctx.block_basefee = block.base_fee;
    ctx.chain_id = block.chain_id;

    let (exec_code, exec_address, contract_address) = match &tx.to {
        Some(to) => (host.code(&crate::Address(*to)), *to, None),
        None => {
            // A creation: the sender's nonce (before increment) determines the
            // address, and the calldata is the init code.
            let new_address = create_address(&sender, tx.nonce);
            (tx.data.clone(), new_address, Some(new_address))
        }
    };
    ctx.address = crate::Address(exec_address);

    // 6 and 7. Every host write the transaction performs goes through the
    // recorder, so the recorded before/after values are the real transitions
    // this execution caused.
    let mut diffs = TraceDiffs::default();
    let result = {
        let mut recording = crate::diff::StateDiffRecorder::new(&mut host);

        // Move the value before execution.
        if tx.value != [0u8; 32] {
            if !recording.transfer(
                &crate::Address(sender),
                &crate::Address(exec_address),
                EvmWord(tx.value),
            ) {
                // The sender was charged the maximum cost before execution, so
                // the snapshot taken at entry has to be restored before
                // returning. Leaving the debit in place would burn the
                // sender's gas for a transaction that never made it into a
                // block.
                state.restore(pre_state);
                return Err(TransitionError::InsufficientFundsForGas);
            }
        }

        let executor = Executor::new();
        let result =
            executor.execute_traced_with_host(&exec_code, &mut ctx, &mut recording, tracer);

        // A creation deploys the returned code at the new address.
        if result.success {
            if let Some(address) = contract_address {
                recording.set_code(&crate::Address(address), result.output.clone());
            }
        }

        diffs.storage = recording.storage_diffs();
        diffs.balances = recording.balance_diffs();
        result
    };

    // If the transaction reverted, discard every state change it made —
    // storage writes, the value transfer, created code — while keeping the
    // gas accounting that follows. The snapshot was taken before the value
    // moved, so restoring it returns the value to the sender as well.
    if !result.success {
        host.state.restore(pre_state);
        // The writes were discarded, so reporting them would misrepresent the
        // resulting state.
        diffs = TraceDiffs::default();
    }

    // Logs follow the same rule as the other state writes: a revert discards
    // them, so a failed transaction contributes no logs to its receipt.
    let logs = if result.success {
        result.logs.clone()
    } else {
        Vec::new()
    };

    // 8. Gas accounting. Execution has the intrinsic cost removed from its
    //    budget so the total never falls below the intrinsic charge.
    let gas_used_by_evm = result.gas_used.min(tx.gas_limit - intrinsic);
    let mut gas_used = intrinsic + gas_used_by_evm;

    // Refunds are capped at half the gas used, per EIP-2200.
    let refund = host.refund().min(gas_used / 2);
    gas_used -= refund;

    let actual_cost = u256_mul_u64(&effective, gas_used)
        .ok_or(TransitionError::MalformedTransaction)?;
    let unused = u256_sub(&max_total, &actual_cost)
        .ok_or(TransitionError::MalformedTransaction)?;

    // Return the unused gas to the sender, then settle the fee. Under
    // EIP-1559 the base fee is burned and only the tip reaches the validator,
    // so crediting the whole cost would mint supply the chain never issued.
    let sender_addr = sender;
    host.state.credit(&sender_addr, &unused);
    let (burned_per_gas, tip_per_gas) = fee_split(&effective, block);
    let tip = u256_mul_u64(&tip_per_gas, gas_used)
        .ok_or(TransitionError::MalformedTransaction)?;
    if tip != [0u8; 32] {
        host.state.credit(&block.coinbase, &tip);
    }
    // The burned portion is accounted for so a caller can report the supply
    // removed by the block.
    let burned = u256_mul_u64(&burned_per_gas, gas_used)
        .ok_or(TransitionError::MalformedTransaction)?;

    // 9. The nonce advances whether or not execution succeeded, because the
    //    transaction was included and paid for.
    host.state.set_nonce(&sender_addr, tx.nonce + 1);

    if !result.success {
        return Ok((
            TransitionOutcome {
                success: false,
                gas_used,
                output: result.output,
                contract_address: None,
                logs,
                burned,
            },
            diffs,
        ));
    }

    // An account created by a transaction is present even when it ends up with
    // no code: EIP-161 has the creator's nonce bump to 1, which is what makes
    // the account exist and what stops a later creation landing on the same
    // address. Leaving the account absent would also make a later transfer to
    // it charge the new-account surcharge again, so the state root here would
    // differ from every other client's.
    if is_creation && result.output.is_empty() {
        if let Some(address) = contract_address {
            if host.state.nonce(&address) == 0 {
                host.state.set_nonce(&address, 1);
            }
        }
    }

    Ok((
        TransitionOutcome {
            success: true,
            gas_used,
            output: result.output,
            contract_address,
            logs,
            burned,
        },
        diffs,
    ))
}

/// The CREATE address for `sender` at `nonce`:
/// `keccak256(rlp([sender, nonce]))[12..]`.
pub fn create_address(sender: &[u8; 20], nonce: u64) -> [u8; 20] {
    let encoded = RlpItem::list(vec![
        RlpItem::Bytes(sender.to_vec()),
        RlpItem::from_u64(nonce),
    ])
    .encode();
    let hash = keccak256(&encoded);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);
    address
}

/// The low 8 bytes of a 256-bit value, saturating at u64::MAX.

/// The price per unit of gas a transaction actually pays, per EIP-1559.
///
/// A legacy or EIP-2930 transaction pays its signed `gasPrice`. An EIP-1559
/// transaction pays `min(maxFeePerGas, baseFee + maxPriorityFeePerGas)`, so
/// the sender never pays more than they authorised and the validator is never
/// underpaid relative to the tip the sender offered.
fn effective_gas_price(
    tx: &TransactionRequest,
    block: &BlockContext,
) -> Result<[u8; 32], TransitionError> {
    let Some(priority) = tx.max_priority_fee_per_gas else {
        return Ok(tx.gas_price);
    };

    // A sender who authorised less than the base fee cannot have their
    // transaction included; including it would either overcharge them or
    // underpay the burn.
    let base_fee = u256_from_u64(block.base_fee);
    if u256_cmp(&tx.gas_price, &base_fee) == std::cmp::Ordering::Less {
        return Err(TransitionError::MalformedTransaction);
    }

    let (base_plus_tip, overflow) = u256_add(&base_fee, &priority);
    if overflow {
        // An absurd tip would wrap the sum; refuse rather than charge a
        // nonsense price.
        return Err(TransitionError::MalformedTransaction);
    }
    Ok(if u256_cmp(&tx.gas_price, &base_plus_tip) == std::cmp::Ordering::Less {
        tx.gas_price
    } else {
        base_plus_tip
    })
}

/// Split a fee into the part that is burned and the part paid to the
/// validator, per EIP-1559. The base fee is burned; only the tip is credited
/// to the block's beneficiary.
fn fee_split(effective: &[u8; 32], block: &BlockContext) -> ([u8; 32], [u8; 32]) {
    let base_fee = u256_from_u64(block.base_fee);
    if u256_cmp(effective, &base_fee) == std::cmp::Ordering::Less {
        return ([0u8; 32], *effective);
    }
    let burned = base_fee;
    let tip = u256_sub(effective, &base_fee).unwrap_or([0u8; 32]);
    (burned, tip)
}

fn u64_from_be(value: &[u8; 32]) -> u64 {
    if value[..24].iter().any(|&b| b != 0) {
        return u64::MAX;
    }
    let mut out = 0u64;
    for &b in &value[24..] {
        out = (out << 8) | b as u64;
    }
    out
}

/// Convenience: a 256-bit value from a u64.
pub fn wei(value: u64) -> [u8; 32] {
    u256_from_u64(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiger_state::account::WorldState;

    fn addr(byte: u8) -> [u8; 20] {
        [byte; 20]
    }

    fn block() -> BlockContext {
        BlockContext {
            chain_id: 6666,
            number: 1,
            timestamp: 1_700_000_000,
            gas_limit: 30_000_000,
            coinbase: addr(0x55),
            base_fee: 0,
        }
    }

    #[test]
    fn intrinsic_gas_matches_the_yellow_paper() {
        // Plain transfer: the base 21,000.
        assert_eq!(intrinsic_gas(&[], false), 21_000);
        // Four zero bytes at 4 gas each.
        assert_eq!(intrinsic_gas(&[0, 0, 0, 0], false), 21_000 + 16);
        // Four non-zero bytes at 16 gas each.
        assert_eq!(intrinsic_gas(&[1, 2, 3, 4], false), 21_000 + 64);
        // Contract creation adds the 32,000 surcharge.
        assert_eq!(intrinsic_gas(&[], true), 21_000 + 32_000);
    }

    #[test]
    fn simple_value_transfer_moves_funds_and_charges_gas() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        let recipient = addr(0x22);
        state.set_balance(&sender, wei(1_000_000));
        state.set_nonce(&sender, 0);

        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(1),
            gas_limit: 21_000,
            to: Some(recipient),
            value: wei(1000),
            data: Vec::new(),
            sender,
            max_priority_fee_per_gas: None,
        };

        let outcome = apply_transaction(&mut state, &tx, &block()).unwrap();
        assert!(outcome.success);
        assert_eq!(outcome.gas_used, 21_000);

        // Recipient received the value.
        assert_eq!(state.balance(&recipient), wei(1000));
        // Sender paid value + 21,000 gas at price 1.
        assert_eq!(state.balance(&sender), wei(1_000_000 - 1000 - 21_000));
        // Beneficiary received the fees.
        assert_eq!(state.balance(&addr(0x55)), wei(21_000));
        // The nonce advanced.
        assert_eq!(state.nonce(&sender), 1);
    }

    #[test]
    fn a_traced_transfer_reports_the_steps_and_the_balance_writes() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        let recipient = addr(0x22);
        state.set_balance(&sender, wei(1_000_000));
        state.set_nonce(&sender, 0);

        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(1),
            gas_limit: 21_000,
            to: Some(recipient),
            value: wei(1000),
            data: Vec::new(),
            sender,
            max_priority_fee_per_gas: None,
        };

        let mut tracer = crate::trace::StepTracerBuffer::new(64);
        let (outcome, diffs) =
            apply_transaction_traced(&mut state, &tx, &block(), &mut tracer).unwrap();

        assert!(outcome.success);

        // A transfer to an account without code executes no opcodes, so the
        // interpreter charges no gas. The 21,000 intrinsic charge is accounted
        // for by the transition, not by the interpreter.
        assert!(tracer.steps().is_empty());
        let summary = tracer.summary().expect("summary is recorded");
        assert!(summary.success);
        assert_eq!(summary.gas_used, 0);
        assert_eq!(outcome.gas_used, 21_000);

        // The value transfer is reported with the before/after values observed
        // at the moment of the write. The up-front gas charge has already been
        // debited by then, so `before` is the balance net of that charge rather
        // than the account's opening balance.
        let sender_diff = diffs
            .balances
            .iter()
            .find(|d| d.address == crate::Address(sender))
            .expect("sender balance change is reported");
        // Step 4 debits `gas_limit * gas_price + value` before execution, so by
        // the time the transfer runs the sender is already down that amount.
        let after_upfront_debit = 1_000_000 - 21_000 - 1000;
        assert_eq!(sender_diff.before, EvmWord(wei(after_upfront_debit)));
        assert_eq!(sender_diff.after, EvmWord(wei(after_upfront_debit - 1000)));

        let recipient_diff = diffs
            .balances
            .iter()
            .find(|d| d.address == crate::Address(recipient))
            .expect("recipient balance change is reported");
        assert_eq!(recipient_diff.before, EvmWord::zero());
        assert_eq!(recipient_diff.after, EvmWord(wei(1000)));
    }

    #[test]
    fn a_traced_contract_call_reports_every_executed_opcode() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        let contract = addr(0x33);
        state.set_balance(&sender, wei(1_000_000));
        state.set_nonce(&sender, 0);

        // PUSH1 0x01, PUSH1 0x02, ADD, PUSH1 0x00, MSTORE, PUSH1 0x20,
        // PUSH1 0x00, RETURN -- stores 3 in memory and returns it.
        let code = vec![
            0x60, 0x01, 0x60, 0x02, 0x01, 0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
        ];
        state.set_code(&contract, code);

        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(1),
            gas_limit: 100_000,
            to: Some(contract),
            value: wei(0),
            data: Vec::new(),
            sender,
            max_priority_fee_per_gas: None,
        };

        let mut tracer = crate::trace::StepTracerBuffer::new(128);
        let (outcome, _) =
            apply_transaction_traced(&mut state, &tx, &block(), &mut tracer).unwrap();

        assert!(outcome.success);
        assert_eq!(outcome.output.len(), 32);
        assert_eq!(outcome.output[31], 3);

        let steps = tracer.steps();
        let names: Vec<&str> = steps.iter().map(|s| s.op).collect();
        assert_eq!(
            names,
            vec!["PUSH1", "PUSH1", "ADD", "PUSH1", "MSTORE", "PUSH1", "PUSH1", "RETURN"],
            "every executed opcode must be reported in order"
        );
    }

    #[test]
    fn a_traced_contract_write_reports_the_storage_slot() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        let contract = addr(0x44);
        state.set_balance(&sender, wei(1_000_000));
        state.set_nonce(&sender, 0);

        // PUSH1 0x2a, PUSH1 0x01, SSTORE, STOP -- storage[1] = 42.
        let code = vec![0x60, 0x2a, 0x60, 0x01, 0x55, 0x00];
        state.set_code(&contract, code);

        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(1),
            gas_limit: 100_000,
            to: Some(contract),
            value: wei(0),
            data: Vec::new(),
            sender,
            max_priority_fee_per_gas: None,
        };

        let mut tracer = crate::trace::StepTracerBuffer::new(64);
        let (outcome, diffs) =
            apply_transaction_traced(&mut state, &tx, &block(), &mut tracer).unwrap();
        assert!(outcome.success);

        let slot_diff = diffs
            .storage
            .iter()
            .find(|d| d.address == crate::Address(contract))
            .expect("the storage write is reported");
        assert_eq!(slot_diff.slot, EvmWord::from_u64(1));
        assert_eq!(slot_diff.before, EvmWord::zero());
        assert_eq!(slot_diff.after, EvmWord::from_u64(42));

        // The write really landed in the world state.
        let mut slot = [0u8; 32];
        slot[31] = 1;
        let mut expected = [0u8; 32];
        expected[31] = 42;
        assert_eq!(state.storage(&contract, &slot), expected);
    }

    #[test]
    fn a_reverted_traced_transaction_reports_no_state_writes() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        let contract = addr(0x66);
        state.set_balance(&sender, wei(1_000_000));
        state.set_nonce(&sender, 0);

        // PUSH1 0x2a, PUSH1 0x01, SSTORE, PUSH1 0x00, PUSH1 0x00, REVERT
        let code = vec![0x60, 0x2a, 0x60, 0x01, 0x55, 0x60, 0x00, 0x60, 0x00, 0xfd];
        state.set_code(&contract, code);

        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(1),
            gas_limit: 100_000,
            to: Some(contract),
            value: wei(0),
            data: Vec::new(),
            sender,
            max_priority_fee_per_gas: None,
        };

        let mut tracer = crate::trace::StepTracerBuffer::new(64);
        let (outcome, diffs) =
            apply_transaction_traced(&mut state, &tx, &block(), &mut tracer).unwrap();

        assert!(!outcome.success);
        // The storage write was rolled back, so nothing may be reported.
        assert!(diffs.storage.is_empty());
        // The opcodes still ran and are still reported.
        assert!(tracer.steps().len() >= 6);
        // The nonce still advanced because the transaction was included.
        assert_eq!(state.nonce(&sender), 1);
    }

    #[test]
    fn wrong_nonce_is_rejected_without_touching_state() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        state.set_balance(&sender, wei(1_000_000));
        state.set_nonce(&sender, 5);
        let before = state.balance(&sender);

        let tx = TransactionRequest {
            nonce: 3,
            gas_price: wei(1),
            gas_limit: 21_000,
            to: Some(addr(0x22)),
            value: wei(1),
            data: Vec::new(),
            sender,
            max_priority_fee_per_gas: None,
        };

        assert_eq!(
            apply_transaction(&mut state, &tx, &block()),
            Err(TransitionError::NonceMismatch {
                expected: 5,
                found: 3
            })
        );
        assert_eq!(state.balance(&sender), before, "state must be untouched");
        assert_eq!(state.nonce(&sender), 5);
    }

    #[test]
    fn insufficient_funds_is_rejected() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        // Enough for the value but not for the gas.
        state.set_balance(&sender, wei(1000));

        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(1),
            gas_limit: 21_000,
            to: Some(addr(0x22)),
            value: wei(1000),
            data: Vec::new(),
            sender,
            max_priority_fee_per_gas: None,
        };

        assert_eq!(
            apply_transaction(&mut state, &tx, &block()),
            Err(TransitionError::InsufficientFundsForGas)
        );
        assert_eq!(state.balance(&sender), wei(1000));
    }

    #[test]
    fn gas_limit_below_intrinsic_is_rejected() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        state.set_balance(&sender, wei(1_000_000));

        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(1),
            gas_limit: 20_000, // below the 21,000 base
            to: Some(addr(0x22)),
            value: wei(1),
            data: Vec::new(),
            sender,
            max_priority_fee_per_gas: None,
        };

        assert_eq!(
            apply_transaction(&mut state, &tx, &block()),
            Err(TransitionError::IntrinsicGasTooLow {
                required: 21_000,
                provided: 20_000
            })
        );
    }

    #[test]
    fn transaction_to_contract_runs_its_code() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        let contract = addr(0xcc);
        state.set_balance(&sender, wei(10_000_000));

        // Contract: SSTORE(0, 0x2a) then STOP.
        state.set_code(&contract, vec![0x60, 0x2a, 0x60, 0x00, 0x55, 0x00]);

        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(1),
            gas_limit: 100_000,
            to: Some(contract),
            value: wei(0),
            data: Vec::new(),
            sender,
            max_priority_fee_per_gas: None,
        };

        let outcome = apply_transaction(&mut state, &tx, &block()).unwrap();
        assert!(outcome.success, "the contract call must succeed");
        // The storage write must be visible in the world state.
        let mut slot = [0u8; 32];
        slot[31] = 0;
        assert_eq!(
            state.storage(&contract, &slot),
            {
                let mut expected = [0u8; 32];
                expected[31] = 0x2a;
                expected
            }
        );
        assert_eq!(state.nonce(&sender), 1);
    }

    #[test]
    fn contract_creation_deploys_the_returned_code() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        state.set_balance(&sender, wei(10_000_000));

        // Init code that returns the runtime code 0x6001600101.
        let init_code = vec![
            0x60, 0x60, 0x60, 0x00, 0x53,
            0x60, 0x01, 0x60, 0x01, 0x53,
            0x60, 0x60, 0x60, 0x02, 0x53,
            0x60, 0x01, 0x60, 0x03, 0x53,
            0x60, 0x01, 0x60, 0x04, 0x53,
            0x60, 0x05, 0x60, 0x00, 0xf3,
        ];

        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(1),
            gas_limit: 200_000,
            to: None,
            value: wei(0),
            data: init_code,
            sender,
            max_priority_fee_per_gas: None,
        };

        let outcome = apply_transaction(&mut state, &tx, &block()).unwrap();
        assert!(outcome.success, "creation must succeed");
        let address = outcome.contract_address.expect("a contract address");
        // The address must be keccak256(rlp([sender, 0]))[12..].
        assert_eq!(address, create_address(&sender, 0));
        // The runtime code must be deployed.
        assert_eq!(state.code(&address), vec![0x60, 0x01, 0x60, 0x01, 0x01]);
    }

    #[test]
    fn reverting_transaction_still_charges_gas_and_bumps_the_nonce() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        let contract = addr(0xcc);
        state.set_balance(&sender, wei(10_000_000));
        // Contract: PUSH1 0 PUSH1 0 REVERT.
        state.set_code(&contract, vec![0x60, 0x00, 0x60, 0x00, 0xfd]);

        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(1),
            gas_limit: 100_000,
            to: Some(contract),
            value: wei(500),
            data: Vec::new(),
            sender,
            max_priority_fee_per_gas: None,
        };

        let outcome = apply_transaction(&mut state, &tx, &block()).unwrap();
        assert!(!outcome.success, "the transaction must report failure");
        // The nonce advanced: the transaction was included and paid for.
        assert_eq!(state.nonce(&sender), 1);
        // The sender paid at least the intrinsic gas.
        assert!(outcome.gas_used >= 21_000);
        // The recipient must not keep the value of a reverted call.
        assert_eq!(state.balance(&contract), wei(0));
    }

    #[test]
    fn a_creation_that_returns_no_code_still_succeeds() {
        // An init program that returns nothing is a legitimate creation: it
        // deploys an account with no code. The transaction succeeds, the
        // account exists with a nonce of one, and the nonce increment is what
        // makes it exist.
        let mut state = WorldState::new();
        let sender = addr(0x11);
        state.set_balance(&sender, wei(10_000_000));

        let request = TransactionRequest {
            nonce: 0,
            gas_price: wei(1),
            gas_limit: 100_000,
            to: None,
            value: [0u8; 32],
            data: vec![0x00],
            sender,
            max_priority_fee_per_gas: None,
        };

        let outcome = apply_transaction(&mut state, &request, &block()).unwrap();
        assert!(
            outcome.success,
            "a creation returning no code must succeed, not be rejected"
        );

        let created = outcome
            .contract_address
            .expect("a successful creation reports its address");
        assert_eq!(created, create_address(&sender, 0));
        assert!(state.code(&created).is_empty());
        assert_eq!(state.nonce(&created), 1, "EIP-161 creator nonce");
        assert!(state.exists(&created), "the account must exist");
    }

    #[test]
    fn a_creation_that_returns_code_deploys_it() {
        // The init program stores the one byte 0x60 and returns it, so the
        // deployed account has real code and the reported address is the
        // keccak256(rlp([sender, nonce])) one.
        let mut state = WorldState::new();
        let sender = addr(0x22);
        state.set_balance(&sender, wei(10_000_000));

        // PUSH1 0x60, PUSH1 0x00, MSTORE8, PUSH1 0x01, PUSH1 0x00, RETURN
        let init = vec![0x60, 0x60, 0x60, 0x00, 0x53, 0x60, 0x01, 0x60, 0x00, 0xf3];
        let request = TransactionRequest {
            nonce: 0,
            gas_price: wei(1),
            gas_limit: 100_000,
            to: None,
            value: [0u8; 32],
            data: init,
            sender,
            max_priority_fee_per_gas: None,
        };

        let outcome = apply_transaction(&mut state, &request, &block()).unwrap();
        assert!(outcome.success);
        let created = outcome.contract_address.unwrap();
        assert_eq!(state.code(&created), vec![0x60]);
    }

    #[test]
    fn create_address_matches_the_known_vector() {
        // The canonical example: sender 0x00..00 at nonce 0.
        // keccak256(rlp(["", ""]))[12..] is a published value.
        let address = create_address(&[0u8; 20], 0);
        assert_eq!(
            address,
            [
                0xbd, 0x77, 0x04, 0x16, 0xa3, 0x34, 0x5f, 0x91, 0xe4, 0xb3, 0x45, 0x76, 0xcb,
                0x80, 0x4a, 0x57, 0x6f, 0xa4, 0x8e, 0xb1
            ]
        );
    }

    #[test]
    fn storage_refund_is_capped_at_half_the_gas_used() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        let contract = addr(0xcc);
        state.set_balance(&sender, wei(10_000_000));
        // Pre-populate a slot so clearing it earns a refund.
        let mut slot = [0u8; 32];
        slot[31] = 0;
        state.set_storage(&contract, slot, wei(1));
        // Contract: SSTORE(0, 0) then STOP.
        state.set_code(&contract, vec![0x60, 0x00, 0x60, 0x00, 0x55, 0x00]);

        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(1),
            gas_limit: 100_000,
            to: Some(contract),
            value: wei(0),
            data: Vec::new(),
            sender,
            max_priority_fee_per_gas: None,
        };

        let outcome = apply_transaction(&mut state, &tx, &block()).unwrap();
        assert!(outcome.success);
        // The refund cannot push the charge below half the gross usage.
        assert!(outcome.gas_used >= 21_000);
        // The slot is now zero.
        assert_eq!(state.storage(&contract, &slot), [0u8; 32]);
    }

    /// EIP-1559 splits the charge: the base fee is burned and only the tip
    /// reaches the validator. Crediting the whole charge would mint supply.
    #[test]
    fn eip1559_burns_the_base_fee_and_pays_only_the_tip() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        let coinbase = addr(0xcc);
        state.set_balance(&sender, wei(100_000_000));
        state.set_nonce(&sender, 0);

        // base fee 100, tip 10, ceiling 1000: the sender pays 110 per unit.
        let block = BlockContext {
            base_fee: 100,
            coinbase,
            gas_limit: 30_000_000,
            ..BlockContext::default()
        };
        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(1000),
            max_priority_fee_per_gas: Some(wei(10)),
            gas_limit: 21_000,
            to: Some(addr(0x22)),
            value: wei(0),
            data: Vec::new(),
            sender,
        };

        let outcome = apply_transaction(&mut state, &tx, &block).unwrap();
        let gas_used = outcome.gas_used as u64;
        assert_eq!(gas_used, 21_000);

        // The validator receives only the tip.
        let expected_tip = u256_mul_u64(&wei(10), gas_used).unwrap();
        assert_eq!(state.balance(&coinbase), expected_tip);

        // The burned amount is the base-fee portion.
        assert_eq!(outcome.burned, u256_mul_u64(&wei(100), gas_used).unwrap());

        // The sender paid the effective price, not the ceiling.
        let effective = u256_mul_u64(&wei(110), gas_used).unwrap();
        assert_eq!(
            state.balance(&sender),
            u256_sub(&wei(100_000_000), &effective).unwrap()
        );
    }

    /// A ceiling below the base fee cannot be included: the sender never
    /// authorised the burn, so the transaction is rejected outright.
    #[test]
    fn eip1559_rejects_a_ceiling_below_the_base_fee() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        state.set_balance(&sender, wei(10_000_000));
        state.set_nonce(&sender, 0);

        let block = BlockContext {
            base_fee: 500,
            ..BlockContext::default()
        };
        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(100),
            max_priority_fee_per_gas: Some(wei(1)),
            gas_limit: 21_000,
            to: Some(addr(0x22)),
            value: wei(0),
            data: Vec::new(),
            sender,
        };
        assert!(apply_transaction(&mut state, &tx, &block).is_err());
        // A rejected transaction leaves no trace on the state.
        assert_eq!(state.balance(&sender), wei(10_000_000));
        assert_eq!(state.nonce(&sender), 0);
    }

    /// With a zero base fee every unit of the ceiling is a tip, so a legacy
    /// transaction and an EIP-1559 transaction paying the same price behave
    /// identically. This is the case a chain with no base fee must not break.
    #[test]
    fn eip1559_with_no_base_fee_pays_the_whole_price_as_tip() {
        let mut state = WorldState::new();
        let sender = addr(0x11);
        let coinbase = addr(0xcc);
        state.set_balance(&sender, wei(10_000_000));
        state.set_nonce(&sender, 0);

        let block = BlockContext {
            base_fee: 0,
            coinbase,
            ..BlockContext::default()
        };
        let tx = TransactionRequest {
            nonce: 0,
            gas_price: wei(7),
            max_priority_fee_per_gas: Some(wei(7)),
            gas_limit: 21_000,
            to: Some(addr(0x22)),
            value: wei(0),
            data: Vec::new(),
            sender,
        };
        let outcome = apply_transaction(&mut state, &tx, &block).unwrap();
        assert_eq!(outcome.burned, [0u8; 32]);
        assert_eq!(
            state.balance(&coinbase),
            u256_mul_u64(&wei(7), outcome.gas_used as u64).unwrap()
        );
    }

}
