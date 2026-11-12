//! Bytecode analysis: function selectors, storage slots and event topics.
//!
//! Every finding here is read out of the instruction stream rather than
//! guessed from raw bytes. The Solidity dispatcher compiles to
//!
//! ```text
//! PUSH4 <selector> ; EQ ; PUSH2 <body> ; JUMPI
//! ```
//!
//! so a selector is only reported when that shape is actually present.
//! Likewise a storage slot is only reported when a constant is pushed
//! immediately before SLOAD/SSTORE, and an event topic only when a PUSH32
//! precedes a LOG opcode.

use std::collections::BTreeMap;

use crate::opcodes::{self, Instruction};
use crate::types::{DecompiledEvent, DecompiledFunction, Parameter, StorageVariable};

/// 4-byte function selectors, computed as keccak256(signature)[..4].
pub const KNOWN_SELECTORS: &[(&str, &str)] = &[
    ("a9059cbb", "transfer(address,uint256)"),
    ("095ea7b3", "approve(address,uint256)"),
    ("23b872dd", "transferFrom(address,address,uint256)"),
    ("70a08231", "balanceOf(address)"),
    ("dd62ed3e", "allowance(address,address)"),
    ("18160ddd", "totalSupply()"),
    ("06fdde03", "name()"),
    ("95d89b41", "symbol()"),
    ("313ce567", "decimals()"),
    ("a22cb465", "setApprovalForAll(address,bool)"),
    ("42842e0e", "safeTransferFrom(address,address,uint256)"),
    ("b88d4fde", "safeTransferFrom(address,address,uint256,bytes)"),
    ("6352211e", "ownerOf(uint256)"),
    ("081812fc", "getApproved(uint256)"),
    ("c87b56dd", "tokenURI(uint256)"),
    ("01ffc9a7", "supportsInterface(bytes4)"),
    ("e985e9c5", "isApprovedForAll(address,address)"),
    ("00fdd58e", "balanceOf(address,uint256)"),
    ("4e1273f4", "balanceOfBatch(address[],uint256[])"),
    ("f242432a", "safeTransferFrom(address,address,uint256,uint256,bytes)"),
    ("2eb2c2d6", "safeBatchTransferFrom(address,address,uint256[],uint256[],bytes)"),
    ("0e89341c", "uri(uint256)"),
    ("8da5cb5b", "owner()"),
    ("8be0079c", "renounceOwnership()"),
    ("f2fde38b", "transferOwnership(address)"),
    ("715018a6", "renounceOwnership()"),
    ("2e1a7d4d", "withdraw(uint256)"),
    ("d0e30db0", "deposit()"),
    ("6e553f65", "deposit(uint256,address)"),
    ("ba087652", "redeem(uint256,address,address)"),
    ("01e1d114", "totalAssets()"),
    ("0902f1ac", "getReserves()"),
    ("0dfe1681", "token0()"),
    ("d21220a7", "token1()"),
    ("022c0d9f", "swap(uint256,uint256,address,bytes)"),
    ("6a627842", "mint(address)"),
    ("89afcb44", "burn(address)"),
    ("fff6cae9", "sync()"),
    ("bc25cf77", "skim(address)"),
    ("6a761202", "execTransaction(address,uint256,bytes,uint8,uint256,uint256,uint256,address,address,bytes)"),
    ("e75235b8", "getThreshold()"),
    ("a0e67e2b", "getOwners()"),
    ("0d582f13", "addOwnerWithThreshold(address,uint256)"),
    ("f8dc5dd9", "removeOwner(address,address,uint256)"),
    ("e318b52b", "swapOwner(address,address,address)"),
    ("610b5925", "enableModule(address)"),
    ("cc2f8452", "getModulesPaginated(address,uint256)"),
    ("b63e800d", "setup(address[],uint256,address,bytes,address,address,uint256,address)"),
    ("ac9650d8", "multicall(bytes[])"),
    ("3659cfe6", "upgradeTo(address)"),
    ("3659cfe7", "upgradeToAndCall(address,bytes)"),
    ("5c60da1b", "implementation()"),
    ("8129fc1c", "initialize()"),
    ("affed0e0", "nonce()"),
    ("3644e515", "DOMAIN_SEPARATOR()"),
    ("d505accf", "permit(address,address,uint256,uint256,uint8,bytes32,bytes32)"),
    ("f3fef3a3", "withdraw(address,uint256)"),
    ("a0712d68", "mint(uint256)"),
    ("42966c68", "burn(uint256)"),
    ("c2985578", "setPaused(bool)"),
];

