//! Records the exact state writes a traced execution performs.
//!
//! [`StateDiffRecorder`] wraps a [`Host`] and forwards every call to it while
//! keeping a before/after record of the storage slots and balances that were
//! touched. The "before" value is captured from the inner host at the moment of
//! the first write to a slot, so the recorded pair is the real transition the
//! execution caused rather than a reconstruction.

use crate::host::Host;
use crate::types::{Address, EvmWord, Log};
use std::collections::BTreeMap;

/// One storage slot transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageDiff {
    /// Account whose slot changed.
    pub address: Address,
    /// Slot that changed.
    pub slot: EvmWord,
    /// Value before the first write of this execution.
    pub before: EvmWord,
    /// Value after the last write of this execution.
    pub after: EvmWord,
}

/// One balance transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceDiff {
    /// Account whose balance changed.
    pub address: Address,
    /// Balance before the first change of this execution.
    pub before: EvmWord,
    /// Balance after the last change of this execution.
    pub after: EvmWord,
}

/// A host that delegates to an inner host and records the writes it observes.
pub struct StateDiffRecorder<'a> {
    inner: &'a mut dyn Host,
    storage: BTreeMap<(Address, EvmWord), (EvmWord, EvmWord)>,
    balances: BTreeMap<Address, (EvmWord, EvmWord)>,
}

impl<'a> StateDiffRecorder<'a> {
    /// Wrap `inner`.
    pub fn new(inner: &'a mut dyn Host) -> Self {
        Self {
            inner,
            storage: BTreeMap::new(),
            balances: BTreeMap::new(),
        }
    }

    /// Storage slots whose final value differs from their initial value,
    /// ordered by address then slot so the output is deterministic.
    pub fn storage_diffs(&self) -> Vec<StorageDiff> {
        self.storage
            .iter()
            .filter(|(_, (before, after))| before != after)
            .map(|((address, slot), (before, after))| StorageDiff {
                address: *address,
                slot: slot.clone(),
                before: before.clone(),
                after: after.clone(),
            })
            .collect()
    }

    /// Accounts whose final balance differs from their initial balance.
    pub fn balance_diffs(&self) -> Vec<BalanceDiff> {
        self.balances
            .iter()
            .filter(|(_, (before, after))| before != after)
            .map(|(address, (before, after))| BalanceDiff {
                address: *address,
                before: before.clone(),
                after: after.clone(),
            })
            .collect()
    }

    /// Record a balance observation, keeping the earliest "before" and the
    /// latest "after".
    /// `before` must be the balance observed before this execution touched the
    /// account; it cannot be read back from the inner host once the write has
    /// been applied.
    fn observe_balance(&mut self, address: Address, before: EvmWord, after: EvmWord) {
        match self.balances.get_mut(&address) {
            Some((_, latest)) => *latest = after,
            None => {
                self.balances.insert(address, (before, after));
            }
        }
    }

    /// Record a storage observation, keeping the earliest "before" and the
    /// latest "after".
    fn observe_storage(&mut self, address: Address, slot: EvmWord, after: EvmWord) {
        match self.storage.get_mut(&(address, slot.clone())) {
            Some((_, latest)) => *latest = after,
            None => {
                let before = self.inner.storage(&address, &slot);
                self.storage.insert((address, slot), (before, after));
            }
        }
    }
}

impl<'a> Host for StateDiffRecorder<'a> {
    fn balance(&self, address: &Address) -> EvmWord {
        self.inner.balance(address)
    }

    fn exists(&self, address: &Address) -> bool {
        self.inner.exists(address)
    }

    fn code(&self, address: &Address) -> Vec<u8> {
        self.inner.code(address)
    }

    fn code_hash(&self, address: &Address) -> EvmWord {
        self.inner.code_hash(address)
    }

    fn storage(&self, address: &Address, slot: &EvmWord) -> EvmWord {
        self.inner.storage(address, slot)
    }

    fn set_storage(&mut self, address: &Address, slot: EvmWord, value: EvmWord) {
        let after = value.clone();
        self.observe_storage(*address, slot.clone(), after);
        self.inner.set_storage(address, slot, value);
    }

    fn block_hash(&self, number: u64) -> EvmWord {
        self.inner.block_hash(number)
    }

    fn emit_log(&mut self, address: &Address, topics: Vec<EvmWord>, data: Vec<u8>) {
        self.inner.emit_log(address, topics, data);
    }

