//! Merkle Patricia Trie (Ethereum Yellow Paper, Appendix D).
//!
//! This is the authenticated data structure behind `stateRoot`,
//! `transactionsRoot`, `receiptsRoot` and contract storage. A single
//! non-canonical node encoding yields a different root hash, so exactness
//! matters for interoperability with other EVM clients.
//!
//! Design notes:
//!   * The trie is an owned tree of [`Node`]s. Every mutation rebuilds the
//!     affected path and re-normalises, so the structure is always canonical
//!     and [`Trie::root_hash`] is a cheap read.
//!   * Child references follow the 32-byte rule: a child whose RLP encoding is
//!     shorter than 32 bytes is embedded inline, otherwise it is replaced by
//!     its keccak256 hash.
//!   * Keys are consumed as nibbles (4-bit units), most significant first.

use crate::rlp::{self, RlpItem};
use tiger_crypto::keccak256;

/// Number of children of a branch node, one per nibble.
const BRANCH_FANOUT: usize = 16;

/// A node of the trie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// The empty node; encodes to the empty byte string.
    Empty,
    /// A leaf: the remaining nibble path and the value it stores.
    Leaf { path: Vec<u8>, value: Vec<u8> },
    /// A shared nibble prefix pointing at a single child.
    Extension { path: Vec<u8>, child: Box<Node> },
    /// A 16-way branch with an optional value for the empty suffix.
    Branch {
        children: Box<[Node; BRANCH_FANOUT]>,
        value: Option<Vec<u8>>,
    },
}

impl Default for Node {
    fn default() -> Self {
        Node::Empty
    }
}

impl Node {
    fn empty_branch() -> Node {
        Node::Branch {
            children: Box::new(std::array::from_fn(|_| Node::Empty)),
            value: None,
        }
    }

    fn is_empty(&self) -> bool {
        matches!(self, Node::Empty)
    }
}

/// Split a byte key into nibbles, high nibble first.
fn bytes_to_nibbles(key: &[u8]) -> Vec<u8> {
    let mut nibbles = Vec::with_capacity(key.len() * 2);
    for &byte in key {
        nibbles.push(byte >> 4);
        nibbles.push(byte & 0x0f);
    }
    nibbles
}

/// Length of the common prefix of two nibble slices.
fn common_prefix_len(a: &[u8], b: &[u8]) -> usize {
    let max = a.len().min(b.len());
    let mut i = 0;
    while i < max && a[i] == b[i] {
        i += 1;
    }
    i
}

/// Compact ("hex-prefix") encoding of a nibble path (Yellow Paper Appendix C).
///
/// The high nibble of the first output byte is a flag: bit 1 marks a leaf and
/// bit 0 marks an odd-length path. Even-length paths get a leading zero nibble
/// so the result is a whole number of bytes.
fn hex_prefix(path: &[u8], leaf: bool) -> Vec<u8> {
    let odd = path.len() % 2 == 1;
    let flag: u8 = if leaf { 2 } else { 0 } + if odd { 1 } else { 0 };

    let mut out = Vec::with_capacity(path.len() / 2 + 1);
    let mut iter = path.iter().copied();

    if odd {
        out.push((flag << 4) | iter.next().unwrap_or(0));
    } else {
        out.push(flag << 4);
    }
    while let Some(hi) = iter.next() {
        let lo = iter.next().unwrap_or(0);
        out.push((hi << 4) | lo);
    }
    out
}

/// Decode a compact-encoded path into `(nibbles, is_leaf)`.
fn decode_hex_prefix(encoded: &[u8]) -> Option<(Vec<u8>, bool)> {
    if encoded.is_empty() {
        return None;
    }
    let flag = encoded[0] >> 4;
    if flag > 3 {
        return None;
    }
    let leaf = flag & 2 != 0;
    let odd = flag & 1 != 0;

    let mut path = Vec::new();
    if odd {
        path.push(encoded[0] & 0x0f);
        for &b in &encoded[1..] {
            path.push(b >> 4);
            path.push(b & 0x0f);
        }
    } else {
        // The first nibble must be zero for an even-length path.
        if encoded[0] & 0x0f != 0 {
            return None;
        }
        for &b in &encoded[1..] {
            path.push(b >> 4);
            path.push(b & 0x0f);
        }
    }
    Some((path, leaf))
}

/// The RLP item for a node, before child-reference substitution.
fn node_to_item(node: &Node) -> RlpItem {
    match node {
        Node::Empty => RlpItem::Bytes(Vec::new()),
        Node::Leaf { path, value } => RlpItem::list(vec![
            RlpItem::Bytes(hex_prefix(path, true)),
            RlpItem::Bytes(value.clone()),
        ]),
        Node::Extension { path, child } => RlpItem::list(vec![
            RlpItem::Bytes(hex_prefix(path, false)),
            node_ref(child),
        ]),
        Node::Branch { children, value } => {
            let mut items: Vec<RlpItem> = children.iter().map(node_ref).collect();
            items.push(match value {
                Some(v) => RlpItem::Bytes(v.clone()),
                None => RlpItem::Bytes(Vec::new()),
            });
            RlpItem::list(items)
        }
    }
}

