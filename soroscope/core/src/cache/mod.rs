pub mod disk;

pub use disk::{DiskCache, DiskCacheConfig};

use crate::simulation::SimulationResult;
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sled::{Db, Tree};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

const CACHE_TTL_SECS: u64 = 3_600;
const CACHE_MAX_CAPACITY: u64 = 1_000;
const DEFAULT_MAX_CACHE_SIZE_MB: u64 = 100;

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry<T> {
    data: T,
    ledger_sequence: u64,
    timestamp: u64,
}

fn current_timestamp_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub struct SimulationCache {
    l1: Cache<String, SimulationResult>,
    l2: Tree,
    max_cache_size_bytes: u64,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl SimulationCache {
    pub fn new(db: &Db) -> Arc<Self> {
        Self::new_with_max_cache_size_mb(db, DEFAULT_MAX_CACHE_SIZE_MB)
    }

    pub fn new_with_max_cache_size_mb(db: &Db, max_cache_size_mb: u64) -> Arc<Self> {
        Self::new_with_max_cache_size_bytes(
            db,
            max_cache_size_mb.saturating_mul(1024 * 1024),
        )
    }

    pub fn new_with_max_cache_size_bytes(db: &Db, max_cache_size_bytes: u64) -> Arc<Self> {
        let l1 = Cache::builder()
            .max_capacity(CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(CACHE_TTL_SECS))
            .build();

        let l2 = db
            .open_tree("simulation_results")
            .expect("Failed to open simulation_results tree");

        Arc::new(Self {
            l1,
            l2,
            max_cache_size_bytes,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        })
    }

    pub fn generate_key(contract_id: &str, function_name: &str, args: &[String]) -> String {
        let args_json = serde_json::to_string(args).unwrap_or_else(|_| "[]".to_string());
        let input = format!("{}{}{}", contract_id, function_name, args_json);
        let digest = Sha256::digest(input.as_bytes());
        hex::encode(digest)
    }

    pub async fn get(&self, key: &str) -> Option<SimulationResult> {
        if let Some(result) = self.l1.get(key).await {
            self.touch(key);
            self.hits.fetch_add(1, Ordering::Relaxed);
            tracing::debug!(cache.key = %key, "Cache HIT (L1)");
            return Some(result);
        }

        if let Ok(Some(bytes)) = self.l2.get(key) {
            if let Ok(entry) = serde_json::from_slice::<CacheEntry<SimulationResult>>(&bytes) {
                self.touch(key);
                self.l1.insert(key.to_string(), entry.data.clone()).await;
                self.hits.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(cache.key = %key, "Cache HIT (L2)");
                return Some(entry.data);
            }
        }

        self.misses.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(cache.key = %key, "Cache MISS");
        None
    }

    pub async fn set(&self, key: String, result: SimulationResult) {
        let entry = CacheEntry {
            ledger_sequence: result.latest_ledger,
            timestamp: current_timestamp_millis(),
            data: result.clone(),
        };

        if let Ok(bytes) = serde_json::to_vec(&entry) {
            if self.l2.insert(&key, bytes).is_ok() {
                self.evict_to_size();
            }
        }
        self.l1.insert(key, result).await;
    }

    fn touch(&self, key: &str) {
        let Some(bytes) = self.l2.get(key).ok().flatten() else {
            return;
        };
        let Ok(mut entry) = serde_json::from_slice::<CacheEntry<SimulationResult>>(&bytes) else {
            return;
        };
        entry.timestamp = current_timestamp_millis();
        if let Ok(updated) = serde_json::to_vec(&entry) {
            let _ = self.l2.insert(key, updated);
        }
    }

    fn evict_to_size(&self) {
        if self.max_cache_size_bytes == 0 {
            return;
        }

        let mut entries = Vec::new();
        let mut total_size = 0u64;
        for item in self.l2.iter() {
            let Ok((key, value)) = item else {
                continue;
            };
            let Ok(entry) = serde_json::from_slice::<CacheEntry<SimulationResult>>(value.as_ref()) else {
                continue;
            };
            total_size = total_size.saturating_add(key.len() as u64 + value.len() as u64);
            entries.push((entry.timestamp, key, value.len() as u64));
        }

        if total_size <= self.max_cache_size_bytes {
            return;
        }

        entries.sort_by_key(|(timestamp, _, _)| *timestamp);
        let mut removed = 0u64;
        for (_, key, value_size) in entries {
            if total_size <= self.max_cache_size_bytes {
                break;
            }
            if self.l2.remove(&key).is_ok() {
                total_size = total_size.saturating_sub(key.len() as u64 + value_size);
                removed += 1;
            }
        }
        if removed > 0 {
            tracing::debug!(removed, "evicted least-recently-used simulation cache entries");
        }
    }

    pub fn log_stats(&self) {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate_pct = hits
            .checked_mul(100)
            .and_then(|v| v.checked_div(total))
            .unwrap_or(0);
        tracing::info!(
            cache.hits = hits,
            cache.misses = misses,
            cache.total = total,
            cache.hit_rate_pct = hit_rate_pct,
            "Cache statistics"
        );
    }

    pub fn hit_count(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    pub fn miss_count(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }
}

pub struct ContractCache {
    wasm_tree: Tree,
    ledger_tree: Tree,
}

impl ContractCache {
    pub fn new(db: &Db) -> Self {
        let wasm_tree = db
            .open_tree("wasm_bytes")
            .expect("Failed to open wasm_bytes tree");
        let ledger_tree = db
            .open_tree("ledger_entries")
            .expect("Failed to open ledger_entries tree");
        Self {
            wasm_tree,
            ledger_tree,
        }
    }

    pub fn get_wasm(&self, hash_hex: &str) -> Option<Vec<u8>> {
        self.wasm_tree
            .get(hash_hex)
            .ok()
            .flatten()
            .map(|v| v.to_vec())
    }

    pub fn set_wasm(&self, hash_hex: String, wasm_bytes: Vec<u8>) {
        let _ = self.wasm_tree.insert(hash_hex, wasm_bytes);
    }

    pub fn get_ledger_entry(&self, key_64: &str, current_ledger: u64) -> Option<Vec<u8>> {
        if let Ok(Some(bytes)) = self.ledger_tree.get(key_64) {
            if let Ok(entry) = serde_json::from_slice::<CacheEntry<Vec<u8>>>(&bytes) {
                if entry.ledger_sequence >= current_ledger {
                    return Some(entry.data);
                }
            }
        }
        None
    }

    pub fn set_ledger_entry(&self, key_64: String, entry_bytes: Vec<u8>, ledger_sequence: u64) {
        let entry = CacheEntry {
            data: entry_bytes,
            ledger_sequence,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        if let Ok(bytes) = serde_json::to_vec(&entry) {
            let _ = self.ledger_tree.insert(key_64, bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::SorobanResources;
    use tempfile::tempdir;

    fn result() -> SimulationResult {
        SimulationResult {
            resources: SorobanResources::default(),
            transaction_hash: None,
            latest_ledger: 1,
            cost_stroops: 0,
            state_dependency: None,
            ttl_analysis: None,
            transaction_data: "test".to_string(),
            call_graph: None,
            state_snapshot: None,
            protocol_version: 0,
        }
    }

    #[tokio::test]
    async fn evicts_least_recently_accessed_entry_when_size_is_exceeded() {
        let dir = tempdir().unwrap();
        let db = sled::open(dir.path()).unwrap();
        let unlimited = SimulationCache::new_with_max_cache_size_bytes(&db, 0);
        unlimited.set("old".to_string(), result()).await;
        assert!(unlimited.get("old").await.is_some());

        let tree = db.open_tree("simulation_results").unwrap();
        let old_size = tree.get("old").unwrap().unwrap().len() as u64 + 3;
        let bounded = SimulationCache::new_with_max_cache_size_bytes(&db, old_size);
        bounded.set("new".to_string(), result()).await;

        assert!(bounded.get("old").await.is_none());
        assert!(bounded.get("new").await.is_some());
    }
}

/// Errors surfaced by the cache subsystem.
///
/// These bubble up from Sled's disk store, JSON (de)serialisation, and
/// I/O when opening a backing directory. The main service converts them
/// into HTTP 500 via the `AppError` layer; callers inside the cache path
/// normally treat L2 errors as misses and log-and-continue rather than
/// failing the whole simulation.
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("disk cache I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("disk cache backend error: {0}")]
    Backend(#[from] sled::Error),

    #[error("cache payload (de)serialisation error: {0}")]
    Serialization(#[from] serde_json::Error),
}