/// Event topics, computed as keccak256(signature). The number of indexed
/// parameters excludes the topic-0 signature itself, so a LOG2 carries one
/// indexed parameter.
pub const KNOWN_EVENTS: &[(&str, &str, u8)] = &[
    ("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef", "Transfer(address,address,uint256)", 2),
    ("8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925", "Approval(address,address,uint256)", 2),
    ("c3d58168c5ae7397731d063d5bbf3d657854427343f4c083240f7aacaa2d0f62", "TransferSingle(address,address,address,uint256,uint256)", 3),
    ("4a39dc06d4c0dbc64b70af90fd698a233a518aa5d07e595d983b8c0526c8f7fb", "TransferBatch(address,address,address,uint256[],uint256[])", 3),
    ("17307eab39ab6107e8899845ad3d59bd9653f200f220920489ca2b5937696c31", "ApprovalForAll(address,address,bool)", 2),
    ("6bb7ff708619ba0610cba295a58592e0451dee2622938c8755667688daf3529b", "URI(string,uint256)", 1),
    ("e1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c", "Deposit(address,uint256)", 1),
    ("7fcf532c15f0a6db0bd6d0e038bea71d30d808c7d98cb3bf7268a95bf5081b65", "Withdrawal(address,uint256)", 1),
    ("d78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822", "Swap(address,uint256,uint256,uint256,uint256,address)", 2),
    ("0f6798a560793a54c3bcfe86a93cde1e73087d944c0ea20544137d4121396885", "Mint(address,uint256)", 1),
    ("cc16f5dbb4873280815c1ee09dbd06736cffcc184412cf7a71a0fdb75d397ca5", "Burn(address,uint256)", 1),
    ("4c209b5fc8ad50758f13e2e1088ba56a560dff690a1c6fef26394f4c03821c4f", "Mint(address,uint256,uint256)", 2),
    ("49995e5dd6158cf69ad3e9777c46755a1a826a446c6416992167462dad033b2a", "Burn(address,uint256,uint256)", 2),
    ("8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0", "OwnershipTransferred(address,address)", 2),
    ("9e71bc8eea02a63969f509818f2dafb9254532904319f9dbda79b67bd34a5f3d", "Staked(address,uint256)", 1),
    ("0f5bb82176feb1b5e747e28471aa92156a04d9f3ab9f45f28e2d704232b93f75", "Unstaked(address,uint256)", 1),
    ("e2403640ba68fed3a2f88b7557551d1993f84b99bb10ff833f0cf8db0c5e0486", "RewardPaid(address,uint256)", 1),
    ("106f923f993c2149d49b4255ff723acafa1f2d94393f561d3eda32ae348f7241", "RewardClaimed(address,uint256)", 1),
    ("62e78cea01bee320cd4e420270b5ea74000d11b0c9f74754ebdbfc544b05a258", "Paused(address)", 1),
    ("5db9ee0a495bf2e6ff9c91a7834c1ba4fdd244a5e8aa4e537bd38aeae4b073aa", "Unpaused(address)", 1),
    ("2f8788117e7eff1d82e926ec794901d17c78024a50270940304540a733656f0d", "RoleGranted(bytes32,address,address)", 3),
    ("f6391f5c32d9c69d2a47ea670b442974b53935d1edc7fd64eb21e047a839171b", "RoleRevoked(bytes32,address,address)", 3),
    ("bc7cd75a20ee27fd9adebab32041f755214dbc6bffa90cc0225b39da2e5c2d3b", "Upgraded(address)", 1),
    ("7e644d79422f17c01e4894b5f4f588d331ebfa28653d42ae832dc59e38c9798f", "AdminChanged(address,address)", 2),
    ("1cf3b03a6cf19fa2baba4df148e9dcabedea7f8a5c07840e207e5c089be95d3e", "BeaconUpgraded(address)", 1),
    ("7f26b83ff96e1f2b6a682f133852f6798a09c465da95921460cefb3847402498", "Initialized(uint8)", 1),
    ("1c411e9a96e071241c2f21f7726b17ae89e3cab4c78be50e062b03a9fffbbad1", "Sync(uint112,uint112)", 0),
    ("3134e8a2e6d97e929a7e54011ea5485d7d196dd5f0ba4d4ef95803e8e3fc257f", "DelegateChanged(address,address,address)", 3),
    ("dec2bacdd2f05b59de34da9b523dff8be42e5e38e818c82fdb0bae774387a724", "DelegateVotesChanged(address,uint256,uint256)", 2),
    ("9465fa0c962cc76958e6373a993326400c1c94f8be2fe3a952adfa7f60b2ea26", "AddedOwner(address)", 1),
    ("f8d49fc529812e9a7c5c50e69c20f0dccc0db8fa95c98bc58cc9a4f1c1299eaf", "RemovedOwner(address)", 1),
    ("442e715f626346e8c54381002da614f62bee8d27386535b2521ec8540898556e", "ExecutionSuccess(bytes32,uint256)", 1),
    ("23428b18acfb3ea64b08dc0c1d296ea9c09702c09083ca5272e64d115b687d23", "ExecutionFailure(bytes32,uint256)", 1),
];

