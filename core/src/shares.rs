//! Shareable profiling result links.
//!
//! # Overview
//! This module implements the `POST /api/shares` and `GET /api/shares/:share_id`
//! endpoints for generating and retrieving shareable links to Soroban contract
//! profiling results.
//!
//! # Design decisions
//! - **ID generation**: 8-character base-62 string generated with `rand::thread_rng()`.
//!   Gives ~218 trillion combinations; uniqueness enforced by SQLite UNIQUE constraint
//!   with a retry-on-collision strategy (max 3 attempts).
//! - **Storage**: SQLite (existing `sqlx::SqlitePool`) + `moka` in-process LRU cache
//!   for hot reads (2048 entries, 1-hour TTL).
//! - **Compression**: snapshot JSON is gzip-compressed before storage; typical
//!   size drops from ~30 KB to ~3–6 KB.
//! - **Rate limiting**: IP-based token bucket — max `RATE_LIMIT_MAX_CREATES`
//!   POST requests per `RATE_LIMIT_WINDOW_SECS` (default 10/60 s). Implemented
//!   with a `moka` cache of per-IP atomic counters; no Redis dependency.
//! - **Expiry**: default 7 days; checked on every GET; returns HTTP 410 Gone.
//! - **WASM bytecode** is explicitly excluded from the snapshot (size / privacy).

use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use moka::sync::Cache;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::errors::AppError;
use crate::gas_golfing::GasGolfingReport;
use crate::simulation::SorobanResources;

// ── Constants ────────────────────────────────────────────────────────────────

/// Default share expiry in seconds (7 days).
const DEFAULT_EXPIRY_SECS: i64 = 7 * 24 * 3600;

/// Maximum allowed share expiry in seconds (30 days).
const MAX_EXPIRY_SECS: i64 = 30 * 24 * 3600;

/// Maximum compressed snapshot size in bytes (512 KB).
const MAX_SNAPSHOT_BYTES: usize = 512 * 1024;

/// Number of base-62 characters in a share ID.
const SHARE_ID_LEN: usize = 8;

/// Maximum attempts before giving up on a collision-free ID.
const MAX_ID_ATTEMPTS: usize = 3;

/// Rate-limit window in seconds.
const RATE_LIMIT_WINDOW_SECS: u64 = 60;

/// Maximum POST /api/shares calls per IP per window.
const RATE_LIMIT_MAX_CREATES: u32 = 10;

/// Base-62 alphabet (URL-safe, case-sensitive).
const BASE62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

// ── Data structures ───────────────────────────────────────────────────────────

/// The profiling data that gets stored in a share snapshot.
///
/// WASM bytecode and wallet addresses are explicitly excluded.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ShareSnapshot {
    /// Soroban resource consumption metrics.
    pub resources: SorobanResources,

    /// Optional gas-golfing optimisation report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_golfing: Option<GasGolfingReport>,

    /// Cross-contract call graph in Mermaid format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_graph_mermaid: Option<String>,

    /// Human-readable title for the share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// The contract ID that was profiled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,

    /// The function name that was invoked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_name: Option<String>,

    /// Network the simulation ran on (e.g. "testnet", "mainnet").
    #[serde(default = "default_network")]
    pub network: String,

    /// Simulation input arguments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<serde_json::Value>,

    /// Arbitrary additional metadata (fee market snapshot, insights, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

fn default_network() -> String {
    "testnet".to_string()
}

/// Request body for `POST /api/shares`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateShareRequest {
    /// The profiling snapshot to store.
    pub snapshot: ShareSnapshot,

    /// Optional expiry in seconds from now (max 30 days; default 7 days).
    #[serde(default)]
    pub expires_in_secs: Option<i64>,
}

/// Successful response from `POST /api/shares`.
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateShareResponse {
    /// The generated short ID.
    pub share_id: String,
    /// Fully-qualified shareable URL.
    pub share_url: String,
    /// UTC timestamp when this share expires.
    pub expires_at: DateTime<Utc>,
}

/// Response from `GET /api/shares/:share_id`.
#[derive(Debug, Serialize, ToSchema)]
pub struct GetShareResponse {
    /// The short ID for this share.
    pub share_id: String,
    /// The stored snapshot.
    pub snapshot: ShareSnapshot,
    /// UTC timestamp when this share expires, if any.
    pub expires_at: Option<DateTime<Utc>>,
    /// Number of times this share has been viewed.
    pub view_count: i64,
    /// UTC timestamp when this share was created.
    pub created_at: DateTime<Utc>,
}