/// A child reference: inline when the encoding is under 32 bytes, else the
/// keccak256 hash of the encoding.
fn node_ref(node: &Node) -> RlpItem {
    if node.is_empty() {
        return RlpItem::Bytes(Vec::new());
    }
    let item = node_to_item(node);
    let encoded = item.encode();
    if encoded.len() < 32 {
        item
    } else {
        RlpItem::Bytes(keccak256(&encoded).to_vec())
    }
}

/// Collapse single-child branches and merge chained extensions so that every
/// logical trie has exactly one canonical encoding.
fn normalize(node: Node) -> Node {
    match node {
        Node::Branch { children, value } => {
            match &value {
                Some(v) if children.iter().all(|c| c.is_empty()) => {
                    return Node::Leaf {
                        path: Vec::new(),
                        value: v.clone(),
                    };
                }
                None => {
                    let mut only: Option<(usize, Node)> = None;
                    let mut count = 0usize;
                    for (i, child) in children.iter().enumerate() {
                        if !child.is_empty() {
                            count += 1;
                            only = Some((i, child.clone()));
                        }
                    }
                    if count == 1 {
                        let (index, child) = only.expect("count checked");
                        return normalize(merge_into_extension(index as u8, child));
                    }
                }
                _ => {}
            }
            Node::Branch { children, value }
        }
        Node::Extension { path, child } => {
            if child.is_empty() {
                return Node::Empty;
            }
            if path.is_empty() {
                return normalize(*child);
            }
            match *child {
                Node::Leaf { path: cp, value } => {
                    let mut merged = path;
                    merged.extend_from_slice(&cp);
                    Node::Leaf {
                        path: merged,
                        value,
                    }
                }
                Node::Extension {
                    path: cp,
                    child: grandchild,
                } => {
                    let mut merged = path;
                    merged.extend_from_slice(&cp);
                    normalize(Node::Extension {
                        path: merged,
                        child: grandchild,
                    })
                }
                other => Node::Extension {
                    path,
                    child: Box::new(other),
                },
            }
        }
        other => other,
    }
}

/// Wrap a lone branch child (identified by its nibble) as an extension.
fn merge_into_extension(nibble: u8, child: Node) -> Node {
    match child {
        Node::Leaf { path, value } => {
            let mut merged = vec![nibble];
            merged.extend_from_slice(&path);
            Node::Leaf {
                path: merged,
                value,
            }
        }
        Node::Extension { path, child } => {
            let mut merged = vec![nibble];
            merged.extend_from_slice(&path);
            Node::Extension {
                path: merged,
                child,
            }
        }
        other => Node::Extension {
            path: vec![nibble],
            child: Box::new(other),
        },
    }
}

/// Wrap a lone branch under a shared prefix, avoiding an empty extension.
fn wrap(prefix: &[u8], branch: Node) -> Node {
    if prefix.is_empty() {
        branch
    } else {
        Node::Extension {
            path: prefix.to_vec(),
            child: Box::new(branch),
        }
    }
}

fn insert_at(node: Node, nibbles: &[u8], value: Vec<u8>) -> Node {
    match node {
        Node::Empty => Node::Leaf {
            path: nibbles.to_vec(),
            value,
        },

        Node::Leaf { path, value: old } => {
            let cp = common_prefix_len(&path, nibbles);

            if cp == path.len() && cp == nibbles.len() {
                // Identical key: replace the value.
                return Node::Leaf { path, value };
            }

            let mut branch = Node::empty_branch();
            if let Node::Branch {
                children,
                value: slot,
            } = &mut branch
            {
                if cp == path.len() {
                    // The stored path is a proper prefix of the new key, so the
                    // stored value lives at the branch itself.
                    *slot = Some(old);
                    children[nibbles[cp] as usize] = Node::Leaf {
                        path: nibbles[cp + 1..].to_vec(),
                        value,
                    };
                } else if cp == nibbles.len() {
                    // The new key is a proper prefix of the stored path.
                    *slot = Some(value);
                    children[path[cp] as usize] = Node::Leaf {
                        path: path[cp + 1..].to_vec(),
                        value: old,
                    };
                } else {
                    // Divergent suffixes.
                    children[path[cp] as usize] = Node::Leaf {
                        path: path[cp + 1..].to_vec(),
                        value: old,
                    };
                    children[nibbles[cp] as usize] = Node::Leaf {
                        path: nibbles[cp + 1..].to_vec(),
                        value,
                    };
                }
            }
            wrap(&path[..cp], branch)
        }

        Node::Extension { path, child } => {
            let cp = common_prefix_len(&path, nibbles);

            if cp == path.len() {
                // Fully shared prefix: descend.
                return Node::Extension {
                    path,
                    child: Box::new(insert_at(*child, &nibbles[cp..], value)),
                };
            }

            let mut branch = Node::empty_branch();
            if let Node::Branch {
                children,
                value: slot,
            } = &mut branch
            {
                children[path[cp] as usize] = if path.len() == cp + 1 {
                    *child
                } else {
                    Node::Extension {
                        path: path[cp + 1..].to_vec(),
                        child,
                    }
                };

                if cp == nibbles.len() {
                    // The new key ends exactly at the split point.
                    *slot = Some(value);
                } else {
                    children[nibbles[cp] as usize] = Node::Leaf {
                        path: nibbles[cp + 1..].to_vec(),
                        value,
                    };
                }
            }
            wrap(&path[..cp], branch)
        }

        Node::Branch {
            mut children,
            value: old,
        } => {
            if nibbles.is_empty() {
                return Node::Branch {
                    children,
                    value: Some(value),
                };
            }
            let index = nibbles[0] as usize;
            let child = std::mem::replace(&mut children[index], Node::Empty);
            children[index] = insert_at(child, &nibbles[1..], value);
            Node::Branch {
                children,
                value: old,
            }
        }
    }
}

