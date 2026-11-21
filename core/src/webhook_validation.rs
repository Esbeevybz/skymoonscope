//! HMAC-SHA256 signature validation for inbound webhook calls.
//!
//! # Security design
//!
//! Each inbound request must carry three headers:
//!   - `x-Sky Moon Scope-signature` — `sha256=<hex>`, the HMAC-SHA256 over
//!     `<timestamp_bytes>.<raw_body_bytes>` using the pre-shared secret.
//!   - `x-Sky Moon Scope-timestamp` — Unix timestamp (seconds) as an ASCII integer.
//!   - `x-Sky Moon Scope-delivery` — opaque delivery UUID (validated for presence only).
//!
//! **Replay protection:** timestamps outside the configured tolerance are
//! rejected (default: [`MAX_TIMESTAMP_SKEW_SECS`]). Delivery nonces and
//! authenticated signatures are retained through the full acceptance window.
//!
//! **Constant-time comparison:** signature bytes are compared with
//! [`hmac::Mac::verify_slice`], which is constant-time by construction and avoids
//! timing-based forgery.
//!
//! # Complexity
//!
//! - Time: `O(n)` in the byte length of the request body, plus cache cleanup.
//! - Space: `O(r)` for `r` deliveries still inside the replay window.
//!
//! # Usage
//!
//! Wrap any inbound route with [`ValidatedWebhook`] as the last extractor:
//!
//! ```rust,ignore
//! async fn handler(ValidatedWebhook(body): ValidatedWebhook) -> impl IntoResponse {
//!     // `body` is the raw, authenticated request bytes.
//! }
//! ```

