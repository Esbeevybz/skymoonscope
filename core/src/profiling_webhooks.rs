//! Webhook delivery for profiling result notifications.
//!
//! This module extends the profiling endpoints to support asynchronous webhook
//! notifications when profiling completes. Similar to the job system webhooks,
//! but tailored for synchronous profiling operations.

use crate::jobs::domain::WebhookConfig;
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, warn};

type HmacSha256 = Hmac<Sha256>;

/// Header name for webhook signatures.
pub const WEBHOOK_SIGNATURE_HEADER: &str = "x-skymoonscoope-signature";

/// Errors that can occur during webhook operations.
#[derive(Debug, Error)]
pub enum WebhookError {
    #[error("Invalid webhook configuration: {0}")]
    InvalidConfig(String),

    #[error("Webhook delivery failed: {0}")]
    DeliveryFailed(String),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Configuration for profiling webhook delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileWebhookConfig {
    /// Base webhook configuration (URL, headers, secret).
    pub config: WebhookConfig,
    /// Timeout for webhook delivery in seconds (default: 30).
    #[serde(default = "default_webhook_timeout_secs")]
    pub timeout_secs: u64,
    /// Maximum number of delivery retries (default: 3).
    #[serde(default = "default_webhook_max_retries")]
    pub max_retries: u32,
}

fn default_webhook_timeout_secs() -> u64 {
    30
}

fn default_webhook_max_retries() -> u32 {
    3
}

impl ProfileWebhookConfig {
    /// Validate the webhook configuration.
    pub fn validate(&self) -> Result<(), WebhookError> {
        if self.config.callback_url.is_empty() {
            return Err(WebhookError::InvalidConfig(
                "callback_url cannot be empty".to_string(),
            ));
        }

        // Validate URL format
        if !self.config.callback_url.starts_with("http://")
            && !self.config.callback_url.starts_with("https://")
        {
            return Err(WebhookError::InvalidConfig(
                "callback_url must start with http:// or https://".to_string(),
            ));
        }

        if self.timeout_secs == 0 {
            return Err(WebhookError::InvalidConfig(
                "timeout_secs must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Payload sent to webhook endpoints for profiling results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilingWebhookPayload {
    /// Unique identifier for the profiling request (optional).
    pub request_id: Option<String>,
    /// Name of the profiled function.
    pub function_name: String,
    /// The profiling result data.
    pub result: Value,
    /// Delivery timestamp.
    pub timestamp: String,
}

/// Sign a webhook payload with HMAC-SHA256.
pub fn sign_webhook_payload(secret: &str, payload: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("failed to create HMAC-SHA256 from secret");
    mac.update(payload);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// Asynchronously deliver a profiling result webhook.
pub async fn deliver_profiling_webhook(
    client: &Client,
    webhook_config: &ProfileWebhookConfig,
    payload: ProfilingWebhookPayload,
) -> Result<(), WebhookError> {
    webhook_config.validate()?;

    let body = serde_json::to_vec(&payload)?;
    let timeout = Duration::from_secs(webhook_config.timeout_secs);

    for attempt in 1..=webhook_config.max_retries {
        let mut request = client
            .post(&webhook_config.config.callback_url)
            .header("Content-Type", "application/json")
            .body(body.clone())
            .timeout(timeout);

        // Add signature if secret is configured.
        if let Some(ref secret) = webhook_config.config.secret {
            let sig = sign_webhook_payload(secret, &body);
            request = request.header(WEBHOOK_SIGNATURE_HEADER, sig);
        }

        // Add custom headers if provided.
        if let Some(headers) = &webhook_config.config.headers {
            for (key, value) in headers {
                request = request.header(key, value);
            }
        }

        match request.send().await {
            Ok(response) => {
                if response.status().is_success() {
                    debug!(
                        attempt,
                        url = %webhook_config.config.callback_url,
                        "Profiling webhook delivered successfully"
                    );
                    return Ok(());
                } else {
                    let status = response.status();
                    warn!(
                        attempt,
                        status = %status,
                        url = %webhook_config.config.callback_url,
                        "Webhook returned non-success status"
                    );

                    // Don't retry on client errors (4xx).
                    if status.is_client_error() {
                        return Err(WebhookError::DeliveryFailed(format!(
                            "HTTP {}: {}",
                            status.as_u16(),
                            response.text().await.unwrap_or_default()
                        )));
                    }
                }
            }
            Err(e) => {
                warn!(attempt, error = %e, "Webhook delivery attempt failed");
            }
        }

        // Retry with exponential backoff (2^attempt - 1 seconds).
        if attempt < webhook_config.max_retries {
            let backoff_secs = 2_u64.pow(attempt - 1);
            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        }
    }

    Err(WebhookError::DeliveryFailed(format!(
        "Failed after {} attempts",
        webhook_config.max_retries
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_config_validation_success() {
        let config = ProfileWebhookConfig {
            config: WebhookConfig {
                callback_url: "https://example.com/webhook".to_string(),
                headers: None,
                secret: None,
            },
            timeout_secs: 30,
            max_retries: 3,
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_webhook_config_validation_empty_url() {
        let config = ProfileWebhookConfig {
            config: WebhookConfig {
                callback_url: "".to_string(),
                headers: None,
                secret: None,
            },
            timeout_secs: 30,
            max_retries: 3,
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("callback_url cannot be empty"));
    }

    #[test]
    fn test_webhook_config_validation_invalid_url_scheme() {
        let config = ProfileWebhookConfig {
            config: WebhookConfig {
                callback_url: "ftp://example.com/webhook".to_string(),
                headers: None,
                secret: None,
            },
            timeout_secs: 30,
            max_retries: 3,
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("http:// or https://"));
    }

    #[test]
    fn test_webhook_config_validation_zero_timeout() {
        let config = ProfileWebhookConfig {
            config: WebhookConfig {
                callback_url: "https://example.com/webhook".to_string(),
                headers: None,
                secret: None,
            },
            timeout_secs: 0,
            max_retries: 3,
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timeout_secs must be greater than 0"));
    }

    #[test]
    fn test_sign_webhook_payload() {
        let secret = "my-secret-key";
        let payload = b"test payload";

        let signature = sign_webhook_payload(secret, payload);
        assert!(signature.starts_with("sha256="));
        assert_eq!(signature.len(), 71); // "sha256=" (7) + 64 hex chars
    }

    #[test]
    fn test_sign_webhook_payload_deterministic() {
        let secret = "my-secret-key";
        let payload = b"test payload";

        let sig1 = sign_webhook_payload(secret, payload);
        let sig2 = sign_webhook_payload(secret, payload);

        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_profiing_webhook_payload_serialization() {
        let payload = ProfilingWebhookPayload {
            request_id: Some("req-123".to_string()),
            function_name: "transfer".to_string(),
            result: json!({
                "total_instructions": 1000,
                "granularity": "instrumented"
            }),
            timestamp: Utc::now().to_rfc3339(),
        };

        let serialized = serde_json::to_value(&payload).unwrap();
        assert_eq!(serialized["function_name"], "transfer");
        assert_eq!(serialized["result"]["total_instructions"], 1000);
    }
}
