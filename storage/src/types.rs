//! Storage Types

// =============================================================================
// STORAGE
// =============================================================================

/// Storage Interface
pub trait Storage {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>>;
    fn put(&mut self, key: Vec<u8>, value: Vec<u8>);
    fn delete(&mut self, key: &[u8]) -> bool;
}