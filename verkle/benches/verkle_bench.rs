//! Benchmarks for insertion, lookup and proof generation.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tiger_verkle::VerkleTree;

fn key(n: u64) -> Vec<u8> {
    let mut k = vec![0u8; 32];
    k[..8].copy_from_slice(&n.to_be_bytes());
    k
}

fn bench_insert(c: &mut Criterion) {
    c.bench_function("insert_1000", |b| {
        b.iter(|| {
            let mut tree = VerkleTree::new();
            for i in 0..1000u64 {
                tree.insert(&key(i), b"value").unwrap();
            }
            black_box(tree.root().map(|r| r.len()));
        })
    });
}

fn bench_get(c: &mut Criterion) {
    let mut tree = VerkleTree::new();
    for i in 0..1000u64 {
        tree.insert(&key(i), b"value").unwrap();
    }

    c.bench_function("get_1000", |b| {
        b.iter(|| {
            for i in 0..1000u64 {
                black_box(tree.get(&key(i)).unwrap());
            }
        })
    });
}

fn bench_prove(c: &mut Criterion) {
    let mut tree = VerkleTree::new();
    for i in 0..1000u64 {
        tree.insert(&key(i), b"value").unwrap();
    }
    let root = tree.root().unwrap().to_vec();

    c.bench_function("prove_and_verify_1000", |b| {
        b.iter(|| {
            for i in 0..1000u64 {
                let proof = tree.prove(&key(i)).unwrap();
                assert!(VerkleTree::verify_proof(&proof, &root).unwrap());
            }
        })
    });
}

criterion_group!(benches, bench_insert, bench_get, bench_prove);
criterion_main!(benches);