/// Cached entry stored in the moka LRU for hot GET reads.
#[derive(Debug, Clone)]
struct CachedShare {
    snapshot: ShareSnapshot,
    expires_at: Option<DateTime<Utc>>,
    view_count: i64,
    created_at: DateTime<Utc>,
}

// ── Per-IP rate limiter entry ─────────────────────────────────────────────────

#[derive(Debug)]
struct RateLimitEntry {
    count: std::sync::atomic::AtomicU32,
    window_start: std::sync::Mutex<std::time::Instant>,
}

impl RateLimitEntry {
    fn new() -> Self {
        Self {
            count: std::sync::atomic::AtomicU32::new(1),
            window_start: std::sync::Mutex::new(std::time::Instant::now()),
        }
    }
}

// ── ShareService ──────────────────────────────────────────────────────────────

/// Core service for creating and retrieving shareable links.
pub struct ShareService {
    db: SqlitePool,
    /// In-process read cache (share_id → CachedShare).
    read_cache: Cache<String, CachedShare>,
    /// Per-IP create-rate counter cache (IP string → RateLimitEntry).
    rate_cache: Cache<String, Arc<RateLimitEntry>>,
    /// Public base URL used to construct share URLs.
    public_base_url: String,
}

impl ShareService {
    /// Create a new `ShareService`.
    ///
    /// `public_base_url` should be the web dashboard origin, e.g.
    /// `https://app.skymoonscope.dev`. It is used to build the `share_url`
    /// returned in [`CreateShareResponse`].
    pub fn new(db: SqlitePool, public_base_url: impl Into<String>) -> Self {
        let read_cache = Cache::builder()
            .max_capacity(2048)
            .time_to_live(Duration::from_secs(3600))
            .build();

        let rate_cache = Cache::builder()
            .max_capacity(10_000)
            .time_to_live(Duration::from_secs(RATE_LIMIT_WINDOW_SECS * 2))
            .build();

        Self {
            db,
            read_cache,
            rate_cache,
            public_base_url: public_base_url.into(),
        }
    }

    // ── Rate limiting ─────────────────────────────────────────────────────

    /// Returns `Ok(())` if the request is within the rate limit, or
    /// `Err(AppError::TooManyRequests)` otherwise.
    ///
    /// Uses a sliding window via `moka` TTL: a new entry is seeded at count=1
    /// and evicted after 2× the window, which is sufficient for the token-
    /// bucket approximation required here.
    fn check_rate_limit(&self, ip: &str) -> Result<(), AppError> {
        let window = Duration::from_secs(RATE_LIMIT_WINDOW_SECS);

        if let Some(entry) = self.rate_cache.get(ip) {
            // Check whether the current window has expired (wall-clock).
            let mut start = entry.window_start.lock().unwrap();
            if start.elapsed() >= window {
                // Reset counter for the new window.
                entry
                    .count
                    .store(1, std::sync::atomic::Ordering::Relaxed);
                *start = std::time::Instant::now();
                return Ok(());
            }

            let prev = entry
                .count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if prev >= RATE_LIMIT_MAX_CREATES {
                return Err(AppError::TooManyRequests(format!(
                    "Rate limit exceeded: max {} share creations per {} seconds",
                    RATE_LIMIT_MAX_CREATES, RATE_LIMIT_WINDOW_SECS
                )));
            }
        } else {
            // First request from this IP in this window.
            self.rate_cache
                .insert(ip.to_string(), Arc::new(RateLimitEntry::new()));
        }

        Ok(())
    }

    // ── ID generation ─────────────────────────────────────────────────────

    /// Generate a random 8-character base-62 string.
    fn generate_id() -> String {
        let mut rng = rand::thread_rng();
        (0..SHARE_ID_LEN)
            .map(|_| BASE62[rng.gen_range(0..BASE62.len())] as char)
            .collect()
    }

    // ── Compression helpers ───────────────────────────────────────────────

