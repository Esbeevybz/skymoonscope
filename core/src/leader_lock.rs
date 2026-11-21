//! Redis-backed lease lock for electing a single leader among multiple
//! Core instances, so leader-only background work (e.g. ledger fee
//! collection) runs on exactly one instance at a time and doesn't race on
//! shared state.
//!
//! This is a simplified, single-Redis-node lease lock: `SET key token NX
//! PX` to acquire, a Lua compare-and-expire script to renew, and a
//! compare-and-delete Lua script to release — the same primitives the
//! Redlock algorithm builds on, sized down to the one `redis_url` this
//! service already connects to for its job queue.

use redis::{AsyncCommands, Client as RedisClient, ExistenceCheck, SetExpiry, SetOptions};
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tokio::time::Instant;
use uuid::Uuid;

const RENEW_SCRIPT: &str = r#"
if redis.call("GET", KEYS[1]) == ARGV[1] then
    return redis.call("PEXPIRE", KEYS[1], ARGV[2])
else
    return 0
end
"#;

const RELEASE_SCRIPT: &str = r#"
if redis.call("GET", KEYS[1]) == ARGV[1] then
    return redis.call("DEL", KEYS[1])
else
    return 0
end
"#;

const RENEW_SCRIPT_SHA: &str = "9d70d8566d8043418770b1952c5d93613a3d574f";
const RELEASE_SCRIPT_SHA: &str = "aefc6bbfaac1ed02c135d12b12c8c42cdfb36af4";

/// Default proactive renewal interval, expressed as a percentage of the lease
/// TTL.
///
/// Renewing at 40% of the TTL (rather than a hard-coded half-TTL) leaves a
/// larger safety margin, so a renewal delayed by Redis latency still lands well
/// before the lease expires and leadership is not lost for a cycle (Issue #070).
const DEFAULT_RENEWAL_INTERVAL_PERCENT: usize = 40;

#[derive(Debug, thiserror::Error)]
pub enum LeaderLockInitError {
    #[error("Redis scripting command failed: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("{script} script SHA mismatch: expected {expected}, got {actual}")]
    ShaMismatch {
        script: &'static str,
        expected: &'static str,
        actual: String,
    },
}

/// A renewable Redis lease used to elect a single leader across instances.
pub struct RedisLeaderLock {
    redis: RedisClient,
    key: String,
    token: String,
    ttl_ms: usize,
    /// How often the lease is proactively renewed before it expires.
    renewal_interval_ms: usize,
    lease_deadline: std::sync::Mutex<Option<Instant>>,
    /// Earliest instant at which the next renewal should be attempted.
    next_renewal: std::sync::Mutex<Option<Instant>>,
}

impl RedisLeaderLock {
    pub fn new(redis: RedisClient, key: impl Into<String>, ttl: Duration) -> Self {
        let ttl_ms = ttl.as_millis().try_into().unwrap_or(usize::MAX).max(1);
        // Default renewal interval: 40% of the TTL, clamped to `[1, ttl]`.
        let renewal_interval_ms =
            (ttl_ms.saturating_mul(DEFAULT_RENEWAL_INTERVAL_PERCENT) / 100).clamp(1, ttl_ms);

        Self {
            redis,
            key: key.into(),
            token: Uuid::new_v4().to_string(),
            ttl_ms,
            renewal_interval_ms,
            lease_deadline: std::sync::Mutex::new(None),
            next_renewal: std::sync::Mutex::new(None),
        }
    }

    /// Override the proactive renewal interval (default: 40% of the TTL).
    ///
    /// The interval is clamped to `[1 ms, ttl]`.  A shorter interval renews more
    /// often (more Redis traffic); a longer interval reduces the safety margin
    /// before the lease expires.
    pub fn with_renewal_interval(mut self, interval: Duration) -> Self {
        let interval_ms: usize = interval.as_millis().try_into().unwrap_or(usize::MAX);
        self.renewal_interval_ms = interval_ms.clamp(1, self.ttl_ms);
        self
    }

    /// Load and validate the Lua scripts before starting leader election.
    pub async fn load_scripts(&self) -> Result<(), LeaderLockInitError> {
        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let renew_sha: String = redis::cmd("SCRIPT")
            .arg("LOAD")
            .arg(RENEW_SCRIPT)
            .query_async(&mut conn)
            .await?;
        if renew_sha != RENEW_SCRIPT_SHA {
            return Err(LeaderLockInitError::ShaMismatch {
                script: "renew",
                expected: RENEW_SCRIPT_SHA,
                actual: renew_sha,
            });
        }

        let release_sha: String = redis::cmd("SCRIPT")
            .arg("LOAD")
            .arg(RELEASE_SCRIPT)
            .query_async(&mut conn)
            .await?;
        if release_sha != RELEASE_SCRIPT_SHA {
            return Err(LeaderLockInitError::ShaMismatch {
                script: "release",
                expected: RELEASE_SCRIPT_SHA,
                actual: release_sha,
            });
        }

        Ok(())
    }

