//! Criterion benchmarks for MerkleTree build performance.
//!
//! Measures build time and throughput for trees with 1 K, 10 K, 100 K, and
//! 1 M leaves, plus proof generation and verification at representative sizes.
//!
//! Run with:
//!   cargo bench -p sky-moon-scope-core
//!
//! HTML reports are written to target/criterion/ when the `html_reports`
//! feature is enabled (enabled by default via Cargo.toml).

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sky_moon_scope_core::merkle_tree::MerkleTree;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_leaves(n: usize) -> Vec<Vec<u8>> {
    (0..n).map(|i| (i as u64).to_le_bytes().to_vec()).collect()
}

fn build_tree(n: usize) -> MerkleTree {
    let leaves = make_leaves(n);
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("build must succeed");
    tree
}

// ── Build benchmarks ──────────────────────────────────────────────────────────

/// Measures total build time for increasingly large datasets.
/// Sizes: 1 K, 10 K, 100 K, 1 M leaves.
fn bench_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("merkle/build");

    // Reduce sample size for the two largest inputs to keep wall-clock time
    // manageable while still producing statistically meaningful results.
    group.sample_size(50);

    for &n in &[1_000usize, 10_000, 100_000] {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("leaves", n), &n, |b, &n| {
            b.iter(|| build_tree(n));
        });
    }

    // 1 M leaves: use a minimal sample count to avoid multi-minute CI runs.
    group.sample_size(10);
    group.throughput(Throughput::Elements(1_000_000));
    group.bench_with_input(
        BenchmarkId::new("leaves", 1_000_000),
        &1_000_000usize,
        |b, &n| {
            b.iter(|| build_tree(n));
        },
    );

    group.finish();
}

// ── Proof benchmarks ──────────────────────────────────────────────────────────

/// Measures proof generation latency for a single leaf at various tree sizes.
fn bench_proof_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("merkle/proof_generation");
    group.sample_size(50);

    for &n in &[1_000usize, 10_000, 100_000] {
        let tree = build_tree(n);
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("tree_size", n), &tree, |b, t| {
            b.iter(|| t.generate_proof(0).unwrap());
        });
    }

    group.finish();
}

/// Measures proof verification latency.
fn bench_proof_verification(c: &mut Criterion) {
    let mut group = c.benchmark_group("merkle/proof_verification");
    group.sample_size(50);

    for &n in &[1_000usize, 10_000, 100_000] {
        let tree = build_tree(n);
        let proof = tree.generate_proof(0).unwrap();
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::new("tree_size", n), &proof, |b, p| {
            b.iter(|| p.verify());
        });
    }

    group.finish();
}

/// Measures end-to-end throughput: build + generate all proofs + verify all.
fn bench_end_to_end(c: &mut Criterion) {
    let mut group = c.benchmark_group("merkle/end_to_end");
    group.sample_size(20);

    for &n in &[1_000usize, 10_000] {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("leaves", n), &n, |b, &n| {
            b.iter(|| {
                let tree = build_tree(n);
                for i in 0..tree.leaf_count() {
                    let proof = tree.generate_proof(i).unwrap();
                    assert!(MerkleTree::verify_proof(&proof, &tree.root));
                }
                tree
            });
        });
    }

    group.finish();
}

// ── Root hex encoding ─────────────────────────────────────────────────────────

/// Measures root hex encoding (should be O(1) but included for completeness).
fn bench_root_hex(c: &mut Criterion) {
    let tree = build_tree(1_000);
    c.bench_function("merkle/root_hex", |b| {
        b.iter(|| tree.get_root_hex());
    });
}

// ── Incremental append benchmarks ─────────────────────────────────────────────

/// Compares full rebuild vs incremental append for growing trees.
fn bench_append_vs_rebuild(c: &mut Criterion) {
    let mut group = c.benchmark_group("merkle/incremental");
    group.sample_size(50);

    let base_sizes = vec![1_000usize, 10_000, 100_000];
    let append_count = 100;

    for &n in &base_sizes {
        // Full rebuild: build from scratch with all leaves
        let all_leaves = make_leaves(n + append_count);
        group.throughput(Throughput::Elements((n + append_count) as u64));
        group.bench_with_input(BenchmarkId::new("full_rebuild", n), &n, |b, _| {
            b.iter(|| {
                let mut tree = MerkleTree::new(32);
                tree.build(all_leaves.clone()).unwrap();
            });
        });

        // Incremental append
        let base_leaves = make_leaves(n);
        let append_leaves: Vec<Vec<u8>> = (n..n + append_count)
            .map(|i| (i as u64).to_le_bytes().to_vec())
            .collect();
        group.bench_with_input(BenchmarkId::new("incremental_append", n), &n, |b, _| {
            b.iter(|| {
                let mut tree = MerkleTree::new(32);
                tree.build(base_leaves.clone()).unwrap();
                tree.append_leaves(append_leaves.clone()).unwrap();
            });
        });
    }

    group.finish();
}

/// Measures single leaf update performance.
fn bench_update_leaf(c: &mut Criterion) {
    let mut group = c.benchmark_group("merkle/update");
    group.sample_size(50);

    for &n in &[1_000usize, 10_000, 100_000] {
        // Full rebuild
        let leaves = make_leaves(n);
        let mut modified_leaves = leaves.clone();
        modified_leaves[0] = vec![0xffu8; 8];
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("full_rebuild_update", n), &n, |b, _| {
            b.iter(|| {
                let mut tree = MerkleTree::new(32);
                tree.build(modified_leaves.clone()).unwrap();
            });
        });

        // Incremental update
        group.bench_with_input(
            BenchmarkId::new("incremental_update_leaf", n),
            &n,
            |b, _| {
                b.iter_batched(
                    || {
                        let mut tree = MerkleTree::new(32);
                        tree.build(leaves.clone()).unwrap();
                        tree
                    },
                    |mut tree| {
                        tree.update_leaf(0, vec![0xffu8; 8]).unwrap();
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ── Registration ──────────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_build,
    bench_proof_generation,
    bench_proof_verification,
    bench_end_to_end,
    bench_root_hex,
    bench_append_vs_rebuild,
    bench_update_leaf,
);
criterion_main!(benches);