use axum::{
    async_trait,
    body::Bytes,
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::webhooks::{DELIVERY_HEADER, SIGNATURE_HEADER, TIMESTAMP_HEADER};

type HmacSha256 = Hmac<Sha256>;

/// Default allowed clock skew (seconds) between the timestamp in the request
/// header and the server's wall clock. Requests older than this are rejected
/// to prevent replay attacks.
///
/// Five minutes is the industry-standard window.
pub const MAX_TIMESTAMP_SKEW_SECS: u64 = 300;

/// Default shared by the library and server configuration loaders.
pub fn default_timestamp_tolerance_secs() -> u64 {
    MAX_TIMESTAMP_SKEW_SECS
}

/// Configured clock skew, injected as an Axum extension by the server.
#[derive(Clone, Copy)]
pub struct InboundWebhookTimestampTolerance(pub u64);

impl Default for InboundWebhookTimestampTolerance {
    fn default() -> Self {
        Self(default_timestamp_tolerance_secs())
    }
}

impl InboundWebhookTimestampTolerance {
    fn accepts(self, request_ts: u64, now_secs: u64) -> bool {
        now_secs.abs_diff(request_ts) <= self.0
    }
}
// ─────────────────────────────────────────────────────────────────────────────
// Shared secret state
// ─────────────────────────────────────────────────────────────────────────────

/// Server-side pre-shared secret injected as an Axum [`Extension`].
#[derive(Clone)]
pub struct InboundWebhookSecret(pub Arc<String>);

/// Shared cache of delivery nonces accepted during the timestamp window.
#[derive(Clone, Default)]
pub struct InboundWebhookNonceCache {
    nonces: Arc<Mutex<HashMap<String, u64>>>,
}

impl InboundWebhookNonceCache {
    fn check_and_store(
        &self,
        nonce: String,
        signature: String,
        now_secs: u64,
        request_ts: u64,
        tolerance_secs: u64,
    ) -> bool {
        let mut nonces = self.nonces.lock().expect("webhook nonce cache poisoned");
        nonces.retain(|_, expires_at| *expires_at >= now_secs);

        let nonce_key = format!("nonce:{nonce}");
        let signature_key = format!("signature:{signature}");
        if nonces.contains_key(&nonce_key) || nonces.contains_key(&signature_key) {
            return false;
        }

        // Future-dated signatures remain valid after now + tolerance. Keep them
        // cached through their last accepted second, including a zero window.
        let expires_at = now_secs.max(request_ts).saturating_add(tolerance_secs);
        nonces.insert(nonce_key, expires_at);
        nonces.insert(signature_key, expires_at);
        true
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Extractor
// ─────────────────────────────────────────────────────────────────────────────

/// Axum extractor that validates an inbound webhook request's HMAC-SHA256
/// signature before the handler runs.
pub struct ValidatedWebhook(pub Bytes);

#[async_trait]
impl<S> FromRequest<S> for ValidatedWebhook
where
    S: Send + Sync,
{
    type Rejection = WebhookValidationError;

    async fn from_request(mut req: Request, state: &S) -> Result<Self, Self::Rejection> {
        // 1. Resolve the shared secret: check Extensions, fallback to environment variable.
        let secret = if let Some(InboundWebhookSecret(s)) =
            req.extensions().get::<InboundWebhookSecret>().cloned()
        {
            (*s).clone()
        } else if let Ok(s) = std::env::var("Sky Moon Scope_INBOUND_WEBHOOK_SECRET") {
            if s.len() < 32 {
                return Err(WebhookValidationError::MissingSecret);
            }
            s
        } else {
            return Err(WebhookValidationError::MissingSecret);
        };

        // 2. Read required header values.
        let timestamp_val = req
            .headers()
            .get(TIMESTAMP_HEADER)
            .and_then(|v| v.to_str().ok())
            .ok_or(WebhookValidationError::MissingHeader(TIMESTAMP_HEADER))?
            .to_owned();

        let signature_val = req
            .headers()
            .get(SIGNATURE_HEADER)
            .and_then(|v| v.to_str().ok())
            .ok_or(WebhookValidationError::MissingHeader(SIGNATURE_HEADER))?
            .to_owned();

        let delivery_id = req
            .headers()
            .get(DELIVERY_HEADER)
            .and_then(|v| v.to_str().ok())
            .filter(|value| !value.is_empty())
            .ok_or(WebhookValidationError::MissingHeader(DELIVERY_HEADER))?
            .to_owned();

        // 3. Validate timestamp freshness.
        let request_ts: u64 = timestamp_val
            .parse()
            .map_err(|_| WebhookValidationError::InvalidTimestamp)?;

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let tolerance = req
            .extensions()
            .get::<InboundWebhookTimestampTolerance>()
            .copied()
            .unwrap_or_default();

        if !tolerance.accepts(request_ts, now_secs) {
            return Err(WebhookValidationError::TimestampExpired);
        }

        let nonce_cache = req
            .extensions()
            .get::<InboundWebhookNonceCache>()
            .cloned()
            .ok_or(WebhookValidationError::MissingNonceCache)?;

        // 4. Buffer the body.
        let body = Bytes::from_request(req, state)
            .await
            .map_err(|_| WebhookValidationError::BodyReadError)?;

        // 5. Verify HMAC-SHA256 signature in constant time.
        if !verify_signature(&secret, &timestamp_val, &body, &signature_val) {
            return Err(WebhookValidationError::InvalidSignature);
        }

        if !nonce_cache.check_and_store(
            delivery_id,
            signature_val,
            now_secs,
            request_ts,
            tolerance.0,
        ) {
            return Err(WebhookValidationError::NonceAlreadyUsed);
        }

        Ok(Self(body))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Core verification logic
// ─────────────────────────────────────────────────────────────────────────────

/// Verify an HMAC-SHA256 signature.
pub fn verify_signature(secret: &str, timestamp: &str, body: &[u8], signature: &str) -> bool {
    let hex_sig = match signature.strip_prefix("sha256=") {
        Some(h) => h,
        None => return false,
    };

    let sig_bytes = match hex::decode(hex_sig) {
        Ok(b) => b,
        Err(_) => return false,
    };

    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any size");
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);

    mac.verify_slice(&sig_bytes).is_ok()
}

// ─────────────────────────────────────────────────────────────────────────────
// Error type
// ─────────────────────────────────────────────────────────────────────────────

/// Errors that can occur during inbound webhook signature validation.
#[derive(Debug)]
pub enum WebhookValidationError {
    /// A required header was absent.
    MissingHeader(&'static str),
    /// The timestamp header value could not be parsed as a `u64`.
    InvalidTimestamp,
    /// The timestamp is outside the skew window.
    TimestampExpired,
    /// The HMAC signature verification failed.
    InvalidSignature,
    /// The request body could not be read.
    BodyReadError,
    /// The pre-shared secret is not configured.
    MissingSecret,
    /// The shared replay-protection cache is not configured.
    MissingNonceCache,
    /// The delivery nonce was already accepted during the replay window.
    NonceAlreadyUsed,
}

impl IntoResponse for WebhookValidationError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::MissingHeader(name) => (
                StatusCode::BAD_REQUEST,
                format!("Missing required header: {name}"),
            ),
            Self::InvalidTimestamp => (
                StatusCode::BAD_REQUEST,
                "Invalid timestamp header value".to_string(),
            ),
            Self::TimestampExpired => (
                StatusCode::UNAUTHORIZED,
                "Request timestamp outside configured tolerance".to_string(),
            ),
            Self::InvalidSignature => (
                StatusCode::UNAUTHORIZED,
                "Webhook signature verification failed".to_string(),
            ),
            Self::BodyReadError => (
                StatusCode::BAD_REQUEST,
                "Failed to read request body".to_string(),
            ),
            Self::MissingSecret => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Inbound webhook secret not configured".to_string(),
            ),
            Self::MissingNonceCache => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Inbound webhook nonce cache not configured".to_string(),
            ),
            Self::NonceAlreadyUsed => (
                StatusCode::UNAUTHORIZED,
                "Webhook delivery nonce has already been used".to_string(),
            ),
        };

        (status, message).into_response()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit and Integration tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webhooks::sign;
    use axum::{body::Body, http::Request, routing::post, Router};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tower::ServiceExt;

    fn now_str() -> String {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string()
    }

    const SECRET: &str = "a-secret-that-is-at-least-thirty-two-bytes";
    const BODY: &[u8] = br#"{"event":"transfer","amount":"42"}"#;

    #[tokio::test]
    async fn extractor_uses_configured_timestamp_tolerance() {
        for (seconds, offset, expected) in [
            (30, -120, StatusCode::UNAUTHORIZED),
            (30, 120, StatusCode::UNAUTHORIZED),
            (900, -600, StatusCode::OK),
            (900, 600, StatusCode::OK),
            (900, -1_200, StatusCode::UNAUTHORIZED),
            (900, 1_200, StatusCode::UNAUTHORIZED),
        ] {
            let app = Router::new()
                .route(
                    "/test",
                    post(|ValidatedWebhook(_): ValidatedWebhook| async { StatusCode::OK }),
                )
                .layer(Extension(InboundWebhookSecret(Arc::new(SECRET.into()))))
                .layer(Extension(InboundWebhookNonceCache::default()))
                .layer(Extension(InboundWebhookTimestampTolerance(seconds)));
            let timestamp = (now_str().parse::<i64>().unwrap() + offset).to_string();
            let signature = format!("sha256={}", sign(SECRET, &timestamp, BODY));
            let req = Request::builder()
                .method("POST")
                .uri("/test")
                .header(TIMESTAMP_HEADER, &timestamp)
                .header(SIGNATURE_HEADER, &signature)
                .header(DELIVERY_HEADER, "configured-tolerance")
                .body(Body::from(BODY))
                .unwrap();
            assert_eq!(app.oneshot(req).await.unwrap().status(), expected);
        }
    }

    #[test]
    fn timestamp_tolerance_checks_both_inclusive_boundaries() {
        assert_eq!(InboundWebhookTimestampTolerance::default().0, 300);
        for seconds in [0, 30, 300, 900] {
            let tolerance = InboundWebhookTimestampTolerance(seconds);
            assert!(tolerance.accepts(1000 - seconds, 1000));
            assert!(tolerance.accepts(1000 + seconds, 1000));
            assert!(!tolerance.accepts(999 - seconds, 1000));
            assert!(!tolerance.accepts(1001 + seconds, 1000));
        }
        assert!(!InboundWebhookTimestampTolerance(300).accepts(u64::MAX, 0));
        assert!(InboundWebhookTimestampTolerance(u64::MAX).accepts(0, u64::MAX));
    }

    #[test]
    fn replay_cache_covers_future_timestamp_and_inclusive_expiry() {
        let cache = InboundWebhookNonceCache::default();
        assert!(cache.check_and_store("delivery".into(), "signature".into(), 1000, 1900, 900));
        // The signature is still valid long after the former 300-second TTL.
        assert!(!cache.check_and_store("other".into(), "signature".into(), 2800, 1900, 900));
        assert!(!cache.check_and_store("delivery".into(), "other".into(), 2800, 1900, 900));
        assert!(cache.check_and_store("delivery".into(), "signature".into(), 2801, 2801, 900));
    }

    #[test]
    fn zero_tolerance_still_prevents_replay_in_the_same_second() {
        let cache = InboundWebhookNonceCache::default();
        assert!(cache.check_and_store("delivery".into(), "signature".into(), 1000, 1000, 0));
        assert!(!cache.check_and_store("delivery".into(), "signature".into(), 1000, 1000, 0));
        assert!(cache.check_and_store("delivery".into(), "signature".into(), 1001, 1001, 0));
    }

    #[test]
    fn valid_signature_is_accepted() {
        let ts = now_str();
        let sig = format!("sha256={}", sign(SECRET, &ts, BODY));
        assert!(verify_signature(SECRET, &ts, BODY, &sig));
    }

    #[test]
    fn signature_with_wrong_body_is_rejected() {
        let ts = now_str();
        let sig = format!("sha256={}", sign(SECRET, &ts, BODY));
        assert!(!verify_signature(SECRET, &ts, b"tampered", &sig));
    }

    #[test]
    fn signature_with_wrong_timestamp_is_rejected() {
        let ts = now_str();
        let sig = format!("sha256={}", sign(SECRET, &ts, BODY));
        assert!(!verify_signature(SECRET, "0", BODY, &sig));
    }

    #[test]
    fn signature_with_wrong_secret_is_rejected() {
        let ts = now_str();
        let sig = format!("sha256={}", sign(SECRET, &ts, BODY));
        let wrong_secret = "wrong-secret-that-is-at-least-thirty-two-bytes";
        assert!(!verify_signature(wrong_secret, &ts, BODY, &sig));
    }

    #[tokio::test]
    async fn extractor_accepts_valid_signature() {
        let secret = Arc::new(SECRET.to_string());
        let app = Router::new()
            .route(
                "/test",
                post(|ValidatedWebhook(body): ValidatedWebhook| async move {
                    assert_eq!(body, BODY);
                    StatusCode::OK
                }),
            )
            .layer(Extension(InboundWebhookSecret(secret)))
            .layer(Extension(InboundWebhookNonceCache::default()));

        let ts = now_str();
        let sig = format!("sha256={}", sign(SECRET, &ts, BODY));

        let req = Request::builder()
            .method("POST")
            .uri("/test")
            .header(TIMESTAMP_HEADER, &ts)
            .header(SIGNATURE_HEADER, &sig)
            .header(DELIVERY_HEADER, "some-uuid")
            .body(Body::from(BODY))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn extractor_rejects_reused_delivery_nonce() {
        let secret = Arc::new(SECRET.to_string());
        let app = Router::new()
            .route(
                "/test",
                post(|ValidatedWebhook(_): ValidatedWebhook| async move { StatusCode::OK }),
            )
            .layer(Extension(InboundWebhookSecret(secret)))
            .layer(Extension(InboundWebhookNonceCache::default()));

        let ts = now_str();
        let sig = format!("sha256={}", sign(SECRET, &ts, BODY));
        let request = || {
            Request::builder()
                .method("POST")
                .uri("/test")
                .header(TIMESTAMP_HEADER, &ts)
                .header(SIGNATURE_HEADER, &sig)
                .header(DELIVERY_HEADER, "reused-delivery-id")
                .body(Body::from(BODY))
                .unwrap()
        };

        let first = app.clone().oneshot(request()).await.unwrap();
        assert_eq!(first.status(), StatusCode::OK);

        let second = app.oneshot(request()).await.unwrap();
        assert_eq!(second.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn extractor_rejects_missing_headers() {
        let secret = Arc::new(SECRET.to_string());
        let app = Router::new()
            .route(
                "/test",
                post(|ValidatedWebhook(_): ValidatedWebhook| async move { StatusCode::OK }),
            )
            .layer(Extension(InboundWebhookSecret(secret)))
            .layer(Extension(InboundWebhookNonceCache::default()));

        let ts = now_str();
        let sig = format!("sha256={}", sign(SECRET, &ts, BODY));

        // Missing signature header
        let req = Request::builder()
            .method("POST")
            .uri("/test")
            .header(TIMESTAMP_HEADER, &ts)
            .header(DELIVERY_HEADER, "some-uuid")
            .body(Body::from(BODY))
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // Missing delivery header
        let req = Request::builder()
            .method("POST")
            .uri("/test")
            .header(TIMESTAMP_HEADER, &ts)
            .header(SIGNATURE_HEADER, &sig)
            .body(Body::from(BODY))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn extractor_rejects_expired_timestamp() {
        let secret = Arc::new(SECRET.to_string());
        let app = Router::new()
            .route(
                "/test",
                post(|ValidatedWebhook(_): ValidatedWebhook| async move { StatusCode::OK }),
            )
            .layer(Extension(InboundWebhookSecret(secret)))
            .layer(Extension(InboundWebhookNonceCache::default()));

        let expired_ts = (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - MAX_TIMESTAMP_SKEW_SECS
            - 10)
            .to_string();

        let sig = format!("sha256={}", sign(SECRET, &expired_ts, BODY));

        let req = Request::builder()
            .method("POST")
            .uri("/test")
            .header(TIMESTAMP_HEADER, &expired_ts)
            .header(SIGNATURE_HEADER, &sig)
            .header(DELIVERY_HEADER, "some-uuid")
            .body(Body::from(BODY))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
