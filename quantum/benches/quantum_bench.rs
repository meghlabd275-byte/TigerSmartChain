//! Throughput benchmarks for the post-quantum primitives.

use criterion::{criterion_group, criterion_main, Criterion};
use tiger_quantum::QuantumEngine;

fn bench_sign(c: &mut Criterion) {
    let mut engine = QuantumEngine::new();
    let keypair = engine.generate_keypair().expect("keygen");
    let message = b"benchmark message for ML-DSA";

    c.bench_function("ml_dsa_sign", |b| {
        b.iter(|| engine.sign(message, &keypair).expect("sign"));
    });
}

fn bench_verify(c: &mut Criterion) {
    let mut engine = QuantumEngine::new();
    let keypair = engine.generate_keypair().expect("keygen");
    let message = b"benchmark message for ML-DSA";
    let signature = engine.sign(message, &keypair).expect("sign");

    c.bench_function("ml_dsa_verify", |b| {
        b.iter(|| {
            engine
                .verify(message, &signature, &keypair.public_key)
                .expect("verify")
        });
    });
}

fn bench_kem(c: &mut Criterion) {
    let engine = QuantumEngine::new();
    let keypair = engine.generate_kem_keypair().expect("keygen");
    let encapsulated = engine
        .encapsulate(&keypair.public_key)
        .expect("encapsulate");

    c.bench_function("ml_kem_decapsulate", |b| {
        b.iter(|| {
            engine
                .decapsulate(&keypair.secret_key, &encapsulated.ciphertext)
                .expect("decapsulate")
        });
    });
}

fn bench_keccak(c: &mut Criterion) {
    let engine = QuantumEngine::new();
    let payload = vec![0xabu8; 4096];

    c.bench_function("keccak256_4k", |b| {
        b.iter(|| engine.keccak256(&payload));
    });
}

criterion_group!(benches, bench_sign, bench_verify, bench_kem, bench_keccak);
criterion_main!(benches);
