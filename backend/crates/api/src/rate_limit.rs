//! Per-customer sliding-window limit on posted messages. In memory: one backend
//! process serves the demo, and a restart forgetting the window is harmless.

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
}