fn get_at(node: &Node, nibbles: &[u8]) -> Option<Vec<u8>> {
    match node {
        Node::Empty => None,
        Node::Leaf { path, value } => (path == nibbles).then(|| value.clone()),
        Node::Extension { path, child } => {
            if nibbles.len() >= path.len() && nibbles[..path.len()] == path[..] {
                get_at(child, &nibbles[path.len()..])
            } else {
                None
            }
        }
        Node::Branch { children, value } => {
            if nibbles.is_empty() {
                value.clone()
            } else {
                get_at(&children[nibbles[0] as usize], &nibbles[1..])
            }
        }
    }
}

fn remove_at(node: Node, nibbles: &[u8]) -> Node {
    match node {
        Node::Empty => Node::Empty,
        Node::Leaf { path, value } => {
            if path == nibbles {
                Node::Empty
            } else {
                Node::Leaf { path, value }
            }
        }
        Node::Extension { path, child } => {
            if nibbles.len() < path.len() || nibbles[..path.len()] != path[..] {
                return Node::Extension { path, child };
            }
            let new_child = remove_at(*child, &nibbles[path.len()..]);
            normalize(Node::Extension {
                path,
                child: Box::new(new_child),
            })
        }
        Node::Branch {
            mut children,
            value,
        } => {
            if nibbles.is_empty() {
                return normalize(Node::Branch {
                    children,
                    value: None,
                });
            }
            let index = nibbles[0] as usize;
            let child = std::mem::replace(&mut children[index], Node::Empty);
            children[index] = remove_at(child, &nibbles[1..]);
            normalize(Node::Branch { children, value })
        }
    }
}

/// Collect every key/value pair in the tree, appending to `out`.
///
/// `path` carries the nibbles consumed so far; a leaf completes a key and a
/// branch value completes the key that ends at the branch.
fn collect_entries(node: &Node, path: &mut Vec<u8>, out: &mut Vec<(Vec<u8>, Vec<u8>)>) {
    match node {
        Node::Empty => {}
        Node::Leaf { path: leaf_path, value } => {
            let mut nibbles = path.clone();
            nibbles.extend_from_slice(leaf_path);
            out.push((nibbles_to_bytes(&nibbles), value.clone()));
        }
        Node::Extension { path: ext_path, child } => {
            path.extend_from_slice(ext_path);
            collect_entries(child, path, out);
            path.truncate(path.len() - ext_path.len());
        }
        Node::Branch { children, value } => {
            if let Some(v) = value {
                out.push((nibbles_to_bytes(path), v.clone()));
            }
            for (index, child) in children.iter().enumerate() {
                path.push(index as u8);
                collect_entries(child, path, out);
                path.pop();
            }
        }
    }
}

/// Pack a nibble sequence back into bytes. Trie keys always have an even
/// nibble count, so the pairing is exact.
fn nibbles_to_bytes(nibbles: &[u8]) -> Vec<u8> {
    nibbles
        .chunks(2)
        .map(|pair| (pair[0] << 4) | pair.get(1).copied().unwrap_or(0))
        .collect()
}

/// Record every node whose encoding is at least 32 bytes, keyed by hash.
/// Decode one RLP-encoded node, resolving child references through `db`.
///
/// A child reference is either inline (the child is encoded inside the parent,
/// when its encoding is under 32 bytes) or a 32-byte hash pointing at another
/// entry in `db`. Both forms are handled, and a hash reference is verified
/// against the bytes it names.
fn decode_node(
    encoded: &[u8],
    db: &std::collections::HashMap<[u8; 32], Vec<u8>>,
) -> Option<Node> {
    let item = crate::rlp::decode(encoded).ok()?;
    let items = match item {
        RlpItem::List(items) => items,
        // A bare byte string is not a node.
        _ => return None,
    };

    match items.len() {
        // Leaf or extension.
        2 => {
            let hp = match &items[0] {
                RlpItem::Bytes(b) => b,
                _ => return None,
            };
            let (path, leaf) = decode_hex_prefix(hp)?;

            if leaf {
                match &items[1] {
                    RlpItem::Bytes(value) => Some(Node::Leaf {
                        path,
                        value: value.clone(),
                    }),
                    _ => None,
                }
            } else {
                let child = decode_child(&items[1], db)?;
                Some(Node::Extension {
                    path,
                    child: Box::new(child),
                })
            }
        }
        // Branch: sixteen children and an optional value.
        17 => {
            let mut children: [Node; BRANCH_FANOUT] = std::array::from_fn(|_| Node::Empty);
            for (index, reference) in items[..16].iter().enumerate() {
                children[index] = decode_child(reference, db)?;
            }

            let value = match &items[16] {
                RlpItem::Bytes(b) if b.is_empty() => None,
                RlpItem::Bytes(b) => Some(b.clone()),
                _ => return None,
            };

            Some(Node::Branch {
                children: Box::new(children),
                value,
            })
        }
        _ => None,
    }
}

