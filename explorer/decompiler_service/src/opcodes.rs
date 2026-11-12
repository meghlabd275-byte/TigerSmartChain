//! EVM opcode table and linear disassembler.
//!
//! A raw byte scan is not a disassembler: PUSH immediates are data, not code,
//! and reading them as opcodes invents instructions that the EVM never
//! executes. `disassemble` walks the bytecode the way the EVM does, consuming
//! each PUSH immediate as an operand.

/// One decoded instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Byte offset of the opcode in the code.
    pub offset: usize,
    pub opcode: u8,
    pub name: &'static str,
    /// The immediate operand of a PUSH instruction, big-endian, empty otherwise.
    pub immediate: Vec<u8>,
    /// Offset of the operand, when the instruction has one.
    pub immediate_offset: Option<usize>,
}

impl Instruction {
    /// The full mnemonic, including the PUSH/DUP/SWAP ordinal.
    pub fn mnemonic(&self) -> String {
        mnemonic(self.opcode)
    }

    /// The immediate as an unsigned integer. Immediates longer than 16 bytes are
    /// truncated to their low 16 bytes, which is enough for offsets and lengths.
    pub fn immediate_value(&self) -> Option<u128> {
        if self.immediate.is_empty() {
            return None;
        }

        let mut value: u128 = 0;

        for byte in self.immediate.iter().take(16) {
            value = (value << 8) | *byte as u128;
        }

        Some(value)
    }

    /// The immediate as a full 32-byte, left-padded value.
    pub fn immediate_u256(&self) -> Option<[u8; 32]> {
        if self.immediate.is_empty() {
            return None;
        }

        let mut word = [0u8; 32];
        let source = &self.immediate[self.immediate.len().saturating_sub(32)..];
        word[32 - source.len()..].copy_from_slice(source);

        Some(word)
    }
}

/// The number of immediate bytes that follow a PUSH opcode.
pub fn push_size(opcode: u8) -> usize {
    match opcode {
        0x60..=0x7f => opcode as usize - 0x5f,
        _ => 0,
    }
}

/// The mnemonic for an opcode, or `UNKNOWN` for undefined bytes.
pub fn name(opcode: u8) -> &'static str {
    match opcode {
        0x00 => "STOP",
        0x01 => "ADD",
        0x02 => "MUL",
        0x03 => "SUB",
        0x04 => "DIV",
        0x05 => "SDIV",
        0x06 => "MOD",
        0x07 => "SMOD",
        0x08 => "ADDMOD",
        0x09 => "MULMOD",
        0x0a => "EXP",
        0x0b => "SIGNEXTEND",
        0x10 => "LT",
        0x11 => "GT",
        0x12 => "SLT",
        0x13 => "SGT",
        0x14 => "EQ",
        0x15 => "ISZERO",
        0x16 => "AND",
        0x17 => "OR",
        0x18 => "XOR",
        0x19 => "NOT",
        0x1a => "BYTE",
        0x1b => "SHL",
        0x1c => "SHR",
        0x1d => "SAR",
        0x20 => "KECCAK256",
        0x30 => "ADDRESS",
        0x31 => "BALANCE",
        0x32 => "ORIGIN",
        0x33 => "CALLER",
        0x34 => "CALLVALUE",
        0x35 => "CALLDATALOAD",
        0x36 => "CALLDATASIZE",
        0x37 => "CALLDATACOPY",
        0x38 => "CODESIZE",
        0x39 => "CODECOPY",
        0x3a => "GASPRICE",
        0x3b => "EXTCODESIZE",
        0x3c => "EXTCODECOPY",
        0x3d => "RETURNDATASIZE",
        0x3e => "RETURNDATACOPY",
        0x3f => "EXTCODEHASH",
        0x40 => "BLOCKHASH",
        0x41 => "COINBASE",
        0x42 => "TIMESTAMP",
        0x43 => "NUMBER",
        0x44 => "PREVRANDAO",
        0x45 => "GASLIMIT",
        0x46 => "CHAINID",
        0x47 => "SELFBALANCE",
        0x48 => "BASEFEE",
        0x49 => "BLOBHASH",
        0x4a => "BLOBBASEFEE",
        0x50 => "POP",
        0x51 => "MLOAD",
        0x52 => "MSTORE",
        0x53 => "MSTORE8",
        0x54 => "SLOAD",
        0x55 => "SSTORE",
        0x56 => "JUMP",
        0x57 => "JUMPI",
        0x58 => "PC",
        0x59 => "MSIZE",
        0x5a => "GAS",
        0x5b => "JUMPDEST",
        0x5c => "TLOAD",
        0x5d => "TSTORE",
        0x5e => "MCOPY",
        0x5f => "PUSH0",
        0x60..=0x7f => "PUSH",
        0x80..=0x8f => "DUP",
        0x90..=0x9f => "SWAP",
        0xa0 => "LOG0",
        0xa1 => "LOG1",
        0xa2 => "LOG2",
        0xa3 => "LOG3",
        0xa4 => "LOG4",
        0xf0 => "CREATE",
        0xf1 => "CALL",
        0xf2 => "CALLCODE",
        0xf3 => "RETURN",
        0xf4 => "DELEGATECALL",
        0xf5 => "CREATE2",
        0xfa => "STATICCALL",
        0xfd => "REVERT",
        0xfe => "INVALID",
        0xff => "SELFDESTRUCT",
        _ => "UNKNOWN",
    }
}

