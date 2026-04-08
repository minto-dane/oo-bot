use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct HttpCircuitBreaker {
    window: Duration,
    threshold: usize,
    open_for: Duration,
    failures: VecDeque<Instant>,
    open_until: Option<Instant>,
}

impl HttpCircuitBreaker {
    pub fn new(window: Duration, threshold: usize, open_for: Duration) -> Self {
        Self {
            window,
            threshold: threshold.max(1),
            open_for,
            failures: VecDeque::new(),
            open_until: None,
        }
    }

    pub fn allows_outbound(&self, now: Instant) -> bool {
        self.open_until.map_or(true, |until| now >= until)
    }

    pub fn record_status(&mut self, status: u16, now: Instant) {
        if !matches!(status, 401 | 403 | 429) {
            return;
        }

        self.failures.push_back(now);
        self.trim(now);

        if self.failures.len() >= self.threshold {
            self.open_until = Some(now + self.open_for);
        }
    }

    pub fn is_open(&self, now: Instant) -> bool {
        self.open_until.is_some_and(|until| now < until)
    }

    fn trim(&mut self, now: Instant) {
        while let Some(front) = self.failures.front().copied() {
            if now.duration_since(front) > self.window {
                let _ = self.failures.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::HttpCircuitBreaker;

    #[test]
    fn opens_when_threshold_reached() {
        let now = Instant::now();
        let mut b = HttpCircuitBreaker::new(Duration::from_secs(10), 3, Duration::from_secs(5));

        b.record_status(429, now);
        b.record_status(403, now + Duration::from_secs(1));
        assert!(!b.is_open(now + Duration::from_secs(1)));
        b.record_status(401, now + Duration::from_secs(2));
        assert!(b.is_open(now + Duration::from_secs(2)));
        assert!(!b.is_open(now + Duration::from_secs(8)));
    }
}
