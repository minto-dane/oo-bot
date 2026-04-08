use std::time::Instant;

#[derive(Debug, Clone)]
pub struct TokenBucket {
    capacity: f64,
    refill_per_sec: f64,
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    pub fn new(capacity: u32, refill_per_sec: f64, now: Instant) -> Self {
        let cap = capacity.max(1) as f64;
        Self {
            capacity: cap,
            refill_per_sec: refill_per_sec.max(0.1),
            tokens: cap,
            last_refill: now,
        }
    }

    pub fn try_take(&mut self, amount: u32, now: Instant) -> bool {
        self.refill(now);
        let needed = amount.max(1) as f64;
        if self.tokens >= needed {
            self.tokens -= needed;
            true
        } else {
            false
        }
    }

    fn refill(&mut self, now: Instant) {
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        if elapsed <= 0.0 {
            return;
        }
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last_refill = now;
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::TokenBucket;

    #[test]
    fn token_bucket_refills() {
        let now = Instant::now();
        let mut b = TokenBucket::new(2, 1.0, now);
        assert!(b.try_take(1, now));
        assert!(b.try_take(1, now));
        assert!(!b.try_take(1, now));

        let later = now + Duration::from_secs(1);
        assert!(b.try_take(1, later));
    }
}
