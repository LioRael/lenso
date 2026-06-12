use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_delay: Duration,
}

impl RetryPolicy {
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            initial_delay: Duration::ZERO,
        }
    }

    pub fn fixed(max_attempts: u32, initial_delay: Duration) -> Self {
        Self {
            max_attempts,
            initial_delay,
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::fixed(3, Duration::from_secs(5))
    }
}