/// Resolve a child reference to a node.
fn decode_child(
    reference: &RlpItem,
    db: &std::collections::HashMap<[u8; 32], Vec<u8>>,
) -> Option<Node> {
    match reference {
        // An empty reference is an empty child.
        RlpItem::Bytes(b) if b.is_empty() => Some(Node::Empty),
        // A 32-byte string is a hash of a node held in the database.
        RlpItem::Bytes(b) if b.len() == 32 => {
            let mut hash = [0u8; 32];
            hash.copy_from_slice(b);
            let encoded = db.get(&hash)?;
            if keccak256(encoded) != hash {
                return None;
            }
            decode_node(encoded, db)
        }
        // Anything else is an inline node.
        RlpItem::Bytes(_) => None,
        RlpItem::List(_) => {
            let encoded = reference.encode();
            decode_node(&encoded, db)
        }
    }
}

fn collect_db(node: &Node, db: &mut std::collections::HashMap<[u8; 32], Vec<u8>>) {
    match node {
        Node::Empty | Node::Leaf { .. } => {}
        Node::Extension { child, .. } => collect_db(child, db),
        Node::Branch { children, .. } => {
            for child in children.iter() {
                collect_db(child, db);
            }
        }
    }
    if node.is_empty() {
        return;
    }
    let encoded = node_to_item(node).encode();
    if encoded.len() >= 32 {
        db.insert(keccak256(&encoded), encoded);
    }
}

/// A Merkle Patricia Trie over byte keys.
///
/// The trie is held twice, in two representations that are updated together:
///
///   * `root`, an owned Rust node tree. This is the structural form used for
///     proofs, node collection and key removal, which need to walk nodes.
///   * `fast`, the C++ implementation in `crypto_cpp/trie.cpp` bound through
///     the `tiger-crypto` crate. Root hashing is the consensus hot path — it
///     runs once per account per block, and again for every transaction and
///     receipt root — so it is computed there rather than by walking the Rust
///     tree.
///
/// Both representations are always in agreement, and a test in this module
/// asserts that on every operation, so the duplication cannot drift silently.
///
/// `Clone` copies both, so a clone is independent and equally fast.
pub struct Trie {
    root: Node,
    fast: tiger_crypto::Trie,
}

impl std::fmt::Debug for Trie {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Trie")
            .field("root", &self.root)
            .field("root_hash", &self.root_hash())
            .finish()
    }
}

impl Clone for Trie {
    fn clone(&self) -> Self {
        let mut fast = tiger_crypto::Trie::new();
        for (key, value) in self.entries() {
            fast.insert(&key, &value);
        }
        Self {
            root: self.root.clone(),
            fast,
        }
    }
}

impl Default for Trie {
    fn default() -> Self {
        Self::new()
    }
}

impl Trie {
    /// Create an empty trie.
    pub fn new() -> Self {
        Self {
            root: Node::Empty,
            fast: tiger_crypto::Trie::new(),
        }
    }

    /// Insert or overwrite a key/value pair.
    pub fn insert(&mut self, key: &[u8], value: &[u8]) {
        let nibbles = bytes_to_nibbles(key);
        let root = std::mem::replace(&mut self.root, Node::Empty);
        self.root = normalize(insert_at(root, &nibbles, value.to_vec()));
        self.fast.insert(key, value);
    }

