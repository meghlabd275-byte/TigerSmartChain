//! Criterion benchmarks for the pruning engine.
//!
//! Exercises the real `PruningEngine` state tracker: populating the tracker,
//! estimating state size, and running each pruning strategy over a realistic
//! number of accounts and storage slots.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tiger_pruning::{
    AccountState, PruningConfig, PruningEngine, PruningStrategy, StorageState,
};

fn account(address: u32, last_accessed: u64) -> AccountState {
    AccountState {
        address: address.to_le_bytes().to_vec(),
        nonce: 1,
        balance: vec![0, 100],
        code_hash: vec![0; 32],
        storage_root: vec![0; 32],
        last_accessed,
        last_accessed_block: last_accessed,
        access_count: 1,
    }
}

fn storage(address: u32, slot: u32) -> StorageState {
    StorageState {
        address: address.to_le_bytes().to_vec(),
        slot: slot.to_le_bytes().to_vec(),
        value: vec![1],
        last_accessed: 0,
    }
}

fn populated_engine(strategy: PruningStrategy, accounts: u32, slots_per_account: u32) -> PruningEngine {
    let config = PruningConfig {
        strategy,
        ..Default::default()
    };
    let engine = PruningEngine::new(config);
    engine.start().unwrap();

    for i in 0..accounts {
        // Half of the accounts are "old" (last_accessed = 0), half are recent.
        let last_accessed = if i % 2 == 0 { 0 } else { u64::MAX / 2 };
        engine.track_account(i.to_le_bytes().to_vec(), account(i, last_accessed));
        for s in 0..slots_per_account {
            engine.track_storage(
                i.to_le_bytes().to_vec(),
                s.to_le_bytes().to_vec(),
                storage(i, s),
            );
        }
    }

    engine
}

fn bench_track_account(c: &mut Criterion) {
    c.bench_function("track_account", |b| {
        let engine = PruningEngine::new(PruningConfig::default());
        let mut i = 0u32;
        b.iter(|| {
            engine.track_account(black_box(i.to_le_bytes().to_vec()), account(i, 0));
            i = i.wrapping_add(1);
        });
    });
}

fn bench_estimate_state_size(c: &mut Criterion) {
    let engine = populated_engine(
        PruningStrategy::BlockBased { blocks_to_keep: 100 },
        10_000,
        4,
    );

    c.bench_function("estimate_state_size", |b| {
        b.iter(|| black_box(engine.estimate_state_size()));
    });
}

fn bench_prune_block_based(c: &mut Criterion) {
    c.bench_function("prune_block_based_10k_accounts", |b| {
        b.iter_batched(
            || populated_engine(PruningStrategy::BlockBased { blocks_to_keep: 100 }, 10_000, 4),
            |engine| {
                // cutoff = current_block - 100; old accounts (last_accessed 0)
                // are removed, recent ones (u64::MAX/2) are retained.
                black_box(engine.prune(200, 1_000).unwrap());
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

fn bench_prune_memory_based(c: &mut Criterion) {
    c.bench_function("prune_memory_based_10k_accounts", |b| {
        b.iter_batched(
            || populated_engine(PruningStrategy::MemoryBased { max_memory_mb: 1 }, 10_000, 4),
            |engine| {
                black_box(engine.prune(0, 0).unwrap());
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

fn bench_prune_combined(c: &mut Criterion) {
    c.bench_function("prune_combined_10k_accounts", |b| {
        b.iter_batched(
            || {
                populated_engine(
                    PruningStrategy::Combined {
                        epochs_to_keep: 1,
                        blocks_to_keep: 100,
                        max_age_seconds: 1,
                        max_memory_mb: 1024,
                    },
                    10_000,
                    4,
                )
            },
            |engine| {
                black_box(engine.prune(1_000_000, 1_000_000).unwrap());
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

criterion_group!(
    benches,
    bench_track_account,
    bench_estimate_state_size,
    bench_prune_block_based,
    bench_prune_memory_based,
    bench_prune_combined,
);
criterion_main!(benches);