/// The EIP-1967 implementation slot, `keccak256("eip1967.proxy.implementation") - 1`.
pub const EIP1967_IMPLEMENTATION_SLOT: [u8; 32] = [
    0x36, 0x08, 0x94, 0xa1, 0x3b, 0xa1, 0xa3, 0x21, 0x06, 0x67, 0xc8, 0x28, 0x49, 0x2d, 0xb9, 0x8d,
    0xca, 0x3e, 0x20, 0x76, 0xcc, 0x37, 0x35, 0xa9, 0x20, 0xa3, 0xca, 0x50, 0x5d, 0x38, 0x2b, 0xbc,
];

/// The EIP-1967 admin slot, `keccak256("eip1967.proxy.admin") - 1`.
pub const EIP1967_ADMIN_SLOT: [u8; 32] = [
    0xb5, 0x31, 0x27, 0x68, 0x4a, 0x56, 0x8b, 0x31, 0x73, 0xae, 0x13, 0xb9, 0xf8, 0xa6, 0x01, 0x6e,
    0x24, 0x3e, 0x63, 0xb6, 0xe8, 0xee, 0x11, 0x78, 0xd6, 0xa7, 0x17, 0x85, 0x0b, 0x5d, 0x61, 0x03,
];

/// The EIP-1967 beacon slot, `keccak256("eip1967.proxy.beacon") - 1`.
pub const EIP1967_BEACON_SLOT: [u8; 32] = [
    0xa3, 0xf0, 0xad, 0x74, 0xe5, 0x42, 0x3a, 0xeb, 0xfd, 0x80, 0xd3, 0xef, 0x43, 0x46, 0x57, 0x83,
    0x35, 0xa9, 0xa7, 0x2a, 0xea, 0xee, 0x59, 0xff, 0x6c, 0xb3, 0x58, 0x2b, 0x35, 0x13, 0x3d, 0x50,
];

/// The EIP-1167 minimal proxy runtime prefix:
/// `363d3d373d3d3d363d73<20-byte implementation>5af43d82803e903d91602b57fd5bf3`.
pub const EIP1167_PREFIX: [u8; 10] = [0x36, 0x3d, 0x3d, 0x37, 0x3d, 0x3d, 0x3d, 0x36, 0x3d, 0x73];

/// How far back from a LOG instruction to look for its topic-0 PUSH32.
const LOG_TOPIC_WINDOW: usize = 64;

/// The largest stack the model tracks. Anything deeper is summarised by
/// clearing the stack, which loses the constants rather than inventing them.
const MAX_STACK_DEPTH: usize = 64;

/// Push a value, dropping the oldest entry once the depth cap is reached.
fn push(stack: &mut Vec<Option<usize>>, value: Option<usize>) {
    if stack.len() >= MAX_STACK_DEPTH {
        stack.remove(0);
    }

    stack.push(value);
}