    /// Look up a key.
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        get_at(&self.root, &bytes_to_nibbles(key))
    }

    /// Every key/value pair in byte-key order.
    fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::new();
        collect_entries(&self.root, &mut Vec::new(), &mut out);
        out.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Remove a key, returning true when it was present.
    ///
    /// Removal is rare compared with insertion and lookup, so the fast trie is
    /// rebuilt from the surviving entries rather than kept incrementally. That
    /// keeps the two representations in step without a removal path in C++.
    pub fn delete(&mut self, key: &[u8]) -> bool {
        let nibbles = bytes_to_nibbles(key);
        if get_at(&self.root, &nibbles).is_none() {
            return false;
        }
        let root = std::mem::replace(&mut self.root, Node::Empty);
        self.root = normalize(remove_at(root, &nibbles));

        self.fast = tiger_crypto::Trie::new();
        for (k, v) in self.entries() {
            self.fast.insert(&k, &v);
        }
        true
    }

    /// The keccak256 hash of the RLP-encoded root node.
    ///
    /// The empty trie hashes to
    /// `56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421`.
    pub fn root_hash(&self) -> [u8; 32] {
        self.fast.root_hash()
    }

    /// The RLP encoding of the root node.
    pub fn root_encoding(&self) -> Vec<u8> {
        self.fast.root_encoding()
    }

    /// Rebuild a trie from a persisted node database and a known root hash.
    ///
    /// The nodes are keyed by the keccak-256 of their encoding, so the trie is
    /// reconstructed by resolving each child reference from the root downward.
    /// This is the inverse of [`Trie::node_db`]: what a client wrote to disk can
    /// be read back and queried, which is what makes a restart able to serve
    /// state and proofs.
    ///
    /// Returns `None` when a referenced node is missing or does not hash to the
    /// reference, so a truncated or tampered database is detected rather than
    /// silently producing a different trie.
    pub fn from_root(
        db: &std::collections::HashMap<[u8; 32], Vec<u8>>,
        root: [u8; 32],
    ) -> Option<Self> {
        // The empty trie has no nodes to resolve.
        if root == Trie::new().root_hash() {
            return Some(Trie::new());
        }

        let encoded = db.get(&root)?;
        if keccak256(encoded) != root {
            return None;
        }

        let node = decode_node(encoded, db)?;
        let mut trie = Trie::new();
        trie.root = node;
        trie.rebuild_fast();
        Some(trie)
    }

    /// Rebuild the fast trie from the owned node tree.
    fn rebuild_fast(&mut self) {
        self.fast = tiger_crypto::Trie::new();
        for (key, value) in self.entries() {
            self.fast.insert(&key, &value);
        }
    }

    /// Every node with an encoding of 32 bytes or more, keyed by hash. This is
    /// what a client persists; the root hash alone cannot answer queries.
    pub fn node_db(&self) -> std::collections::HashMap<[u8; 32], Vec<u8>> {
        let mut db = std::collections::HashMap::new();
        collect_db(&self.root, &mut db);
        db
    }

    /// Collect the RLP encoding of every node on the path to `key`, root first.
    pub fn prove(&self, key: &[u8]) -> Vec<Vec<u8>> {
        let nibbles = bytes_to_nibbles(key);
        let mut proof = Vec::new();
        let mut node = &self.root;
        let mut consumed = 0usize;

        loop {
            match node {
                Node::Empty => break,
                Node::Leaf { .. } => {
                    proof.push(node_to_item(node).encode());
                    break;
                }
                Node::Extension { path, child } => {
                    proof.push(node_to_item(node).encode());
                    if nibbles.len() < consumed + path.len()
                        || nibbles[consumed..consumed + path.len()] != path[..]
                    {
                        break;
                    }
                    consumed += path.len();
                    node = child;
                }
                Node::Branch { children, .. } => {
                    proof.push(node_to_item(node).encode());
                    if consumed == nibbles.len() {
                        break;
                    }
                    let index = nibbles[consumed] as usize;
                    consumed += 1;
                    node = &children[index];
                }
            }
        }
        proof
    }

    /// Verify a proof produced by [`Trie::prove`] against a known root.
    ///
    /// Returns `Some(Some(value))` when the proof validly shows the key holds
    /// `value`, `Some(None)` when it validly shows the key is absent, and
    /// `None` when the proof does not link to `root`.
    pub fn verify_proof(
        root: &[u8; 32],
        key: &[u8],
        proof: &[Vec<u8>],
    ) -> Option<Option<Vec<u8>>> {
        if proof.is_empty() {
            return (*root == Trie::new().root_hash()).then_some(None);
        }

        // The first element must be the node whose hash is the root.
        if keccak256(&proof[0]) != *root {
            return None;
        }

        let nibbles = bytes_to_nibbles(key);
        let mut consumed = 0usize;
        let mut expected_hash: Option<[u8; 32]> = None;

        for (index, encoded) in proof.iter().enumerate() {
            if let Some(expected) = expected_hash {
                if keccak256(encoded) != expected {
                    return None;
                }
                expected_hash = None;
            }

            let item = rlp::decode(encoded).ok()?;
            let items = match item {
                RlpItem::List(items) => items,
                RlpItem::Bytes(ref b) if b.is_empty() => return Some(None),
                _ => return None,
            };

            match items.len() {
                // Leaf or extension.
                2 => {
                    let hp = match &items[0] {
                        RlpItem::Bytes(b) => b,
                        _ => return None,
                    };
                    let (path, leaf) = decode_hex_prefix(hp)?;
                    if nibbles.len() < consumed + path.len()
                        || nibbles[consumed..consumed + path.len()] != path[..]
                    {
                        return Some(None);
                    }
                    consumed += path.len();

                    if leaf {
                        if consumed != nibbles.len() {
                            return Some(None);
                        }
                        return match &items[1] {
                            RlpItem::Bytes(b) => Some(Some(b.clone())),
                            _ => None,
                        };
                    }

                    // Extension: resolve the child reference.
                    match resolve_child(&items[1], proof, index)? {
                        ChildRef::Hash(h) => expected_hash = Some(h),
                        ChildRef::Inline => {}
                        ChildRef::Absent => return Some(None),
                    }
                }
                // Branch.
                17 => {
                    if consumed == nibbles.len() {
                        return match &items[16] {
                            RlpItem::Bytes(b) if b.is_empty() => Some(None),
                            RlpItem::Bytes(b) => Some(Some(b.clone())),
                            _ => None,
                        };
                    }
                    let index_nibble = nibbles[consumed] as usize;
                    consumed += 1;
                    match resolve_child(&items[index_nibble], proof, index)? {
                        ChildRef::Hash(h) => expected_hash = Some(h),
                        ChildRef::Inline => {}
                        ChildRef::Absent => return Some(None),
                    }
                }
                _ => return None,
            }
        }

        // Ran out of proof nodes without reaching a leaf: treat as absence.
        Some(None)
    }
}

