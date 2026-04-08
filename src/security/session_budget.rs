#[derive(Debug, Clone)]
pub struct SessionBudget {
    pub total: u32,
    pub remaining: u32,
    pub reset_after_secs: u64,
    pub low_watermark: u32,
}

impl SessionBudget {
    pub fn new(total: u32, remaining: u32, reset_after_secs: u64, low_watermark: u32) -> Self {
        Self { total, remaining, reset_after_secs, low_watermark: low_watermark.max(1) }
    }

    pub fn consume_identify(&mut self) {
        self.remaining = self.remaining.saturating_sub(1);
    }

    pub fn is_low(&self) -> bool {
        self.remaining <= self.low_watermark
    }
}

#[cfg(test)]
mod tests {
    use super::SessionBudget;

    #[test]
    fn low_budget_detected() {
        let mut b = SessionBudget::new(1000, 2, 100, 2);
        assert!(b.is_low());
        b.consume_identify();
        assert!(b.is_low());
    }
}
