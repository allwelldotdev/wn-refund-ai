//! In-memory limits: a per-customer sliding window on posted messages, and a
//! per-email pause after repeated failed sign-ins. One backend process serves
//! the demo, and a restart forgetting either is harmless.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use uuid::Uuid;

pub const MESSAGES_PER_WINDOW: usize = 10;
pub const WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct RateLimiter {
    hits: Arc<Mutex<HashMap<Uuid, VecDeque<Instant>>>>,
    limit: usize,
    window: Duration,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(MESSAGES_PER_WINDOW, WINDOW)
    }
}

impl RateLimiter {
    pub fn new(limit: usize, window: Duration) -> Self {
        Self {
            hits: Arc::default(),
            limit,
            window,
        }
    }

    /// Records a hit, or returns how long until the oldest hit leaves the window.
    pub fn check(&self, key: Uuid) -> Result<(), Duration> {
        self.check_at(key, Instant::now())
    }

    fn check_at(&self, key: Uuid, now: Instant) -> Result<(), Duration> {
        let mut hits = self.hits.lock().unwrap_or_else(|e| e.into_inner());
        let recent = hits.entry(key).or_default();
        while recent
            .front()
            .is_some_and(|t| now.duration_since(*t) >= self.window)
        {
            recent.pop_front();
        }
        if recent.len() >= self.limit {
            let oldest = recent.front().copied().unwrap_or(now);
            return Err(self.window.saturating_sub(now.duration_since(oldest)));
        }
        recent.push_back(now);
        Ok(())
    }
}

pub const SIGN_IN_ATTEMPTS: u32 = 5;
pub const SIGN_IN_PAUSE: Duration = Duration::from_secs(5 * 60);
/// Failures older than this no longer count toward a pause.
const SIGN_IN_MEMORY: Duration = Duration::from_secs(15 * 60);
/// Above this many tracked emails, stale entries are pruned on each failure.
const SIGN_IN_PRUNE_AT: usize = 10_000;

#[derive(Clone, Copy)]
struct SignInEntry {
    failures: u32,
    last_failure: Instant,
    paused_until: Option<Instant>,
}

/// What a failed sign-in leads to.
#[derive(Debug, PartialEq, Eq)]
pub enum SignInFailure {
    /// The caller may try again this many more times before a pause.
    AttemptsLeft(u32),
    /// This failure started a pause of the given length.
    Paused(Duration),
}

/// Pauses sign-in for an email after `SIGN_IN_ATTEMPTS` consecutive failures.
/// Keyed by the normalised email whether or not an account exists, so the
/// responses reveal nothing about which emails are registered.
#[derive(Clone, Default)]
pub struct LoginThrottle {
    entries: Arc<Mutex<HashMap<String, SignInEntry>>>,
}

impl LoginThrottle {
    /// `Err(remaining)` while the email is paused.
    pub fn check(&self, email: &str) -> Result<(), Duration> {
        self.check_at(email, Instant::now())
    }

    pub fn record_failure(&self, email: &str) -> SignInFailure {
        self.record_failure_at(email, Instant::now())
    }

    pub fn record_success(&self, email: &str) {
        self.lock().remove(email);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, SignInEntry>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn check_at(&self, email: &str, now: Instant) -> Result<(), Duration> {
        let mut entries = self.lock();
        match entries.get(email).and_then(|e| e.paused_until) {
            Some(until) if until > now => Err(until - now),
            Some(_) => {
                entries.remove(email);
                Ok(())
            }
            None => Ok(()),
        }
    }

    fn record_failure_at(&self, email: &str, now: Instant) -> SignInFailure {
        let mut entries = self.lock();
        if entries.len() >= SIGN_IN_PRUNE_AT {
            entries.retain(|_, e| {
                e.paused_until.is_some_and(|u| u > now)
                    || now.duration_since(e.last_failure) < SIGN_IN_MEMORY
            });
        }
        let entry = entries.entry(email.to_owned()).or_insert(SignInEntry {
            failures: 0,
            last_failure: now,
            paused_until: None,
        });
        if now.duration_since(entry.last_failure) >= SIGN_IN_MEMORY {
            entry.failures = 0;
        }
        entry.failures += 1;
        entry.last_failure = now;
        if entry.failures >= SIGN_IN_ATTEMPTS {
            entry.failures = 0;
            entry.paused_until = Some(now + SIGN_IN_PAUSE);
            SignInFailure::Paused(SIGN_IN_PAUSE)
        } else {
            SignInFailure::AttemptsLeft(SIGN_IN_ATTEMPTS - entry.failures)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_the_limit_then_waits_for_the_oldest_hit() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let t0 = Instant::now();
        assert!(limiter.check_at(a, t0).is_ok());
        assert!(limiter.check_at(a, t0 + Duration::from_secs(10)).is_ok());
        assert_eq!(
            limiter.check_at(a, t0 + Duration::from_secs(20)),
            Err(Duration::from_secs(40))
        );
        assert!(limiter.check_at(b, t0 + Duration::from_secs(20)).is_ok());
        assert!(limiter.check_at(a, t0 + Duration::from_secs(60)).is_ok());
        assert!(limiter.check_at(a, t0 + Duration::from_secs(61)).is_err());
    }

    #[test]
    fn five_failures_pause_sign_in_and_success_resets() {
        let throttle = LoginThrottle::default();
        let t0 = Instant::now();
        for left in [4, 3, 2, 1] {
            assert_eq!(
                throttle.record_failure_at("a@x", t0),
                SignInFailure::AttemptsLeft(left)
            );
        }
        assert_eq!(
            throttle.record_failure_at("a@x", t0),
            SignInFailure::Paused(SIGN_IN_PAUSE)
        );
        assert_eq!(
            throttle.check_at("a@x", t0 + Duration::from_secs(60)),
            Err(Duration::from_secs(240))
        );
        assert!(throttle.check_at("b@x", t0).is_ok());
        assert!(throttle.check_at("a@x", t0 + SIGN_IN_PAUSE).is_ok());
        assert_eq!(
            throttle.record_failure_at("a@x", t0 + SIGN_IN_PAUSE),
            SignInFailure::AttemptsLeft(4)
        );
        throttle.record_success("a@x");
        assert_eq!(
            throttle.record_failure_at("a@x", t0 + SIGN_IN_PAUSE),
            SignInFailure::AttemptsLeft(4)
        );
    }

    #[test]
    fn old_failures_are_forgotten() {
        let throttle = LoginThrottle::default();
        let t0 = Instant::now();
        for _ in 0..4 {
            throttle.record_failure_at("a@x", t0);
        }
        assert_eq!(
            throttle.record_failure_at("a@x", t0 + SIGN_IN_MEMORY),
            SignInFailure::AttemptsLeft(4)
        );
    }
}
