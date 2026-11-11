//! Conformance tool: read a trie description on stdin, print the root hash.
//!
//! Input is one `key_hex<TAB>value_hex` entry per line, with `-` for an empty
//! value. Parsing is line-based so an empty value stays distinct from a missing
//! token.
//!
//! This exists to cross-check the two implementations of the trie against each
//! other on arbitrary inputs: `crypto_cpp/trie.cpp` (the consensus path) and
//! `state/src/trie.rs`. Feed the same input to both and compare the output;
//! they must agree byte for byte. `crypto_cpp/trie_test.cpp` pins the published
//! reference vectors, and this tool is what extends that confidence to inputs
//! the vectors do not cover.
//!
//! Usage:
//!   printf 'aa\tbb\naabb\t-' | cargo run --example trie_tool
use std::io::BufRead;
use tiger_state::trie::Trie;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
        .collect()
}

fn main() {
    let mut trie = Trie::new();
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let Some((k, v)) = line.split_once('\t') else { continue };
        if k.is_empty() {
            continue;
        }
        let value = if v == "-" { String::new() } else { v.to_string() };
        trie.insert(&unhex(k), &unhex(&value));
    }
    println!(
        "{}",
        trie.root_hash().iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
}