    fn transfer(&mut self, from: &Address, to: &Address, value: EvmWord) -> bool {
        // The pre-transfer balances must be read before delegating, because the
        // inner host applies the transfer in place.
        let from_before = self.inner.balance(from);
        let to_before = self.inner.balance(to);

        let from_after = from_before.sub(&value);
        let to_after = to_before.add(&value);

        let performed = self.inner.transfer(from, to, value);
        if performed {
            self.observe_balance(*from, from_before, from_after);
            self.observe_balance(*to, to_before, to_after);
        }
        performed
    }

    fn set_code(&mut self, address: &Address, code: Vec<u8>) {
        self.inner.set_code(address, code);
    }

    fn bump_nonce(&mut self, address: &Address) -> u64 {
        self.inner.bump_nonce(address)
    }

    fn selfdestruct(&mut self, address: &Address) {
        self.inner.selfdestruct(address);
    }

    fn logs(&self) -> &[Log] {
        self.inner.logs()
    }

    fn take_logs(&mut self) -> Vec<Log> {
        self.inner.take_logs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::MemoryHost;

    fn addr(b: u8) -> Address {
        Address([b; 20])
    }

    #[test]
    fn only_slots_that_actually_changed_are_reported() {
        let slot = EvmWord::from_u64(1);
        let mut inner =
            MemoryHost::new().with_storage(addr(1), slot.clone(), EvmWord::from_u64(5));

        let mut recorder = StateDiffRecorder::new(&mut inner);
        // Rewrite the same value: no net change.
        recorder.set_storage(&addr(1), slot.clone(), EvmWord::from_u64(5));
        assert!(recorder.storage_diffs().is_empty());

        recorder.set_storage(&addr(1), slot.clone(), EvmWord::from_u64(9));
        let diffs = recorder.storage_diffs();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].before.as_u64(), 5);
        assert_eq!(diffs[0].after.as_u64(), 9);
    }

    #[test]
    fn repeated_writes_report_the_first_before_and_last_after() {
        let slot = EvmWord::from_u64(2);
        let mut inner =
            MemoryHost::new().with_storage(addr(2), slot.clone(), EvmWord::from_u64(100));

        let mut recorder = StateDiffRecorder::new(&mut inner);
        recorder.set_storage(&addr(2), slot.clone(), EvmWord::from_u64(1));
        recorder.set_storage(&addr(2), slot.clone(), EvmWord::from_u64(2));
        recorder.set_storage(&addr(2), slot.clone(), EvmWord::from_u64(3));

        let diffs = recorder.storage_diffs();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].before.as_u64(), 100);
        assert_eq!(diffs[0].after.as_u64(), 3);
    }

    #[test]
    fn a_transfer_reports_both_sides() {
        let mut inner = MemoryHost::new()
            .with_balance(addr(1), EvmWord::from_u64(50))
            .with_balance(addr(2), EvmWord::from_u64(10));

        let mut recorder = StateDiffRecorder::new(&mut inner);
        assert!(recorder.transfer(&addr(1), &addr(2), EvmWord::from_u64(20)));

        let diffs = recorder.balance_diffs();
        assert_eq!(diffs.len(), 2);

        let sender = diffs.iter().find(|d| d.address == addr(1)).unwrap();
        assert_eq!(sender.before.as_u64(), 50);
        assert_eq!(sender.after.as_u64(), 30);

        let recipient = diffs.iter().find(|d| d.address == addr(2)).unwrap();
        assert_eq!(recipient.before.as_u64(), 10);
        assert_eq!(recipient.after.as_u64(), 30);
    }

    #[test]
    fn a_rejected_transfer_records_nothing() {
        let mut inner = MemoryHost::new().with_balance(addr(1), EvmWord::from_u64(5));

        let mut recorder = StateDiffRecorder::new(&mut inner);
        assert!(!recorder.transfer(&addr(1), &addr(2), EvmWord::from_u64(1000)));
        assert!(recorder.balance_diffs().is_empty());
    }

    #[test]
    fn storage_reads_pass_through_to_the_inner_host() {
        let mut inner = MemoryHost::new().with_storage(
            addr(3),
            EvmWord::from_u64(7),
            EvmWord::from_u64(42),
        );

        let recorder = StateDiffRecorder::new(&mut inner);
        assert_eq!(
            recorder.storage(&addr(3), &EvmWord::from_u64(7)).as_u64(),
            42
        );
    }
}
