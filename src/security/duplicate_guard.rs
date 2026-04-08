use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct DuplicateGuard {
    ttl: Duration,
    cap: usize,
    seen: HashMap<u64, Instant>,
}

impl DuplicateGuard {
    pub fn new(ttl: Duration, cap: usize) -> Self {
        Self { ttl, cap: cap.max(64), seen: HashMap::new() }
    }

    pub fn is_duplicate_and_mark(&mut self, message_id: u64, now: Instant) -> bool {
        self.evict_old(now);
        let is_dup = self.seen.contains_key(&message_id);
        self.seen.insert(message_id, now);

        if self.seen.len() > self.cap {
            self.compact();
        }

        is_dup
    }

    fn evict_old(&mut self, now: Instant) {
        let ttl = self.ttl;
        self.seen.retain(|_, ts| now.duration_since(*ts) <= ttl);
    }

    fn compact(&mut self) {
        let mut items: Vec<(u64, Instant)> = self.seen.iter().map(|(k, v)| (*k, *v)).collect();
        items.sort_by_key(|(_, ts)| *ts);

        let keep_from = items.len().saturating_sub(self.cap);
        self.seen.clear();
        for (k, v) in items.into_iter().skip(keep_from) {
            let _ = self.seen.insert(k, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::DuplicateGuard;

    #[test]
    fn duplicate_is_detected_within_ttl() {
        let now = Instant::now();
        let mut guard = DuplicateGuard::new(Duration::from_secs(60), 128);

        assert!(!guard.is_duplicate_and_mark(1, now));
        assert!(guard.is_duplicate_and_mark(1, now + Duration::from_secs(1)));
        assert!(!guard.is_duplicate_and_mark(1, now + Duration::from_secs(62)));
    }
}