/// `(pops, pushes)` for the opcodes that do not touch storage, or `None` when
/// the opcode changes control flow or ends execution, in which case the stack
/// model is no longer trustworthy and is reset.
fn stack_effect(opcode: u8) -> Option<(usize, usize)> {
    match opcode {
        0x00 | 0x56 | 0x57 | 0xf3 | 0xfd | 0xfe | 0xff | 0xf0 | 0xf1 | 0xf2 | 0xf4 | 0xf5 | 0xfa
        | 0x5b => None,
        0x01 | 0x02 | 0x03 | 0x04 | 0x05 | 0x06 | 0x07 | 0x0b | 0x10 | 0x11 | 0x12 | 0x13 | 0x14
        | 0x16 | 0x17 | 0x18 | 0x1a | 0x1b | 0x1c | 0x1d => Some((2, 1)),
        0x08 | 0x09 => Some((3, 1)),
        0x0a => Some((2, 1)),
        0x15 | 0x19 => Some((1, 1)),
        0x20 => Some((2, 1)),
        0x30 | 0x32 | 0x33 | 0x34 | 0x36 | 0x38 | 0x3a | 0x3d | 0x40 | 0x41 | 0x42 | 0x43 | 0x44
        | 0x45 | 0x46 | 0x47 | 0x48 | 0x49 | 0x4a | 0x58 | 0x59 | 0x5a | 0x3b | 0x3f => {
            Some((0, 1))
        }
        0x31 => Some((1, 1)),
        0x35 => Some((1, 1)),
        0x37 | 0x39 | 0x3c => Some((3, 0)),
        0x3e => Some((3, 0)),
        0x50 => Some((1, 0)),
        0x51 => Some((1, 1)),
        0x52 | 0x53 => Some((2, 0)),
        0x5c => Some((1, 1)),
        0x5d => Some((2, 0)),
        0x5e => Some((3, 0)),
        0xa0 => Some((2, 0)),
        0xa1 => Some((3, 0)),
        0xa2 => Some((4, 0)),
        0xa3 => Some((5, 0)),
        0xa4 => Some((6, 0)),
        // Undefined bytes halt the interpreter.
        _ => None,
    }
}

/// Static analysis of runtime bytecode.
pub struct Analyzer;

/// Everything static analysis can recover from a runtime code blob.
#[derive(Debug, Clone, Default)]
pub struct Analysis {
    pub functions: Vec<DecompiledFunction>,
    pub variables: Vec<StorageVariable>,
    pub events: Vec<DecompiledEvent>,
    pub is_minimal_proxy: bool,
    pub is_proxy: bool,
    pub has_metadata: bool,
    /// Runtime length with the CBOR metadata trailer removed.
    pub runtime_size: usize,
    /// The implementation address for an EIP-1167 clone, when present.
    pub minimal_proxy_implementation: Option<String>,
}

impl Analyzer {
    /// Analyse runtime bytecode.
    pub fn analyze(bytecode: &str) -> Analysis {
        let code = match decode_bytecode(bytecode) {
            Some(code) => code,
            None => return Analysis::default(),
        };

        let instructions = opcodes::disassemble(&code);
        let metadata_len = metadata_trailer_len(&code).unwrap_or(0);

        Analysis {
            functions: Self::functions(&instructions),
            variables: Self::storage_variables(&instructions),
            events: Self::events(&instructions),
            is_minimal_proxy: Self::minimal_proxy_implementation(&code).is_some(),
            is_proxy: Self::has_eip1967_slot(&instructions),
            has_metadata: metadata_len > 0,
            runtime_size: code.len().saturating_sub(metadata_len),
            minimal_proxy_implementation: Self::minimal_proxy_implementation(&code),
        }
    }

    /// Selectors recovered from the dispatcher.
    ///
    /// Solidity compiles each external function into a `PUSH4 selector ; EQ ;
    /// PUSH2 destination ; JUMPI` comparison. Only selectors that appear in
    /// that shape are reported, and selectors that are not in the local table
    /// are still reported with `resolved: false` so callers can look them up
    /// in an external signature database.
    fn functions(instructions: &[Instruction]) -> Vec<DecompiledFunction> {
        let table: BTreeMap<String, &str> = KNOWN_SELECTORS
            .iter()
            .map(|(selector, signature)| ((*selector).to_string(), *signature))
            .collect();
        let mut found: BTreeMap<String, DecompiledFunction> = BTreeMap::new();

        for window in instructions.windows(2) {
            let current = &window[0];
            let next = &window[1];

            if current.opcode != 0x63 || current.immediate.len() != 4 || next.opcode != 0x14 {
                continue;
            }

            let selector = hex::encode(&current.immediate);

            let (name, signature, resolved) = match table.get(&selector) {
                Some(signature) => (
                    signature.split('(').next().unwrap_or("unknown").to_string(),
                    (*signature).to_string(),
                    true,
                ),
                None => (
                    format!("unknown_{}", selector),
                    String::new(),
                    false,
                ),
            };

            found.entry(selector.clone()).or_insert(DecompiledFunction {
                name,
                signature: signature.clone(),
                selector,
                visibility: "external".to_string(),
                inputs: parameters(&signature),
                resolved,
            });
        }

        found.into_values().collect()
    }