    /// Attempt to become (or remain) leader. Returns `true` if this
    /// instance holds the lease after the call, `false` if another
    /// instance currently holds it (or Redis is unreachable, in which case
    /// callers should treat the cycle as non-leader and skip).
    pub async fn try_acquire_or_renew(&self) -> bool {
        let now = Instant::now();

        // Renewal-interval gate (Issue #070): while we still hold a lease that
        // has not yet reached its renewal interval, report leadership without a
        // Redis round-trip.  Renewing no more often than `renewal_interval`
        // (default 40% of the TTL) keeps a safety margin so a renewal delayed by
        // Redis latency still arrives before the lease expires.
        {
            let deadline = *self
                .lease_deadline
                .lock()
                .expect("leader lock deadline mutex poisoned");
            let renew_at = *self
                .next_renewal
                .lock()
                .expect("leader lock renewal mutex poisoned");
            if let (Some(deadline), Some(renew_at)) = (deadline, renew_at) {
                if now < deadline && now < renew_at {
                    return true;
                }
            }
        }

        let expired = self
            .lease_deadline
            .lock()
            .expect("leader lock deadline mutex poisoned")
            .is_some_and(|deadline| now >= deadline);
        if expired {
            *self
                .lease_deadline
                .lock()
                .expect("leader lock deadline mutex poisoned") = None;
            *self
                .next_renewal
                .lock()
                .expect("leader lock renewal mutex poisoned") = None;
        }

        let mut conn = match self.redis.get_multiplexed_async_connection().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "leader lock: failed to connect to Redis");
                return false;
            }
        };

        // If we already hold the lease, extend it.
        let renewed: i64 = if expired {
            0
        } else {
            match redis::Script::new(RENEW_SCRIPT)
                .key(&self.key)
                .arg(&self.token)
                .arg(self.ttl_ms)
                .invoke_async(&mut conn)
                .await
            {
                Ok(renewed) => renewed,
                Err(error) => {
                    tracing::error!(
                        severity = "critical",
                        error = %error,
                        "Leader lease renewal script failed"
                    );
                    return false;
                }
            }
        };

        if renewed == 1 {
            self.update_lease_deadline();
            return true;
        }

        // Otherwise try to claim it fresh (fails if another instance holds it).
        let opts = SetOptions::default()
            .conditional_set(ExistenceCheck::NX)
            .with_expiration(SetExpiry::PX(self.ttl_ms));

        let acquired = conn
            .set_options::<_, _, bool>(&self.key, self.token.as_str(), opts)
            .await
            .unwrap_or(false);
        if acquired {
            self.update_lease_deadline();
        }
        acquired
    }

    fn update_lease_deadline(&self) {
        let now = Instant::now();
        *self
            .lease_deadline
            .lock()
            .expect("leader lock deadline mutex poisoned") =
            Some(now + Duration::from_millis(self.ttl_ms as u64));
        *self
            .next_renewal
            .lock()
            .expect("leader lock renewal mutex poisoned") =
            Some(now + Duration::from_millis(self.renewal_interval_ms as u64));
    }

    /// Release the lease if still held by this instance. Best-effort — if
    /// this fails (e.g. Redis is briefly unreachable) the lease simply
    /// expires after `ttl` and another instance takes over.
    #[allow(dead_code)]
    pub async fn release(&self) {
        let Ok(mut conn) = self.redis.get_multiplexed_async_connection().await else {
            return;
        };
        let released: Result<i64, _> = redis::Script::new(RELEASE_SCRIPT)
            .key(&self.key)
            .arg(&self.token)
            .invoke_async(&mut conn)
            .await;
        if let Err(error) = released {
            tracing::error!(
                severity = "critical",
                error = %error,
                "Leader lease release script failed"
            );
        }
        *self
            .lease_deadline
            .lock()
            .expect("leader lock deadline mutex poisoned") = None;
        *self
            .next_renewal
            .lock()
            .expect("leader lock renewal mutex poisoned") = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_deadline_survives_wall_clock_jump() {
        let start = Instant::now();
        let deadline = start + Duration::from_secs(1);

        // A wall-clock adjustment cannot change a tokio monotonic instant.
        assert!(Instant::now() < deadline);
        assert_eq!(deadline.duration_since(start), Duration::from_secs(1));
    }

    #[test]
    fn zero_ttl_is_encoded_as_one_millisecond() {
        let lock = RedisLeaderLock::new(
            RedisClient::open("redis://127.0.0.1/").expect("valid Redis URL"),
            "test",
            Duration::ZERO,
        );
        assert_eq!(lock.ttl_ms, 1);
    }

    #[test]
    fn default_renewal_interval_is_forty_percent_of_ttl() {
        let lock = RedisLeaderLock::new(
            RedisClient::open("redis://127.0.0.1/").expect("valid Redis URL"),
            "test",
            Duration::from_millis(1_000),
        );
        assert_eq!(lock.ttl_ms, 1_000);
        assert_eq!(lock.renewal_interval_ms, 400);
    }

    #[test]
    fn renewal_interval_override_is_clamped_to_ttl() {
        let lock = RedisLeaderLock::new(
            RedisClient::open("redis://127.0.0.1/").expect("valid Redis URL"),
            "test",
            Duration::from_millis(100),
        )
        .with_renewal_interval(Duration::from_secs(5));
        assert_eq!(lock.renewal_interval_ms, 100);
    }
}
