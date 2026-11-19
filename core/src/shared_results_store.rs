use crate::shared_results_domain::SharedResult;
use chrono::Utc;
use nanoid::nanoid;
use sqlx::{Row, Sqlite, SqlitePool};
use tracing::{error, warn, info};

/// Error type for shared results store operations
#[derive(Debug)]
pub enum SharedResultsStoreError {
    DatabaseError(String),
    NotFound,
    Expired,
    InvalidRequest(String),
}

impl std::fmt::Display for SharedResultsStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            Self::NotFound => write!(f, "Share not found"),
            Self::Expired => write!(f, "Share has expired"),
            Self::InvalidRequest(msg) => write!(f, "Invalid request: {}", msg),
        }
    }
}

impl std::error::Error for SharedResultsStoreError {}

/// Store for managing shared results
pub struct SharedResultsStore {
    pool: SqlitePool,
}

impl SharedResultsStore {
    /// Create a new shared results store
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create a new shared result
    pub async fn create(&self, mut shared: SharedResult) -> Result<SharedResult, SharedResultsStoreError> {
        let now = Utc::now();
        
        let query = r#"
            INSERT INTO shared_results (
                id, share_id, result_data, metadata, created_at, expires_at,
                access_count, created_by, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        sqlx::query(query)
            .bind(&shared.id)
            .bind(&shared.share_id)
            .bind(&shared.result_data)
            .bind(&shared.metadata)
            .bind(shared.created_at.to_rfc3339())
            .bind(shared.expires_at.to_rfc3339())
            .bind(shared.access_count)
            .bind(&shared.created_by)
            .bind(now.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to insert shared result: {}", e);
                SharedResultsStoreError::DatabaseError(e.to_string())
            })?;

        info!("Created shared result: {}", shared.share_id);
        Ok(shared)
    }

    /// Get a shared result by share_id
    pub async fn get_by_share_id(&self, share_id: &str) -> Result<SharedResult, SharedResultsStoreError> {
        let query = r#"
            SELECT id, share_id, result_data, metadata, created_at, expires_at,
                   access_count, created_by, updated_at
            FROM shared_results
            WHERE share_id = ?
        "#;

        let row = sqlx::query(query)
            .bind(share_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to query shared result: {}", e);
                SharedResultsStoreError::DatabaseError(e.to_string())
            })?
            .ok_or(SharedResultsStoreError::NotFound)?;

        let shared = self.row_to_shared_result(row)?;
        
        if shared.is_expired() {
            warn!("Attempted to access expired share: {}", share_id);
            return Err(SharedResultsStoreError::Expired);
        }

        Ok(shared)
    }

    /// Increment the access count for a shared result
    pub async fn increment_access(&self, share_id: &str) -> Result<(), SharedResultsStoreError> {
        let query = r#"
            UPDATE shared_results
            SET access_count = access_count + 1, updated_at = ?
            WHERE share_id = ?
        "#;

        sqlx::query(query)
            .bind(Utc::now().to_rfc3339())
            .bind(share_id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to increment access count: {}", e);
                SharedResultsStoreError::DatabaseError(e.to_string())
            })?;

        Ok(())
    }

    /// Check if a share exists and is valid
    pub async fn exists(&self, share_id: &str) -> Result<bool, SharedResultsStoreError> {
        let query = r#"
            SELECT 1 FROM shared_results
            WHERE share_id = ? AND expires_at > ?
        "#;

        let exists = sqlx::query(query)
            .bind(share_id)
            .bind(Utc::now().to_rfc3339())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to check share existence: {}", e);
                SharedResultsStoreError::DatabaseError(e.to_string())
            })?
            .is_some();

        Ok(exists)
    }

    /// Get expiry time for a share (if exists)
    pub async fn get_expiry(&self, share_id: &str) -> Result<Option<chrono::DateTime<Utc>>, SharedResultsStoreError> {
        let query = r#"
            SELECT expires_at FROM shared_results
            WHERE share_id = ?
        "#;

        let row = sqlx::query(query)
            .bind(share_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to query share expiry: {}", e);
                SharedResultsStoreError::DatabaseError(e.to_string())
            })?;

        if let Some(row) = row {
            let expires_at_str: String = row.get("expires_at");
            let expires_at = chrono::DateTime::parse_from_rfc3339(&expires_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| {
                    error!("Failed to parse datetime: {}", e);
                    SharedResultsStoreError::DatabaseError(e.to_string())
                })?;
            Ok(Some(expires_at))
        } else {
            Ok(None)
        }
    }

    /// Delete an expired shared result
    pub async fn delete(&self, share_id: &str) -> Result<(), SharedResultsStoreError> {
        let query = "DELETE FROM shared_results WHERE share_id = ?";

        sqlx::query(query)
            .bind(share_id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to delete shared result: {}", e);
                SharedResultsStoreError::DatabaseError(e.to_string())
            })?;

        info!("Deleted shared result: {}", share_id);
        Ok(())
    }

    /// Delete all expired shares (cleanup job)
    pub async fn delete_expired(&self) -> Result<u64, SharedResultsStoreError> {
        let query = r#"
            DELETE FROM shared_results
            WHERE expires_at <= ?
        "#;

        let result = sqlx::query(query)
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to delete expired shares: {}", e);
                SharedResultsStoreError::DatabaseError(e.to_string())
            })?;

        let deleted_count = result.rows_affected();
        info!("Cleaned up {} expired shares", deleted_count);
        Ok(deleted_count)
    }

    /// Get all shares created by a user (pagination support)
    pub async fn get_by_creator(
        &self,
        created_by: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SharedResult>, SharedResultsStoreError> {
        let query = r#"
            SELECT id, share_id, result_data, metadata, created_at, expires_at,
                   access_count, created_by, updated_at
            FROM shared_results
            WHERE created_by = ?
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
        "#;

        let rows = sqlx::query(query)
            .bind(created_by)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to query shares by creator: {}", e);
                SharedResultsStoreError::DatabaseError(e.to_string())
            })?;

        rows.into_iter().map(|row| self.row_to_shared_result(row)).collect()
    }

    /// Helper to convert database row to SharedResult
    fn row_to_shared_result(&self, row: sqlx::sqlite::SqliteRow) -> Result<SharedResult, SharedResultsStoreError> {
        let created_at_str: String = row.get("created_at");
        let expires_at_str: String = row.get("expires_at");

        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                error!("Failed to parse created_at: {}", e);
                SharedResultsStoreError::DatabaseError(e.to_string())
            })?;

        let expires_at = chrono::DateTime::parse_from_rfc3339(&expires_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                error!("Failed to parse expires_at: {}", e);
                SharedResultsStoreError::DatabaseError(e.to_string())
            })?;

        Ok(SharedResult {
            id: row.get("id"),
            share_id: row.get("share_id"),
            result_data: row.get("result_data"),
            metadata: row.get("metadata"),
            created_at,
            expires_at,
            access_count: row.get("access_count"),
            created_by: row.get("created_by"),
            updated_at: {
                let updated_at_str: String = row.get("updated_at");
                chrono::DateTime::parse_from_rfc3339(&updated_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| {
                        error!("Failed to parse updated_at: {}", e);
                        SharedResultsStoreError::DatabaseError(e.to_string())
                    })?
            },
        })
    }
}

/// Generate a unique share ID
pub fn generate_share_id() -> String {
    nanoid!(12, &nanoid::alphabet::ALPHANUMERIC)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_share_id() {
        let id = generate_share_id();
        assert_eq!(id.len(), 12);
        assert!(id.chars().all(|c| c.is_alphanumeric()));
    }
}