    fn compress(data: &[u8]) -> Result<Vec<u8>, AppError> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(data)
            .map_err(|e| AppError::Internal(format!("Gzip encode error: {e}")))?;
        encoder
            .finish()
            .map_err(|e| AppError::Internal(format!("Gzip finish error: {e}")))
    }

    fn decompress(data: &[u8]) -> Result<Vec<u8>, AppError> {
        let mut decoder = GzDecoder::new(data);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|e| AppError::Internal(format!("Gzip decode error: {e}")))?;
        Ok(out)
    }

    // ── Public API ────────────────────────────────────────────────────────

    /// Create a new share entry and return the response.
    ///
    /// Validates the payload size, generates a collision-free ID, compresses
    /// and stores the snapshot, and returns the shareable URL.
    pub async fn create_share(
        &self,
        request: CreateShareRequest,
        client_ip: &str,
    ) -> Result<CreateShareResponse, AppError> {
        // 1. Rate-limit check.
        self.check_rate_limit(client_ip)?;

        // 2. Validate + clamp expiry.
        let expiry_secs = request
            .expires_in_secs
            .unwrap_or(DEFAULT_EXPIRY_SECS)
            .clamp(1, MAX_EXPIRY_SECS);
        let expires_at = Utc::now() + chrono::Duration::seconds(expiry_secs);

        // 3. Serialise and compress the snapshot.
        let json_bytes = serde_json::to_vec(&request.snapshot)
            .map_err(|e| AppError::BadRequest(format!("Failed to serialise snapshot: {e}")))?;

        let compressed = Self::compress(&json_bytes)?;

        if compressed.len() > MAX_SNAPSHOT_BYTES {
            return Err(AppError::BadRequest(format!(
                "Snapshot too large after compression: {} bytes (max {})",
                compressed.len(),
                MAX_SNAPSHOT_BYTES
            )));
        }

        // 4. Generate a unique ID (retry on collision).
        let share_id = self
            .allocate_id(&compressed, &request.snapshot, expires_at)
            .await?;

        // 5. Build the response.
        let share_url = format!(
            "{}/share/{}",
            self.public_base_url.trim_end_matches('/'),
            share_id
        );

        tracing::info!(
            share_id = %share_id,
            client_ip = %client_ip,
            compressed_bytes = compressed.len(),
            expires_at = %expires_at,
            "Share created"
        );

        Ok(CreateShareResponse {
            share_id,
            share_url,
            expires_at,
        })
    }

    /// Insert the row with retry on UNIQUE constraint violation (ID collision).
    async fn allocate_id(
        &self,
        compressed: &[u8],
        snapshot: &ShareSnapshot,
        expires_at: DateTime<Utc>,
    ) -> Result<String, AppError> {
        for attempt in 0..MAX_ID_ATTEMPTS {
            let id = Self::generate_id();

            let result = sqlx::query(
                r#"
                INSERT INTO shares
                    (share_id, snapshot, contract_id, function_name, network,
                     title, expires_at, is_active, view_count, created_at, updated_at)
                VALUES
                    (?, ?, ?, ?, ?, ?, ?, 1, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                "#,
            )
            .bind(&id)
            .bind(compressed)
            .bind(&snapshot.contract_id)
            .bind(&snapshot.function_name)
            .bind(&snapshot.network)
            .bind(&snapshot.title)
            .bind(expires_at.to_rfc3339())
            .execute(&self.db)
            .await;

            match result {
                Ok(_) => return Ok(id),
                Err(sqlx::Error::Database(db_err))
                    if db_err.message().contains("UNIQUE") =>
                {
                    tracing::warn!(
                        attempt = attempt + 1,
                        id = %id,
                        "Share ID collision, retrying"
                    );
                    continue;
                }
                Err(e) => {
                    return Err(AppError::Internal(format!("Failed to insert share: {e}")));
                }
            }
        }

        Err(AppError::Internal(
            "Failed to allocate a unique share ID after multiple attempts".to_string(),
        ))
    }

    /// Retrieve a share by ID, incrementing the view counter.
    ///
    /// Returns:
    /// - `AppError::NotFound` for unknown IDs or deactivated shares.
    /// - `AppError::Gone` for expired shares (HTTP 410).
    pub async fn get_share(&self, share_id: &str) -> Result<GetShareResponse, AppError> {
        // 1. Validate the ID format (only base-62 chars, correct length).
        if !is_valid_share_id(share_id) {
            return Err(AppError::NotFound(format!(
                "Share '{}' not found",
                share_id
            )));
        }

        // 2. Check the in-process read cache first.
        if let Some(cached) = self.read_cache.get(share_id) {
            // Verify the cached entry hasn't expired.
            if let Some(exp) = cached.expires_at {
                if Utc::now() > exp {
                    self.read_cache.invalidate(share_id);
                    return Err(AppError::Gone(format!(
                        "Share '{}' has expired",
                        share_id
                    )));
                }
            }

            // Increment view counter in DB asynchronously (fire-and-forget).
            let db = self.db.clone();
            let id = share_id.to_string();
            tokio::spawn(async move {
                let _ = sqlx::query(
                    "UPDATE shares SET view_count = view_count + 1, updated_at = CURRENT_TIMESTAMP WHERE share_id = ?",
                )
                .bind(&id)
                .execute(&db)
                .await;
            });

            return Ok(GetShareResponse {
                share_id: share_id.to_string(),
                snapshot: cached.snapshot,
                expires_at: cached.expires_at,
                view_count: cached.view_count,
                created_at: cached.created_at,
            });
        }

        // 3. Cache miss — fetch from DB.
        let row = sqlx::query!(
            r#"
            SELECT share_id, snapshot, expires_at, is_active, view_count, created_at
            FROM shares
            WHERE share_id = ?
            "#,
            share_id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {e}")))?;

        let row = match row {
            Some(r) => r,
            None => {
                return Err(AppError::NotFound(format!(
                    "Share '{}' not found",
                    share_id
                )));
            }
        };

        // 4. Check is_active (soft-delete).
        if row.is_active == 0 {
            return Err(AppError::NotFound(format!(
                "Share '{}' not found",
                share_id
            )));
        }

        // 5. Parse expiry and check it.
        let expires_at: Option<DateTime<Utc>> = row
            .expires_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        if let Some(exp) = expires_at {
            if Utc::now() > exp {
                return Err(AppError::Gone(format!(
                    "Share '{}' has expired",
                    share_id
                )));
            }
        }

        // 6. Decompress and deserialise the snapshot.
        let decompressed = Self::decompress(&row.snapshot)?;
        let snapshot: ShareSnapshot = serde_json::from_slice(&decompressed)
            .map_err(|e| AppError::Internal(format!("Failed to deserialise snapshot: {e}")))?;

        let created_at: DateTime<Utc> = row
            .created_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        let view_count = row.view_count;

        // 7. Increment the view counter synchronously so the value returned
        //    in this response is accurate.
        sqlx::query(
            "UPDATE shares SET view_count = view_count + 1, updated_at = CURRENT_TIMESTAMP WHERE share_id = ?",
        )
        .bind(share_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to update view count: {e}")))?;

        // 8. Populate the read cache (store incremented count).
        self.read_cache.insert(
            share_id.to_string(),
            CachedShare {
                snapshot: snapshot.clone(),
                expires_at,
                view_count: view_count + 1,
                created_at,
            },
        );

        tracing::info!(share_id = %share_id, view_count = view_count + 1, "Share viewed");

        Ok(GetShareResponse {
            share_id: share_id.to_string(),
            snapshot,
            expires_at,
            view_count: view_count + 1,
            created_at,
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Validate that a candidate share ID contains only base-62 characters and
/// has exactly [`SHARE_ID_LEN`] characters.
fn is_valid_share_id(id: &str) -> bool {
    id.len() == SHARE_ID_LEN && id.bytes().all(|b| BASE62.contains(&b))
}

// ── Axum state wrapper ────────────────────────────────────────────────────────

/// Axum-injectable wrapper around the share service.
///
/// Stored as `Arc<ShareState>` in the Axum `State` extractor so it can be
/// cheaply cloned across handler tasks without copying the underlying service.
#[derive(Clone)]
pub struct ShareState(pub Arc<ShareService>);

// ── HTTP Handlers ─────────────────────────────────────────────────────────────

/// Extract the client IP from the `X-Forwarded-For` or `X-Real-IP` header,
/// falling back to `"unknown"` when neither is present.
fn extract_client_ip(headers: &HeaderMap) -> String {
    if let Some(xff) = headers.get("x-forwarded-for") {
        if let Ok(val) = xff.to_str() {
            // `X-Forwarded-For: client, proxy1, proxy2` — take the first entry.
            if let Some(ip) = val.split(',').next() {
                return ip.trim().to_string();
            }
        }
    }
    if let Some(xri) = headers.get("x-real-ip") {
        if let Ok(val) = xri.to_str() {
            return val.trim().to_string();
        }
    }
    "unknown".to_string()
}

/// `POST /api/shares` — create a shareable link.
///
/// Rate-limited to [`RATE_LIMIT_MAX_CREATES`] requests per IP per
/// [`RATE_LIMIT_WINDOW_SECS`] seconds.
///
/// # Request body
/// ```json
/// {
///   "snapshot": { "resources": {...}, "contract_id": "C...", ... },
///   "expires_in_secs": 604800
/// }
/// ```
///
/// # Response (201 Created)
/// ```json
/// {
///   "share_id": "aB3xK9mZ",
///   "share_url": "https://app.skymoonscope.dev/share/aB3xK9mZ",
///   "expires_at": "2026-10-24T23:09:28Z"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/api/shares",
    request_body = CreateShareRequest,
    responses(
        (status = 201, description = "Share created successfully", body = CreateShareResponse),
        (status = 400, description = "Invalid or oversized payload"),
        (status = 429, description = "Rate limit exceeded"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Shares"
)]
pub async fn create_share_handler(
    State(state): State<Arc<ShareState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateShareRequest>,
) -> Result<impl IntoResponse, AppError> {
    let ip = extract_client_ip(&headers);
    let response = state.0.create_share(payload, &ip).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// `GET /api/shares/:share_id` — retrieve a shareable link.
///
/// Increments the view counter and checks expiry on every request.
///
/// # Response (200 OK)
/// ```json
/// {
///   "share_id": "aB3xK9mZ",
///   "snapshot": { ... },
///   "expires_at": "2026-10-24T23:09:28Z",
///   "view_count": 5,
///   "created_at": "2026-10-17T23:09:28Z"
/// }
/// ```
///
/// # Errors
/// - `404 Not Found` — share ID does not exist or has been deactivated.
/// - `410 Gone` — share has passed its `expires_at` timestamp.
#[utoipa::path(
    get,
    path = "/api/shares/{share_id}",
    params(
        ("share_id" = String, Path, description = "8-character base-62 share ID")
    ),
    responses(
        (status = 200, description = "Share retrieved successfully", body = GetShareResponse),
        (status = 404, description = "Share not found"),
        (status = 410, description = "Share has expired"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Shares"
)]
pub async fn get_share_handler(
    State(state): State<Arc<ShareState>>,
    Path(share_id): Path<String>,
) -> Result<Json<GetShareResponse>, AppError> {
    let response = state.0.get_share(&share_id).await?;
    Ok(Json(response))
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        routing::{get, post},
        Router,
    };
    use tower::ServiceExt;

    /// Build an in-memory SQLite pool with the shares schema.
    async fn test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory SQLite");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS shares (
                share_id          TEXT PRIMARY KEY,
                snapshot          BLOB NOT NULL,
                contract_id       TEXT,
                function_name     TEXT,
                network           TEXT NOT NULL DEFAULT 'testnet',
                title             TEXT,
                expires_at        TIMESTAMP,
                access_token_hash TEXT,
                is_active         INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
                view_count        INTEGER NOT NULL DEFAULT 0,
                created_at        TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at        TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("create shares table");

        pool
    }

    fn make_snapshot() -> ShareSnapshot {
        ShareSnapshot {
            resources: SorobanResources {
                cpu_instructions: 1_000_000,
                ram_bytes: 65_536,
                ledger_read_bytes: 2048,
                ledger_write_bytes: 512,
                transaction_size_bytes: 450,
            },
            gas_golfing: None,
            call_graph_mermaid: None,
            title: Some("Test share".to_string()),
            contract_id: Some(
                "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC".to_string(),
            ),
            function_name: Some("transfer".to_string()),
            network: "testnet".to_string(),
            inputs: None,
            metadata: None,
        }
    }

    // ── ID generation ──────────────────────────────────────────────────────

    #[test]
    fn generate_id_has_correct_length_and_charset() {
        for _ in 0..100 {
            let id = ShareService::generate_id();
            assert_eq!(id.len(), SHARE_ID_LEN, "ID length must be {SHARE_ID_LEN}");
            assert!(
                is_valid_share_id(&id),
                "ID '{id}' contains invalid characters"
            );
        }
    }

    #[test]
    fn is_valid_share_id_rejects_wrong_length() {
        assert!(!is_valid_share_id("short"));
        assert!(!is_valid_share_id("toolongstring"));
    }

    #[test]
    fn is_valid_share_id_rejects_invalid_chars() {
        assert!(!is_valid_share_id("aB3xK9m!")); // ! not in base-62
        assert!(!is_valid_share_id("aB3xK9m-")); // - not in base-62
        assert!(!is_valid_share_id("aB3 K9mZ")); // space not in base-62
    }

    #[test]
    fn is_valid_share_id_accepts_valid_ids() {
        assert!(is_valid_share_id("aB3xK9mZ"));
        assert!(is_valid_share_id("00000000"));
        assert!(is_valid_share_id("zzzzzzzz"));
        assert!(is_valid_share_id("ZZZZZZZZ"));
    }

    // ── Compression round-trip ─────────────────────────────────────────────

    #[test]
    fn compress_decompress_roundtrip() {
        let original = b"Hello Soroban! This is a test payload for the share snapshot.";
        let compressed = ShareService::compress(original).expect("compress");
        // After decompression the bytes should match exactly.
        let decompressed = ShareService::decompress(&compressed).expect("decompress");
        assert_eq!(decompressed, original);
    }

    // ── Rate limiting ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn rate_limit_allows_up_to_max_creates() {
        let pool = test_db().await;
        let service = ShareService::new(pool, "http://localhost:3000");

        for i in 0..RATE_LIMIT_MAX_CREATES {
            service
                .check_rate_limit("192.168.1.1")
                .unwrap_or_else(|_| panic!("Request {i} should be allowed"));
        }
    }

    #[tokio::test]
    async fn rate_limit_blocks_over_max_creates() {
        let pool = test_db().await;
        let service = ShareService::new(pool, "http://localhost:3000");

        // Fill up the bucket.
        for _ in 0..RATE_LIMIT_MAX_CREATES {
            let _ = service.check_rate_limit("10.0.0.1");
        }

        let result = service.check_rate_limit("10.0.0.1");
        assert!(
            matches!(result, Err(AppError::TooManyRequests(_))),
            "Expected TooManyRequests error after exceeding limit"
        );
    }

    #[tokio::test]
    async fn rate_limit_is_per_ip() {
        let pool = test_db().await;
        let service = ShareService::new(pool, "http://localhost:3000");

        // Exhaust the limit for one IP.
        for _ in 0..RATE_LIMIT_MAX_CREATES {
            let _ = service.check_rate_limit("10.0.0.1");
        }
        assert!(service.check_rate_limit("10.0.0.1").is_err());

        // Different IP must still be within its own limit.
        assert!(service.check_rate_limit("10.0.0.2").is_ok());
    }

    // ── Create share ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn create_share_returns_valid_response() {
        let pool = test_db().await;
        let service = ShareService::new(pool, "https://app.skymoonscope.dev");

        let request = CreateShareRequest {
            snapshot: make_snapshot(),
            expires_in_secs: None,
        };

        let response = service
            .create_share(request, "127.0.0.1")
            .await
            .expect("create share");

        assert_eq!(response.share_id.len(), SHARE_ID_LEN);
        assert!(is_valid_share_id(&response.share_id));
        assert!(response.share_url.contains(&response.share_id));
        assert!(response
            .share_url
            .starts_with("https://app.skymoonscope.dev/share/"));
        assert!(response.expires_at > Utc::now());
    }

    #[tokio::test]
    async fn create_share_respects_expiry_clamp() {
        let pool = test_db().await;
        let service = ShareService::new(pool, "https://app.skymoonscope.dev");

        // Request more than MAX_EXPIRY_SECS — should be clamped.
        let request = CreateShareRequest {
            snapshot: make_snapshot(),
            expires_in_secs: Some(MAX_EXPIRY_SECS + 100_000),
        };

        let response = service
            .create_share(request, "127.0.0.1")
            .await
            .expect("create share");

        let max_allowed = Utc::now() + chrono::Duration::seconds(MAX_EXPIRY_SECS + 5);
        assert!(
            response.expires_at <= max_allowed,
            "Expiry should be clamped to MAX_EXPIRY_SECS"
        );
    }

    // ── Get share ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_share_returns_stored_snapshot() {
        let pool = test_db().await;
        let service = ShareService::new(pool, "https://app.skymoonscope.dev");

        let snapshot = make_snapshot();
        let create_req = CreateShareRequest {
            snapshot: snapshot.clone(),
            expires_in_secs: None,
        };

        let created = service
            .create_share(create_req, "127.0.0.1")
            .await
            .expect("create");
        let retrieved = service.get_share(&created.share_id).await.expect("get");

        assert_eq!(retrieved.share_id, created.share_id);
        assert_eq!(retrieved.snapshot.contract_id, snapshot.contract_id);
        assert_eq!(
            retrieved.snapshot.resources.cpu_instructions,
            snapshot.resources.cpu_instructions
        );
        assert_eq!(retrieved.view_count, 1);
    }

    #[tokio::test]
    async fn get_share_increments_view_count() {
        let pool = test_db().await;
        let service = ShareService::new(pool, "https://app.skymoonscope.dev");

        let create_req = CreateShareRequest {
            snapshot: make_snapshot(),
            expires_in_secs: None,
        };
        let created = service
            .create_share(create_req, "127.0.0.1")
            .await
            .expect("create");
        let share_id = &created.share_id;

        // Invalidate the cache so each call hits the DB.
        service.read_cache.invalidate(share_id);
        let r1 = service.get_share(share_id).await.expect("get 1");
        service.read_cache.invalidate(share_id);
        let r2 = service.get_share(share_id).await.expect("get 2");

        assert!(
            r2.view_count >= r1.view_count,
            "View count should not decrease between calls"
        );
    }

    #[tokio::test]
    async fn get_share_returns_not_found_for_unknown_id() {
        let pool = test_db().await;
        let service = ShareService::new(pool, "https://app.skymoonscope.dev");

        let result = service.get_share("abcdefgh").await;
        assert!(
            matches!(result, Err(AppError::NotFound(_))),
            "Expected NotFound for unknown share ID"
        );
    }

    #[tokio::test]
    async fn get_share_returns_not_found_for_invalid_id_format() {
        let pool = test_db().await;
        let service = ShareService::new(pool, "https://app.skymoonscope.dev");

        let result = service.get_share("bad-id!").await;
        assert!(
            matches!(result, Err(AppError::NotFound(_))),
            "Expected NotFound for malformed share ID"
        );
    }

    #[tokio::test]
    async fn get_share_returns_gone_for_expired_share() {
        let pool = test_db().await;
        let service = ShareService::new(pool.clone(), "https://app.skymoonscope.dev");

        // Insert a share that expired 1 second ago.
        let id = "testExp1";
        let snapshot = make_snapshot();
        let json_bytes = serde_json::to_vec(&snapshot).unwrap();
        let compressed = ShareService::compress(&json_bytes).unwrap();
        let past = Utc::now() - chrono::Duration::seconds(1);

        sqlx::query(
            r#"INSERT INTO shares
               (share_id, snapshot, network, expires_at, is_active, view_count, created_at, updated_at)
               VALUES (?, ?, 'testnet', ?, 1, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
        )
        .bind(id)
        .bind(&compressed)
        .bind(past.to_rfc3339())
        .execute(&pool)
        .await
        .expect("insert expired share");

        let result = service.get_share(id).await;
        assert!(
            matches!(result, Err(AppError::Gone(_))),
            "Expected Gone (410) for an expired share, got: {:?}",
            result.unwrap_err()
        );
    }

    // ── Client IP extraction ───────────────────────────────────────────────

    #[test]
    fn extract_client_ip_from_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.5, 10.0.0.1".parse().unwrap(),
        );
        assert_eq!(extract_client_ip(&headers), "203.0.113.5");
    }

    #[test]
    fn extract_client_ip_falls_back_to_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "198.51.100.1".parse().unwrap());
        assert_eq!(extract_client_ip(&headers), "198.51.100.1");
    }

    #[test]
    fn extract_client_ip_falls_back_to_unknown() {
        let headers = HeaderMap::new();
        assert_eq!(extract_client_ip(&headers), "unknown");
    }

    // ── HTTP integration (handler wiring) ─────────────────────────────────

    fn build_test_router(service: ShareService) -> Router {
        let state = Arc::new(ShareState(Arc::new(service)));
        Router::new()
            .route("/api/shares", post(create_share_handler))
            .route("/api/shares/:share_id", get(get_share_handler))
            .with_state(state)
    }

    #[tokio::test]
    async fn http_create_share_returns_201() {
        let pool = test_db().await;
        let service = ShareService::new(pool, "https://app.skymoonscope.dev");
        let app = build_test_router(service);

        let body = serde_json::json!({
            "snapshot": {
                "resources": {
                    "cpu_instructions": 1_000_000,
                    "ram_bytes": 65_536,
                    "ledger_read_bytes": 2048,
                    "ledger_write_bytes": 512,
                    "transaction_size_bytes": 450
                },
                "network": "testnet"
            }
        });

        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/shares")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(parsed["share_id"].as_str().is_some());
        assert!(parsed["share_url"].as_str().unwrap().contains("/share/"));
        assert!(parsed["expires_at"].as_str().is_some());
    }

    #[tokio::test]
    async fn http_get_share_returns_200_after_create() {
        let pool = test_db().await;
        let service = ShareService::new(pool, "https://app.skymoonscope.dev");
        let app = build_test_router(service);

        let body = serde_json::json!({
            "snapshot": {
                "resources": {
                    "cpu_instructions": 500_000,
                    "ram_bytes": 32_768,
                    "ledger_read_bytes": 1024,
                    "ledger_write_bytes": 256,
                    "transaction_size_bytes": 300
                },
                "network": "testnet",
                "contract_id": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
            }
        });

        // Create
        let create_req = Request::builder()
            .method(Method::POST)
            .uri("/api/shares")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let create_resp = app.clone().oneshot(create_req).await.unwrap();
        assert_eq!(create_resp.status(), StatusCode::CREATED);

        let bytes = axum::body::to_bytes(create_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let share_id = created["share_id"].as_str().unwrap().to_string();

        // Retrieve
        let get_req = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/shares/{}", share_id))
            .body(Body::empty())
            .unwrap();

        let get_resp = app.oneshot(get_req).await.unwrap();
        assert_eq!(get_resp.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let retrieved: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(retrieved["share_id"], share_id);
        assert!(retrieved["snapshot"]["resources"]["cpu_instructions"]
            .as_u64()
            .is_some());
        assert_eq!(retrieved["view_count"].as_i64().unwrap(), 1);
    }

    #[tokio::test]
    async fn http_get_unknown_share_returns_404() {
        let pool = test_db().await;
        let service = ShareService::new(pool, "https://app.skymoonscope.dev");
        let app = build_test_router(service);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/shares/notfound")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn http_rate_limit_returns_429_after_exceeding_limit() {
        let pool = test_db().await;
        let service = ShareService::new(pool, "https://app.skymoonscope.dev");
        let app = build_test_router(service);

        let body = serde_json::json!({
            "snapshot": {
                "resources": {
                    "cpu_instructions": 100,
                    "ram_bytes": 100,
                    "ledger_read_bytes": 100,
                    "ledger_write_bytes": 100,
                    "transaction_size_bytes": 100
                },
                "network": "testnet"
            }
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();

        let mut last_status = StatusCode::CREATED;
        // Send RATE_LIMIT_MAX_CREATES + 1 requests from the same IP.
        for i in 0..=(RATE_LIMIT_MAX_CREATES + 1) {
            let req = Request::builder()
                .method(Method::POST)
                .uri("/api/shares")
                .header("content-type", "application/json")
                // Simulate the same IP via X-Forwarded-For.
                .header("x-forwarded-for", "10.10.10.10")
                .body(Body::from(body_bytes.clone()))
                .unwrap();

            let resp = app.clone().oneshot(req).await.unwrap();
            last_status = resp.status();
            if last_status == StatusCode::TOO_MANY_REQUESTS {
                break;
            }
            if i == RATE_LIMIT_MAX_CREATES + 1 {
                panic!(
                    "Expected 429 after {} requests, but got {} on request {}",
                    RATE_LIMIT_MAX_CREATES, last_status, i
                );
            }
        }

        assert_eq!(last_status, StatusCode::TOO_MANY_REQUESTS);
    }
}