/// The mnemonic for an opcode, including the PUSH/DUP/SWAP ordinal.
pub fn mnemonic(opcode: u8) -> String {
    match opcode {
        0x60..=0x7f => format!("PUSH{}", push_size(opcode)),
        0x80..=0x8f => format!("DUP{}", opcode - 0x7f),
        0x90..=0x9f => format!("SWAP{}", opcode - 0x8f),
        _ => name(opcode).to_string(),
    }
}

/// True when the opcode pushes an immediate operand.
pub fn is_push(opcode: u8) -> bool {
    (0x60..=0x7f).contains(&opcode)
}

/// Walk the bytecode linearly, decoding instructions and consuming the
/// immediates of PUSH instructions.
///
/// A PUSH whose operand runs past the end of the code is still emitted; the
/// EVM zero-pads a truncated immediate, so dropping the instruction would
/// misreport the program.
pub fn disassemble(code: &[u8]) -> Vec<Instruction> {
    let mut instructions = Vec::new();
    let mut offset = 0;

    while offset < code.len() {
        let opcode = code[offset];
        let size = push_size(opcode);

        let (immediate, immediate_offset) = if size > 0 {
            let start = offset + 1;
            let end = (start + size).min(code.len());
            (code[start..end].to_vec(), Some(start))
        } else {
            (Vec::new(), None)
        };

        instructions.push(Instruction {
            offset,
            opcode,
            name: name(opcode),
            immediate,
            immediate_offset,
        });

        offset += 1 + size;
    }

    instructions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_size_matches_the_opcode_range() {
        assert_eq!(push_size(0x5f), 0);
        assert_eq!(push_size(0x60), 1);
        assert_eq!(push_size(0x61), 2);
        assert_eq!(push_size(0x7f), 32);
        assert_eq!(push_size(0x80), 0);
    }

    #[test]
    fn push_immediates_are_not_decoded_as_opcodes() {
        // PUSH1 0x60 ; PUSH1 0x00 ; MSTORE
        let code = [0x60u8, 0x60, 0x60, 0x00, 0x52];

        let decoded = disassemble(&code);

        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded[0].mnemonic(), "PUSH1");
        assert_eq!(decoded[0].immediate, vec![0x60]);
        assert_eq!(decoded[1].immediate, vec![0x00]);
        assert_eq!(decoded[2].mnemonic(), "MSTORE");
        assert_eq!(decoded[2].offset, 4);
    }

    #[test]
    fn a_truncated_push_keeps_the_bytes_that_exist() {
        let code = [0x62u8, 0xaa, 0xbb];

        let decoded = disassemble(&code);

        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].mnemonic(), "PUSH3");
        assert_eq!(decoded[0].immediate, vec![0xaa, 0xbb]);
        assert_eq!(decoded[0].immediate_value(), Some(0xaabb));
    }

    #[test]
    fn an_immediate_is_read_big_endian() {
        let decoded = disassemble(&[0x61, 0x01, 0x00]);

        assert_eq!(decoded[0].immediate_value(), Some(256));
    }

    #[test]
    fn push0_has_no_immediate() {
        let decoded = disassemble(&[0x5f, 0x50]);

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].mnemonic(), "PUSH0");
        assert!(decoded[0].immediate.is_empty());
        assert_eq!(decoded[0].immediate_value(), None);
    }

    #[test]
    fn push32_is_left_padded_into_a_word() {
        let mut code = vec![0x7fu8];
        code.extend_from_slice(&[0u8; 30]);
        code.extend_from_slice(&[0xde, 0xad]);

        let decoded = disassemble(&code);
        let word = decoded[0].immediate_u256().unwrap();

        assert_eq!(word[30], 0xde);
        assert_eq!(word[31], 0xad);
        assert!(word[..30].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn ordinals_are_appended_for_dup_and_swap() {
        assert_eq!(mnemonic(0x80), "DUP1");
        assert_eq!(mnemonic(0x8f), "DUP16");
        assert_eq!(mnemonic(0x90), "SWAP1");
        assert_eq!(mnemonic(0x9f), "SWAP16");
        assert_eq!(mnemonic(0x54), "SLOAD");
    }

    #[test]
    fn cancun_opcodes_are_named() {
        assert_eq!(name(0x5c), "TLOAD");
        assert_eq!(name(0x5d), "TSTORE");
        assert_eq!(name(0x5e), "MCOPY");
        assert_eq!(name(0x49), "BLOBHASH");
    }
}