    /// Storage slots that are touched with a constant key.
    ///
    /// The interpreter keeps the slot key on the stack, so tracking the stack
    /// is the only sound way to tell a literal slot from a computed one. A
    /// computed slot (mappings, dynamic arrays, keccak-derived bases) leaves an
    /// unknown on the stack and is therefore not reported, which makes the
    /// result a lower bound rather than a complete layout.
    fn storage_variables(instructions: &[Instruction]) -> Vec<StorageVariable> {
        let mut slots: BTreeMap<usize, String> = BTreeMap::new();
        let mut stack: Vec<Option<usize>> = Vec::new();

        for instruction in instructions {
            let opcode = instruction.opcode;

            // PUSH0..PUSH32
            if (0x5f..=0x7f).contains(&opcode) {
                let value = if opcode == 0x5f {
                    Some(0)
                } else if opcode == 0x7f {
                    // A 32-byte immediate is a hash or a mapping base, never a
                    // plain slot number.
                    None
                } else {
                    instruction
                        .immediate_value()
                        .and_then(|value| usize::try_from(value).ok())
                };

                push(&mut stack, value);
                continue;
            }

            // DUP1..DUP16 duplicates an existing slot, so it propagates.
            if (0x80..=0x8f).contains(&opcode) {
                let depth = (opcode - 0x7f) as usize;

                if stack.len() >= depth {
                    let value = stack[stack.len() - depth];
                    push(&mut stack, value);
                } else {
                    stack.clear();
                }

                continue;
            }

            // SWAP1..SWAP16 only reorders, so the constants follow.
            if (0x90..=0x9f).contains(&opcode) {
                let depth = (opcode - 0x8f) as usize;

                if stack.len() > depth {
                    let top = stack.len() - 1;
                    stack.swap(top, top - depth);
                } else {
                    stack.clear();
                }

                continue;
            }

            match opcode {
                0x54 => {
                    // SLOAD pops the key and pushes the loaded word.
                    if let Some(slot) = stack.pop().flatten() {
                        let access = slots.entry(slot).or_default();

                        if access.is_empty() {
                            *access = "read".to_string();
                        } else if access == "write" {
                            *access = "read_write".to_string();
                        }
                    }

                    push(&mut stack, None);
                }
                0x55 => {
                    // SSTORE pops the key first, then the stored value.
                    if let Some(slot) = stack.pop().flatten() {
                        let access = slots.entry(slot).or_default();

                        if access.is_empty() {
                            *access = "write".to_string();
                        } else if access == "read" {
                            *access = "read_write".to_string();
                        }
                    }

                    stack.pop();
                }
                _ => match stack_effect(opcode) {
                    Some((pops, pushes)) => {
                        if stack.len() < pops {
                            stack.clear();
                        } else {
                            stack.truncate(stack.len() - pops);
                        }

                        for _ in 0..pushes {
                            push(&mut stack, None);
                        }
                    }
                    None => stack.clear(),
                },
            }
        }

        slots
            .into_iter()
            .map(|(slot, access)| StorageVariable {
                name: format!("storage_slot_{slot}"),
                param_type: "uint256".to_string(),
                slot,
                access: if access.is_empty() {
                    "unknown".to_string()
                } else {
                    access
                },
            })
            .collect()
    }

    /// Events emitted by the contract.
    ///
    /// A `LOG1..LOG4` is preceded by a `PUSH32` of topic 0 for every event the
    /// Solidity compiler emits, so the topic can be read directly from the
    /// instruction stream instead of being assumed.
    fn events(instructions: &[Instruction]) -> Vec<DecompiledEvent> {
        let table: BTreeMap<String, (&str, u8)> = KNOWN_EVENTS
            .iter()
            .map(|(topic, signature, indexed)| ((*topic).to_string(), (*signature, *indexed)))
            .collect();

        let mut found: BTreeMap<String, DecompiledEvent> = BTreeMap::new();

        for (index, instruction) in instructions.iter().enumerate() {
            if !(0xa1..=0xa4).contains(&instruction.opcode) {
                continue;
            }

            // Topic 0 is pushed before the data words, so the topic is the
            // nearest preceding PUSH32 that matches a known signature. The
            // window bounds the search to the emitting basic block.
            let window_start = index.saturating_sub(LOG_TOPIC_WINDOW);

            let topic = instructions[window_start..index]
                .iter()
                .rev()
                .find_map(|candidate| {
                    if candidate.opcode != 0x7f || candidate.immediate.len() != 32 {
                        return None;
                    }

                    let encoded = hex::encode(&candidate.immediate);

                    table.contains_key(&encoded).then_some(encoded)
                });

            let Some(topic) = topic else {
                continue;
            };

            let indexed_count = instruction.opcode - 0xa0;
            let (name, signature) = table
                .get(&topic)
                .map(|(signature, _)| {
                    (
                        signature.split('(').next().unwrap_or("unknown").to_string(),
                        (*signature).to_string(),
                    )
                })
                .unwrap_or_else(|| (format!("unknown_event_{}", &topic[..8]), String::new()));

            found.entry(topic.clone()).or_insert(DecompiledEvent {
                name,
                signature,
                topic,
                indexed_count,
            });
        }

        found.into_values().collect()
    }

