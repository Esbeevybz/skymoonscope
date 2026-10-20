use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a shared profiling result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedResult {
    /// UUID identifier
    pub id: String,
    /// Short URL-friendly share ID (12-16 characters)
    pub share_id: String,
    /// Full profiling result data (JSON)
    pub result_data: String,
    /// Additional metadata (JSON)
    pub metadata: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Expiration timestamp
    pub expires_at: DateTime<Utc>,
    /// Number of times this share has been accessed
    pub access_count: i32,
    /// Optional identifier of who created this share
    pub created_by: Option<String>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

impl SharedResult {
    /// Create a new shared result
    pub fn new(
        share_id: String,
        result_data: String,
        metadata: Option<String>,
        ttl_days: i64,
        created_by: Option<String>,
    ) -> Self {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::days(ttl_days);

        Self {
            id: Uuid::new_v4().to_string(),
            share_id,
            result_data,
            metadata,
            created_at: now,
            expires_at,
            access_count: 0,
            created_by,
            updated_at: now,
        }
    }

    /// Check if this share has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
    }

    /// Increment the access count
    pub fn increment_access(&mut self) {
        self.access_count += 1;
        self.updated_at = Utc::now();
    }

    /// Check if share ID is valid format
    pub fn is_valid_share_id(share_id: &str) -> bool {
        // Share ID should be 12-16 alphanumeric characters (nanoid format)
        share_id.len() >= 12 && share_id.len() <= 16 && share_id.chars().all(|c| c.is_alphanumeric())
    }
}

/// Request to create a new shared result
#[derive(Debug, Deserialize)]
pub struct CreateShareRequest {
    /// The profiling result to share (JSON string or object)
    pub result: serde_json::Value,
    /// Time-to-live in days (default: 30, max: 90)
    #[serde(default = "default_ttl_days")]
    pub ttl_days: i64,
    /// Whether this share is public (default: true)
    #[serde(default = "default_is_public")]
    pub is_public: bool,
}

fn default_ttl_days() -> i64 {
    30
}

fn default_is_public() -> bool {
    true
}

impl CreateShareRequest {
    /// Validate the share request
    pub fn validate(&self) -> Result<(), String> {
        if self.ttl_days < 1 || self.ttl_days > 90 {
            return Err("ttl_days must be between 1 and 90".to_string());
        }
        
        if self.result.is_null() || self.result.is_object() == false {
            return Err("result must be a valid JSON object".to_string());
        }
        
        Ok(())
    }
}

/// Response when a shared result is created
#[derive(Debug, Serialize)]
pub struct CreateShareResponse {
    /// The short share ID
    pub share_id: String,
    /// The full share URL
    pub share_url: String,
    /// When this share will expire
    pub expires_at: DateTime<Utc>,
}

impl CreateShareResponse {
    pub fn new(share_id: String, base_url: &str, expires_at: DateTime<Utc>) -> Self {
        let share_url = format!("{}/share/{}", base_url.trim_end_matches('/'), share_id);
        Self {
            share_id,
            share_url,
            expires_at,
        }
    }
}

/// Response when retrieving a shared result
#[derive(Debug, Serialize)]
pub struct ViewShareResponse {
    /// The share ID
    pub share_id: String,
    /// The profiling result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Metadata about the share
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// When this share was created
    pub created_at: DateTime<Utc>,
    /// Current access count
    pub access_count: i32,
}

impl ViewShareResponse {
    pub fn from_shared_result(shared: SharedResult) -> Result<Self, serde_json::Error> {
        Ok(Self {
            share_id: shared.share_id,
            result: Some(serde_json::from_str(&shared.result_data)?),
            metadata: shared.metadata.and_then(|m| serde_json::from_str(&m).ok()),
            created_at: shared.created_at,
            access_count: shared.access_count,
        })
    }
}

/// Response for checking if a share exists
#[derive(Debug, Serialize)]
pub struct ShareExistsResponse {
    /// Whether the share exists and is valid
    pub exists: bool,
    /// When it expires (if exists)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_id_validation() {
        assert!(SharedResult::is_valid_share_id("abc123xyz789"));
        assert!(SharedResult::is_valid_share_id("123456789012345"));
        assert!(SharedResult::is_valid_share_id("a".repeat(12).as_str()));
        assert!(SharedResult::is_valid_share_id("z".repeat(16).as_str()));
        
        assert!(!SharedResult::is_valid_share_id("abc123")); // Too short
        assert!(!SharedResult::is_valid_share_id("abc123xyz789_extra")); // Too long
        assert!(!SharedResult::is_valid_share_id("abc-123-xyz")); // Invalid chars
    }

    #[test]
    fn test_expiration() {
        let shared = SharedResult::new(
            "abc123xyz789".to_string(),
            "{}".to_string(),
            None,
            0, // Expire immediately
            None,
        );
        assert!(shared.is_expired());
    }

    #[test]
    fn test_create_share_validation() {
        let valid_request = CreateShareRequest {
            result: serde_json::json!({"data": "test"}),
            ttl_days: 30,
            is_public: true,
        };
        assert!(valid_request.validate().is_ok());

        let invalid_ttl = CreateShareRequest {
            result: serde_json::json!({"data": "test"}),
            ttl_days: 120,
            is_public: true,
        };
        assert!(invalid_ttl.validate().is_err());

        let invalid_result = CreateShareRequest {
            result: serde_json::Value::Null,
            ttl_days: 30,
            is_public: true,
        };
        assert!(invalid_result.validate().is_err());
    }
}