enum ChildRef {
    Hash([u8; 32]),
    Inline,
    Absent,
}

/// Classify a child reference inside a node and, for inline children, check
/// that the next proof element is structurally identical to it.
fn resolve_child(reference: &RlpItem, proof: &[Vec<u8>], index: usize) -> Option<ChildRef> {
    match reference {
        RlpItem::Bytes(b) if b.is_empty() => Some(ChildRef::Absent),
        RlpItem::Bytes(b) if b.len() == 32 => {
            let mut hash = [0u8; 32];
            hash.copy_from_slice(b);
            Some(ChildRef::Hash(hash))
        }
        RlpItem::Bytes(_) => None,
        RlpItem::List(_) => {
            // Inline child: the very next proof element must be the same node.
            let next = proof.get(index + 1)?;
            let decoded = rlp::decode(next).ok()?;
            if decoded == *reference {
                Some(ChildRef::Inline)
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn a_trie_rebuilt_from_its_node_db_answers_the_same_queries() {
        let mut trie = Trie::new();
        for i in 0..40u8 {
            let mut key = [0u8; 32];
            key[0] = i;
            key[31] = i.wrapping_mul(7);
            trie.insert(&key, format!("value-{i}").as_bytes());
        }

        let db = trie.node_db();
        let root = trie.root_hash();
        let restored = Trie::from_root(&db, root).expect("trie must rebuild from its nodes");

        assert_eq!(restored.root_hash(), root);

        for i in 0..40u8 {
            let mut key = [0u8; 32];
            key[0] = i;
            key[31] = i.wrapping_mul(7);
            assert_eq!(
                restored.get(&key),
                trie.get(&key),
                "restored trie disagrees for key {i}"
            );
        }
    }

    #[test]
    fn a_rebuilt_trie_produces_verifiable_proofs() {
        let mut trie = Trie::new();
        for i in 0..20u8 {
            let mut key = [0u8; 32];
            key[0] = i;
            trie.insert(&key, b"payload");
        }

        let db = trie.node_db();
        let root = trie.root_hash();
        let restored = Trie::from_root(&db, root).unwrap();

        let mut key = [0u8; 32];
        key[0] = 7;
        let proof = restored.prove(&key);
        assert_eq!(
            Trie::verify_proof(&root, &key, &proof),
            Some(Some(b"payload".to_vec()))
        );
    }

    #[test]
    fn rebuilding_with_a_missing_node_is_rejected() {
        let mut trie = Trie::new();
        for i in 0..30u8 {
            let mut key = [0u8; 32];
            key[0] = i;
            trie.insert(&key, b"value");
        }

        let mut db = trie.node_db();
        let root = trie.root_hash();

        // Drop one node the walk needs.
        let victim = db.keys().next().copied().unwrap();
        db.remove(&victim);

        assert!(Trie::from_root(&db, root).is_none());
    }

    #[test]
    fn rebuilding_with_a_tampered_node_is_rejected() {
        let mut trie = Trie::new();
        for i in 0..30u8 {
            let mut key = [0u8; 32];
            key[0] = i;
            trie.insert(&key, b"value");
        }

        let mut db = trie.node_db();
        let root = trie.root_hash();

        // Corrupt the bytes of a node without changing its key.
        let victim = db.keys().next().copied().unwrap();
        let mut bytes = db.remove(&victim).unwrap();
        bytes.push(0xff);
        db.insert(victim, bytes);

        // Either the hash check fails or the node no longer decodes.
        assert!(Trie::from_root(&db, root).is_none());
    }

    #[test]
    fn an_empty_trie_rebuilds_to_an_empty_trie() {
        let trie = Trie::new();
        let db = trie.node_db();
        let restored = Trie::from_root(&db, trie.root_hash()).unwrap();
        assert_eq!(restored.root_hash(), trie.root_hash());
        assert!(restored.get(b"anything").is_none());
    }

    fn to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn empty_trie_has_canonical_root() {
        assert_eq!(
            to_hex(&Trie::new().root_hash()),
            "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
        );
    }

    /// Cross-checked against ethereum/py-trie (HexaryTrie), the authoritative
    /// reference implementation. This is the canonical example from the
    /// Ethereum wiki's trie page.
    #[test]
    fn matches_reference_implementation_on_wiki_example() {
        let mut trie = Trie::new();
        trie.insert(b"do", b"verb");
        trie.insert(b"dog", b"puppy");
        trie.insert(b"doge", b"coin");
        trie.insert(b"horse", b"stallion");
        assert_eq!(
            to_hex(&trie.root_hash()),
            "5991bb8c6514148a29db676a14ac506cd2cd5775ace63c30a4fe457715e9ac84"
        );
    }

    /// A 256-entry keyset exercising branch/extension splits at depth.
    /// Cross-checked against ethereum/py-trie (HexaryTrie).
    #[test]
    fn matches_reference_implementation_on_large_keyset() {
        let mut trie = Trie::new();
        for i in 0..256u32 {
            trie.insert(
                &hex(&format!("{i:064x}")),
                &hex(&format!("{:064x}", i * 7 + 1)),
            );
        }
        assert_eq!(
            to_hex(&trie.root_hash()),
            "5643a50bb43d87e2552644da07ccf5f8f63b657eb244f7a94f67cfde62771fb3"
        );
    }

    #[test]
    fn hex_prefix_encoding_and_decoding() {
        assert_eq!(hex_prefix(&[], false), vec![0x00]);
        assert_eq!(hex_prefix(&[], true), vec![0x20]);
        assert_eq!(hex_prefix(&[0x1], false), vec![0x11]);
        assert_eq!(hex_prefix(&[0x1, 0x2], false), vec![0x00, 0x12]);
        assert_eq!(hex_prefix(&[0x1], true), vec![0x31]);
        assert_eq!(hex_prefix(&[0x1, 0x2], true), vec![0x20, 0x12]);

        for (path, leaf) in [
            (vec![], false),
            (vec![], true),
            (vec![1], true),
            (vec![1, 2, 3], true),
            (vec![15, 0, 7, 8, 9], false),
        ] {
            let encoded = hex_prefix(&path, leaf);
            assert_eq!(decode_hex_prefix(&encoded), Some((path, leaf)));
        }
        // A non-zero low nibble on an even-length path is invalid.
        assert_eq!(decode_hex_prefix(&[0x01, 0x23]), None);
    }

    #[test]
    fn insert_get_delete_round_trip() {
        let mut trie = Trie::new();
        trie.insert(b"do", b"verb");
        trie.insert(b"dog", b"puppy");
        trie.insert(b"doge", b"coin");
        trie.insert(b"horse", b"stallion");

        assert_eq!(trie.get(b"do"), Some(b"verb".to_vec()));
        assert_eq!(trie.get(b"dog"), Some(b"puppy".to_vec()));
        assert_eq!(trie.get(b"doge"), Some(b"coin".to_vec()));
        assert_eq!(trie.get(b"horse"), Some(b"stallion".to_vec()));
        assert_eq!(trie.get(b"cat"), None);

        trie.insert(b"dog", b"canine");
        assert_eq!(trie.get(b"dog"), Some(b"canine".to_vec()));

        assert!(trie.delete(b"doge"));
        assert_eq!(trie.get(b"doge"), None);
        assert!(!trie.delete(b"doge"));
        assert_eq!(trie.get(b"dog"), Some(b"canine".to_vec()));

        assert!(trie.delete(b"do"));
        assert!(trie.delete(b"dog"));
        assert!(trie.delete(b"horse"));
        assert_eq!(trie.root_hash(), Trie::new().root_hash());
    }

    #[test]
    fn root_is_deterministic_and_order_independent() {
        let mut a = Trie::new();
        a.insert(b"do", b"verb");
        a.insert(b"dog", b"puppy");
        a.insert(b"doge", b"coin");
        a.insert(b"horse", b"stallion");

        let mut b = Trie::new();
        b.insert(b"horse", b"stallion");
        b.insert(b"doge", b"coin");
        b.insert(b"dog", b"puppy");
        b.insert(b"do", b"verb");

        assert_eq!(a.root_hash(), b.root_hash());
    }

    #[test]
    fn empty_key_and_empty_value() {
        let mut trie = Trie::new();
        trie.insert(b"", b"empty-key");
        assert_eq!(trie.get(b""), Some(b"empty-key".to_vec()));
        trie.insert(b"a", b"");
        assert_eq!(trie.get(b"a"), Some(Vec::new()));
        assert_ne!(trie.root_hash(), Trie::new().root_hash());
    }

    /// The two representations must agree after every operation. The Rust tree
    /// is the structural form used for proofs; the C++ trie computes the root.
    /// If they ever disagreed, a proof would be verified against a root the
    /// encoding does not produce.
    #[test]
    fn the_rust_tree_and_the_cpp_trie_always_agree() {
        let mut trie = Trie::new();

        let keys: Vec<Vec<u8>> = (0u32..200)
            .map(|i| {
                let mut k = vec![0u8; 4];
                k[0] = (i >> 8) as u8;
                k[1] = i as u8;
                k
            })
            .collect();

        for (n, key) in keys.iter().enumerate() {
            trie.insert(key, &[n as u8; 8]);
            // The C++ root must equal the root derived from the Rust tree's
            // own encoding.
            assert_eq!(
                trie.root_hash(),
                keccak256(&node_to_item(&trie.root).encode()),
                "divergence after inserting {n} keys"
            );
        }

        // Removal is the path that rebuilds the fast trie, so it is the most
        // likely place for the two to drift apart.
        for key in keys.iter().take(100) {
            assert!(trie.delete(key));
            assert_eq!(
                trie.root_hash(),
                keccak256(&node_to_item(&trie.root).encode()),
                "divergence after a removal"
            );
        }

        // Overwriting an existing key must also stay in step.
        for (n, key) in keys.iter().skip(100).enumerate() {
            trie.insert(key, &[n as u8; 16]);
            assert_eq!(
                trie.root_hash(),
                keccak256(&node_to_item(&trie.root).encode()),
                "divergence after an overwrite"
            );
        }

        // And a clone must be independent and still agree.
        let clone = trie.clone();
        assert_eq!(clone.root_hash(), trie.root_hash());
        assert_eq!(clone.get(&keys[150]), trie.get(&keys[150]));
    }

    /// Keys that are prefixes of one another exercise the branch value slot,
    /// which is the case the C++ encoding handles separately.
    #[test]
    fn prefix_keys_agree_between_representations() {
        let mut trie = Trie::new();
        trie.insert(b"a", b"1");
        assert_eq!(trie.root_hash(), keccak256(&node_to_item(&trie.root).encode()));
        trie.insert(b"ab", b"2");
        assert_eq!(trie.root_hash(), keccak256(&node_to_item(&trie.root).encode()));
        trie.insert(b"abc", b"3");
        assert_eq!(trie.root_hash(), keccak256(&node_to_item(&trie.root).encode()));

        // Removing the middle key forces the Rust tree to collapse a branch,
        // and the fast trie to be rebuilt.
        assert!(trie.delete(b"ab"));
        assert_eq!(trie.root_hash(), keccak256(&node_to_item(&trie.root).encode()));
        assert_eq!(trie.get(b"a"), Some(b"1".to_vec()));
        assert_eq!(trie.get(b"ab"), None);
        assert_eq!(trie.get(b"abc"), Some(b"3".to_vec()));
    }


    #[test]
    fn proofs_verify_and_detect_tampering() {
        let mut trie = Trie::new();
        for i in 0..64u32 {
            trie.insert(
                format!("key{i:04}").as_bytes(),
                format!("value{i}").as_bytes(),
            );
        }
        let root = trie.root_hash();

        for i in 0..64u32 {
            let key = format!("key{i:04}");
            let proof = trie.prove(key.as_bytes());
            assert_eq!(
                Trie::verify_proof(&root, key.as_bytes(), &proof),
                Some(Some(format!("value{i}").into_bytes())),
                "proof for {key} must resolve"
            );
        }

        let absent = trie.prove(b"missing");
        assert_eq!(Trie::verify_proof(&root, b"missing", &absent), Some(None));

        let mut proof = trie.prove(b"key0007");
        proof[0][0] ^= 0xff;
        assert_eq!(Trie::verify_proof(&root, b"key0007", &proof), None);

        let empty_root = Trie::new().root_hash();
        assert_eq!(
            Trie::verify_proof(&empty_root, b"key0007", &trie.prove(b"key0007")),
            None
        );
    }

    #[test]
    fn node_db_contains_referenced_nodes() {
        let mut trie = Trie::new();
        for i in 0..32u32 {
            trie.insert(format!("k{i}").as_bytes(), format!("v{i}").as_bytes());
        }
        let db = trie.node_db();
        assert!(!db.is_empty());
        for (hash, encoded) in &db {
            assert!(encoded.len() >= 32);
            assert_eq!(&keccak256(encoded), hash);
        }
        assert!(db.contains_key(&trie.root_hash()));
    }

    #[test]
    fn large_keyset_is_deterministic_and_ordered_independently() {
        let build = |reversed: bool| {
            let mut trie = Trie::new();
            let mut indices: Vec<u32> = (0..256).collect();
            if reversed {
                indices.reverse();
            }
            for i in indices {
                let key = hex(&format!("{:064x}", i));
                let value = hex(&format!("{:064x}", i * 7 + 1));
                trie.insert(&key, &value);
            }
            trie
        };

        let forward = build(false);
        let backward = build(true);
        assert_eq!(forward.root_hash(), backward.root_hash());

        // Every inserted key must be retrievable.
        for i in 0..256u32 {
            let key = hex(&format!("{:064x}", i));
            let expected = hex(&format!("{:064x}", i * 7 + 1));
            assert_eq!(forward.get(&key), Some(expected));
        }

        // A one-bit change to a single value must change the root.
        let mut mutated = forward.clone();
        let key = hex(&format!("{:064x}", 128));
        let mut value = hex(&format!("{:064x}", 128 * 7 + 1));
        value[31] ^= 1;
        mutated.insert(&key, &value);
        assert_ne!(forward.root_hash(), mutated.root_hash());
    }
}
