//! Cooperative throttling driven by rate-limit response headers.

use reqwest::header::HeaderMap;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub struct RpcThrottle {
    blocked_until: Arc<AtomicU64>,
}

impl Default for RpcThrottle {
    fn default() -> Self {
        Self {
            blocked_until: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl RpcThrottle {
    /// Wait until the provider's advertised retry window has elapsed.
    pub async fn wait(&self) {
        let deadline_ms = self.blocked_until.load(Ordering::Acquire);
        if deadline_ms == 0 {
            return;
        }
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        if deadline_ms > now_ms {
            tokio::time::sleep(Duration::from_millis(deadline_ms - now_ms)).await;
        }
    }

    /// Update the next permitted request time from standard RPC headers.
    pub async fn observe(&self, headers: &HeaderMap) -> Option<Duration> {
        let delay = retry_delay(headers, SystemTime::now())?;
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let new_deadline = now_ms.saturating_add(delay.as_millis() as u64);

        let mut current = self.blocked_until.load(Ordering::Acquire);
        while new_deadline > current {
            match self.blocked_until.compare_exchange_weak(
                current,
                new_deadline,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
        Some(delay)
    }

    /// Returns the current blocked-until deadline as Unix milliseconds, or 0 if none.
    pub fn deadline_ms(&self) -> u64 {
        self.blocked_until.load(Ordering::Acquire)
    }
}

pub fn retry_delay(headers: &HeaderMap, now: SystemTime) -> Option<Duration> {
    if let Some(seconds) = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
    {
        return Some(Duration::from_secs(seconds));
    }

    let reset = headers
        .get("x-ratelimit-reset")?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    let now = now.duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(Duration::from_secs(reset.saturating_sub(now)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderValue, RETRY_AFTER};

    #[test]
    fn retry_after_takes_precedence() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("7"));
        headers.insert("x-ratelimit-reset", HeaderValue::from_static("999999"));
        assert_eq!(
            retry_delay(&headers, UNIX_EPOCH),
            Some(Duration::from_secs(7))
        );
    }

    #[test]
    fn parses_unix_reset_and_clamps_past_deadlines() {
        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-reset", HeaderValue::from_static("110"));
        assert_eq!(
            retry_delay(&headers, UNIX_EPOCH + Duration::from_secs(100)),
            Some(Duration::from_secs(10))
        );
        assert_eq!(
            retry_delay(&headers, UNIX_EPOCH + Duration::from_secs(120)),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn ignores_invalid_or_missing_headers() {
        assert_eq!(retry_delay(&HeaderMap::new(), UNIX_EPOCH), None);
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("tomorrow"));
        assert_eq!(retry_delay(&headers, UNIX_EPOCH), None);
    }

    #[tokio::test]
    async fn observe_updates_deadline_lock_free() {
        let throttle = RpcThrottle::default();
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("2"));

        let delay = throttle.observe(&headers).await;
        assert_eq!(delay, Some(Duration::from_secs(2)));

        let deadline = throttle.deadline_ms();
        assert!(deadline > 0);
    }

    #[tokio::test]
    async fn observe_keeps_later_deadline() {
        let throttle = RpcThrottle::default();
        let mut headers_long = HeaderMap::new();
        headers_long.insert(RETRY_AFTER, HeaderValue::from_static("10"));
        throttle.observe(&headers_long).await;

        let deadline_long = throttle.deadline_ms();

        let mut headers_short = HeaderMap::new();
        headers_short.insert(RETRY_AFTER, HeaderValue::from_static("2"));
        throttle.observe(&headers_short).await;

        let deadline_after = throttle.deadline_ms();
        assert_eq!(deadline_after, deadline_long);
    }

    #[tokio::test]
    async fn wait_returns_immediately_when_no_deadline() {
        let throttle = RpcThrottle::default();
        throttle.wait().await;
    }
}

pub struct TokenBucket {
    tokens: AtomicU64,
}

impl TokenBucket {
    pub fn new(initial: u64) -> Self {
        Self {
            tokens: AtomicU64::new(initial),
        }
    }

    pub fn acquire(&self) -> bool {
        let mut current = self.tokens.load(Ordering::Acquire);
        while current > 0 {
            match self.tokens.compare_exchange_weak(
                current,
                current - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(v) => current = v,
            }
        }
        false
    }
}
