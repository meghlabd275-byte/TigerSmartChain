//! MCOPY Opcode

/// MCOPY - Memory Copy
pub fn mcopy(dst: usize, src: usize, length: usize, memory: &mut Vec<u8>) {
    if src + length > memory.len() {
        return;
    }
    let data = memory[src..src + length].to_vec();
    if dst + length > memory.len() {
        memory.resize(dst + length, 0);
    }
    memory[dst..dst + length].copy_from_slice(&data);
}

/// Get opcode
pub fn get_opcode() -> u8 {
    0x5e
}