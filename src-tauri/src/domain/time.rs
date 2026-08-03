use std::time::{SystemTime, UNIX_EPOCH};

use super::{DomainError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimestampMs(i64);

impl TimestampMs {
    pub fn new(value: i64) -> Result<Self> {
        if value < 0 {
            return Err(DomainError::invalid("timestamp"));
        }
        Ok(Self(value))
    }

    pub const fn as_i64(self) -> i64 {
        self.0
    }

    fn from_system_time(value: SystemTime) -> Self {
        let milliseconds = value
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self(i64::try_from(milliseconds).unwrap_or(i64::MAX))
    }
}

pub trait Clock: Send + Sync {
    fn now(&self) -> TimestampMs;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> TimestampMs {
        TimestampMs::from_system_time(SystemTime::now())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    now: TimestampMs,
}

impl FixedClock {
    pub const fn new(now: TimestampMs) -> Self {
        Self { now }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> TimestampMs {
        self.now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_is_deterministic() {
        let expected = TimestampMs::new(1_700_000_000_000).expect("timestamp is valid");
        let clock = FixedClock::new(expected);

        assert_eq!(clock.now(), expected);
        assert_eq!(clock.now(), expected);
    }

    #[test]
    fn timestamps_reject_pre_epoch_values() {
        assert!(TimestampMs::new(-1).is_err());
    }
}