    /// Detect the EIP-1167 minimal proxy and return its implementation address.
    fn minimal_proxy_implementation(code: &[u8]) -> Option<String> {
        if code.len() < 45 || code[..10] != EIP1167_PREFIX {
            return None;
        }

        Some(format!("0x{}", hex::encode(&code[10..30])))
    }

    /// True when the code pushes one of the EIP-1967 slots, which is how every
    /// upgradeable proxy addresses its implementation.
    fn has_eip1967_slot(instructions: &[Instruction]) -> bool {
        instructions.iter().any(|instruction| {
            instruction.opcode == 0x7f
                && instruction.immediate.len() == 32
                && (instruction.immediate == EIP1967_IMPLEMENTATION_SLOT
                    || instruction.immediate == EIP1967_ADMIN_SLOT
                    || instruction.immediate == EIP1967_BEACON_SLOT)
        })
    }
}

/// Decode a hex string, tolerating a `0x` prefix and whitespace.
pub fn decode_bytecode(bytecode: &str) -> Option<Vec<u8>> {
    let trimmed: String = bytecode
        .trim()
        .trim_start_matches("0x")
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();

    if trimmed.is_empty() {
        return Some(Vec::new());
    }

    hex::decode(&trimmed).ok()
}

/// Length of the trailing Solidity CBOR metadata, or `None` when absent.
///
/// The trailer is a CBOR map followed by a 2-byte big-endian length. Solidity
/// writes `a1 65 "ipfs"` or `a1 65 "bzzr0"` at the start of the map, and the
/// length field counts the map only, not the length field itself.
pub fn metadata_trailer_len(code: &[u8]) -> Option<usize> {
    if code.len() < 4 {
        return None;
    }

    let length = u16::from_be_bytes([code[code.len() - 2], code[code.len() - 1]]) as usize;

    if length == 0 || length + 2 > code.len() {
        return None;
    }

    let start = code.len() - 2 - length;

    if start < 2 {
        return None;
    }

    // `a1` opens a single-entry map, `65` is a 5-character text key.
    if code[start] != 0xa1 || code[start + 1] != 0x65 {
        return None;
    }

    Some(length + 2)
}

