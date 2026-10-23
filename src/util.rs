use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use dashmap::DashMap;
use rand::RngCore;

pub fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn token_urlsafe(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut buffer);
    URL_SAFE_NO_PAD.encode(buffer)
}

#[derive(Default)]
pub struct RateLimiter {
    hits: DashMap<String, Vec<Instant>>,
}

pub type Limit = (usize, Duration);

impl RateLimiter {
    pub fn new() -> RateLimiter {
        RateLimiter::default()
    }

    pub fn allow(&self, key: &str, limits: &[Limit]) -> bool {
        let now = Instant::now();
        let longest = limits.iter().map(|(_, window)| *window).max().unwrap_or_default();
        let mut entry = self.hits.entry(key.to_string()).or_default();
        entry.retain(|hit| now.duration_since(*hit) < longest);
        for (max, window) in limits {
            let in_window = entry.iter().filter(|hit| now.duration_since(**hit) < *window).count();
            if in_window >= *max {
                return false;
            }
        }
        entry.push(now);
        true
    }

    pub fn reset(&self) {
        self.hits.clear();
    }

    pub fn prune(&self, max_age: Duration) {
        let now = Instant::now();
        self.hits.retain(|_, hits| hits.last().map(|last| now.duration_since(*last) < max_age).unwrap_or(false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limiter_blocks_after_the_budget_and_ignores_refused_requests() {
        let limiter = RateLimiter::new();
        let limits = [(3, Duration::from_secs(60))];
        assert!((0..3).all(|_| limiter.allow("a", &limits)));
        assert!(!limiter.allow("a", &limits));
        assert!(limiter.allow("b", &limits));
        limiter.reset();
        assert!(limiter.allow("a", &limits));
    }

    #[test]
    fn tokens_are_url_safe_and_unique() {
        let (a, b) = (token_urlsafe(32), token_urlsafe(32));
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }
}