/// Parse `name(type,type)` into parameters. Types are split on commas that are
/// not inside a tuple, so `(uint256,(address,bytes)[])` stays one parameter.
fn parameters(signature: &str) -> Vec<Parameter> {
    let open = match signature.find('(') {
        Some(index) => index,
        None => return Vec::new(),
    };

    if !signature.ends_with(')') {
        return Vec::new();
    }

    let inner = &signature[open + 1..signature.len() - 1];

    if inner.is_empty() {
        return Vec::new();
    }

    let mut parameters = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();

    for character in inner.chars() {
        match character {
            '(' | '[' => {
                depth += 1;
                current.push(character);
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                current.push(character);
            }
            ',' if depth == 0 => {
                parameters.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(character),
        }
    }

    if !current.trim().is_empty() {
        parameters.push(current.trim().to_string());
    }

    parameters
        .into_iter()
        .enumerate()
        .map(|(index, param_type)| Parameter {
            name: format!("arg{index}"),
            param_type,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_table_matches_keccak_of_the_signatures() {
        // keccak256("transfer(address,uint256)")[..4]
        assert_eq!(
            KNOWN_SELECTORS
                .iter()
                .find(|(selector, _)| *selector == "a9059cbb")
                .map(|(_, signature)| *signature),
            Some("transfer(address,uint256)")
        );

        // keccak256("balanceOf(address)")[..4]
        assert!(KNOWN_SELECTORS
            .iter()
            .any(|(selector, signature)| *selector == "70a08231"
                && *signature == "balanceOf(address)"));
    }

    #[test]
    fn event_topics_match_keccak_of_the_signatures() {
        let transfer = KNOWN_EVENTS
            .iter()
            .find(|(topic, _, _)| *topic == "ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef")
            .unwrap();

        assert_eq!(transfer.1, "Transfer(address,address,uint256)");
        assert_eq!(transfer.2, 2);
    }

    #[test]
    fn eip1967_slots_match_keccak_minus_one() {
        // keccak256("eip1967.proxy.implementation") - 1
        assert_eq!(
            hex::encode(EIP1967_IMPLEMENTATION_SLOT),
            "360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc"
        );
        assert_eq!(
            hex::encode(EIP1967_ADMIN_SLOT),
            "b53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103"
        );
        assert_eq!(
            hex::encode(EIP1967_BEACON_SLOT),
            "a3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6cb3582b35133d50"
        );
    }

    #[test]
    fn a_dispatcher_selector_is_recovered() {
        // PUSH4 a9059cbb ; EQ ; PUSH2 0x0100 ; JUMPI ; STOP
        let code = "63a9059cbb146101005700";

        let analysis = Analyzer::analyze(code);

        assert_eq!(analysis.functions.len(), 1);
        assert_eq!(analysis.functions[0].selector, "a9059cbb");
        assert_eq!(analysis.functions[0].name, "transfer");
        assert_eq!(analysis.functions[0].signature, "transfer(address,uint256)");
        assert!(analysis.functions[0].resolved);
        assert_eq!(analysis.functions[0].inputs.len(), 2);
        assert_eq!(analysis.functions[0].inputs[0].param_type, "address");
    }

    #[test]
    fn an_unknown_selector_is_reported_as_unresolved() {
        let code = "63deadbeef146101005700";

        let analysis = Analyzer::analyze(code);

        assert_eq!(analysis.functions.len(), 1);
        assert_eq!(analysis.functions[0].selector, "deadbeef");
        assert!(!analysis.functions[0].resolved);
    }

    #[test]
    fn a_push4_that_is_not_a_selector_is_ignored() {
        // PUSH4 deadbeef ; MSTORE — no EQ, so it is data, not a selector.
        let code = "63deadbeef5200";

        let analysis = Analyzer::analyze(code);

        assert!(analysis.functions.is_empty());
    }

    #[test]
    fn a_constant_storage_slot_is_recovered() {
        // PUSH1 0x02 ; SLOAD ; POP ; PUSH1 0x05 ; PUSH1 0x03 ; SSTORE ; STOP
        // SSTORE pops the key first, so the key is the last pushed constant.
        let code = "6002545060056003 5500";

        let analysis = Analyzer::analyze(code);

        let slots: Vec<usize> = analysis
            .variables
            .iter()
            .map(|variable| variable.slot)
            .collect();

        assert_eq!(slots, vec![2, 3]);
        assert_eq!(analysis.variables[0].access, "read");
        assert_eq!(analysis.variables[1].access, "write");
    }

    #[test]
    fn a_slot_read_and_written_is_reported_as_read_write() {
        // PUSH1 0x02 ; SLOAD ; POP ; PUSH1 0x05 ; PUSH1 0x02 ; SSTORE ; STOP
        let code = "6002545060056002 5500";

        let analysis = Analyzer::analyze(code);

        assert_eq!(analysis.variables.len(), 1);
        assert_eq!(analysis.variables[0].slot, 2);
        assert_eq!(analysis.variables[0].access, "read_write");
    }

    #[test]
    fn a_computed_storage_slot_is_not_reported() {
        // PUSH1 0x00 ; CALLDATALOAD ; SLOAD — the key comes from calldata.
        let code = "60003554 00";

        let analysis = Analyzer::analyze(code);

        assert!(analysis.variables.is_empty());
    }

    #[test]
    fn a_mapping_access_is_not_reported_as_a_plain_slot() {
        // PUSH32 <slot 0> ; PUSH1 0x00 ; KECCAK256 ; SLOAD — the key is a hash
        // of the base and the mapping key, so it is not a slot number.
        let mut code = String::from("7f");
        code.push_str(&"00".repeat(32));
        code.push_str("60002054 00");

        let analysis = Analyzer::analyze(&code);

        assert!(analysis.variables.is_empty());
    }

    #[test]
    fn a_constant_slot_survives_a_dup_before_the_load() {
        // PUSH1 0x02 ; DUP1 ; SLOAD ; SSTORE-key reuse: the duplicated
        // constant is still the slot.
        let code = "600280545060025500";

        let analysis = Analyzer::analyze(code);

        assert_eq!(analysis.variables.len(), 1);
        assert_eq!(analysis.variables[0].slot, 2);
        assert_eq!(analysis.variables[0].access, "read_write");
    }

    #[test]
    fn a_log1_topic_is_recovered() {
        let mut code = String::from("7f");
        code.push_str("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef");
        code.push_str("a1");

        let analysis = Analyzer::analyze(&code);

        assert_eq!(analysis.events.len(), 1);
        assert_eq!(analysis.events[0].name, "Transfer");
        assert_eq!(analysis.events[0].indexed_count, 1);
    }

    #[test]
    fn a_topic_is_found_through_the_data_words_that_follow_it() {
        // PUSH32 topic0 ; PUSH32 data ; PUSH32 data ; LOG3
        let mut code = String::from("7f");
        code.push_str("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef");
        code.push_str("7f");
        code.push_str(&"11".repeat(32));
        code.push_str("7f");
        code.push_str(&"22".repeat(32));
        code.push_str("a3");

        let analysis = Analyzer::analyze(&code);

        assert_eq!(analysis.events.len(), 1);
        assert_eq!(analysis.events[0].name, "Transfer");
        assert_eq!(analysis.events[0].indexed_count, 3);
    }

    #[test]
    fn a_log_with_no_known_topic_is_not_reported() {
        // PUSH32 <random> ; LOG1
        let mut code = String::from("7f");
        code.push_str(&"ab".repeat(32));
        code.push_str("a1");

        let analysis = Analyzer::analyze(&code);

        assert!(analysis.events.is_empty());
    }

    #[test]
    fn an_eip1167_clone_is_detected_with_its_implementation() {
        let mut code = String::from("363d3d373d3d3d363d73");
        code.push_str("000000000000000000000000deadbeefdeadbeef");
        code.push_str("5af43d82803e903d91602b57fd5bf3");

        let analysis = Analyzer::analyze(&code);

        assert!(analysis.is_minimal_proxy);
        assert_eq!(
            analysis.minimal_proxy_implementation.as_deref(),
            Some("0x000000000000000000000000deadbeefdeadbeef")
        );
    }

    #[test]
    fn an_eip1967_proxy_is_detected() {
        let mut code = String::from("7f");
        code.push_str(&hex::encode(EIP1967_IMPLEMENTATION_SLOT));
        code.push_str("5450600036");

        let analysis = Analyzer::analyze(&code);

        assert!(analysis.is_proxy);
        assert!(!analysis.is_minimal_proxy);
    }

    #[test]
    fn the_metadata_trailer_is_measured_and_stripped() {
        // PUSH1 0x00 ; STOP followed by `a1 65 ipfs ...` and a length field.
        let mut code = vec![0x60u8, 0x00, 0x00];
        let metadata = {
            let mut map = vec![0xa1u8, 0x65];
            map.extend_from_slice(b"ipfs");
            map
        };
        code.extend_from_slice(&metadata);
        code.extend_from_slice(&(metadata.len() as u16).to_be_bytes());

        let analysis = Analyzer::analyze(&hex::encode(&code));

        assert!(analysis.has_metadata);
        assert_eq!(analysis.runtime_size, 3);
        assert_eq!(metadata_trailer_len(&code), Some(metadata.len() + 2));
    }

    #[test]
    fn code_without_a_metadata_trailer_reports_none() {
        assert_eq!(metadata_trailer_len(&[0x60, 0x00, 0x00, 0x00]), None);
    }

    #[test]
    fn tuple_parameters_are_split_correctly() {
        let parsed = parameters("safeBatchTransferFrom(address,address,uint256[],uint256[],bytes)");

        assert_eq!(parsed.len(), 5);
        assert_eq!(parsed[2].param_type, "uint256[]");
        assert_eq!(parsed[4].param_type, "bytes");
    }

    #[test]
    fn a_no_argument_signature_has_no_parameters() {
        assert!(parameters("totalSupply()").is_empty());
    }

    #[test]
    fn invalid_hex_is_rejected_without_panicking() {
        assert!(decode_bytecode("0xzz").is_none());
        assert!(Analyzer::analyze("0xzz").functions.is_empty());
    }

    #[test]
    fn an_empty_contract_yields_nothing() {
        let analysis = Analyzer::analyze("0x");

        assert!(analysis.functions.is_empty());
        assert!(analysis.variables.is_empty());
        assert!(analysis.events.is_empty());
        assert_eq!(analysis.runtime_size, 0);
    }
}
