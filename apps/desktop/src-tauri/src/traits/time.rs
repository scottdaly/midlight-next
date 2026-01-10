//! Time provider abstraction for testability.
//!
//! Provides a trait for time operations that can be controlled in tests.

use chrono::{DateTime, Utc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Abstraction over time operations for testability.
///
/// This allows tests to control time without relying on real clock.
pub trait TimeProvider: Send + Sync {
    /// Get the current system time.
    fn now(&self) -> SystemTime;

    /// Get the current time as a UTC DateTime.
    fn now_utc(&self) -> DateTime<Utc>;

    /// Get the current Unix timestamp in seconds.
    fn unix_timestamp(&self) -> i64 {
        self.now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs() as i64
    }

    /// Get the current Unix timestamp in milliseconds.
    fn unix_timestamp_millis(&self) -> i64 {
        self.now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as i64
    }
}

/// Real implementation using the system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealTimeProvider;

impl RealTimeProvider {
    pub fn new() -> Self {
        Self
    }
}

impl TimeProvider for RealTimeProvider {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }

    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Mock implementation for testing with controlled time.
#[cfg(test)]
pub use mock::MockTimeProvider;

#[cfg(test)]
mod mock {
    use super::*;
    use std::sync::{Arc, RwLock};

    /// Mock time provider for testing.
    ///
    /// Allows setting a fixed time or advancing time manually.
    #[derive(Debug, Clone)]
    pub struct MockTimeProvider {
        current: Arc<RwLock<SystemTime>>,
    }

    impl MockTimeProvider {
        /// Create a new mock time provider with the current real time.
        pub fn new() -> Self {
            Self {
                current: Arc::new(RwLock::new(SystemTime::now())),
            }
        }

        /// Create a mock time provider with a fixed time.
        pub fn fixed(time: SystemTime) -> Self {
            Self {
                current: Arc::new(RwLock::new(time)),
            }
        }

        /// Create a mock time provider with a fixed Unix timestamp.
        pub fn from_timestamp(secs: u64) -> Self {
            Self::fixed(UNIX_EPOCH + Duration::from_secs(secs))
        }

        /// Create a mock time provider from a DateTime<Utc>.
        pub fn from_datetime(dt: DateTime<Utc>) -> Self {
            Self::fixed(SystemTime::from(dt))
        }

        /// Set the current time.
        pub fn set(&self, time: SystemTime) {
            *self.current.write().unwrap() = time;
        }

        /// Set the current time from a Unix timestamp.
        pub fn set_timestamp(&self, secs: u64) {
            self.set(UNIX_EPOCH + Duration::from_secs(secs));
        }

        /// Advance time by a duration.
        pub fn advance(&self, duration: Duration) {
            let mut current = self.current.write().unwrap();
            *current = *current + duration;
        }

        /// Advance time by seconds.
        pub fn advance_secs(&self, secs: u64) {
            self.advance(Duration::from_secs(secs));
        }

        /// Advance time by minutes.
        pub fn advance_mins(&self, mins: u64) {
            self.advance(Duration::from_secs(mins * 60));
        }

        /// Advance time by hours.
        pub fn advance_hours(&self, hours: u64) {
            self.advance(Duration::from_secs(hours * 3600));
        }

        /// Advance time by days.
        pub fn advance_days(&self, days: u64) {
            self.advance(Duration::from_secs(days * 86400));
        }

        /// Rewind time by a duration.
        pub fn rewind(&self, duration: Duration) {
            let mut current = self.current.write().unwrap();
            *current = current.checked_sub(duration).unwrap_or(UNIX_EPOCH);
        }
    }

    impl Default for MockTimeProvider {
        fn default() -> Self {
            Self::new()
        }
    }

    impl TimeProvider for MockTimeProvider {
        fn now(&self) -> SystemTime {
            *self.current.read().unwrap()
        }

        fn now_utc(&self) -> DateTime<Utc> {
            DateTime::<Utc>::from(*self.current.read().unwrap())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_time_provider() {
        let provider = RealTimeProvider::new();
        let now = provider.now();
        let utc = provider.now_utc();

        // Just verify they return reasonable values
        assert!(now.duration_since(UNIX_EPOCH).unwrap().as_secs() > 0);
        assert!(utc.timestamp() > 0);
    }

    #[test]
    fn test_mock_time_provider_fixed() {
        let timestamp = 1704067200u64; // 2024-01-01 00:00:00 UTC
        let provider = MockTimeProvider::from_timestamp(timestamp);

        assert_eq!(provider.unix_timestamp(), timestamp as i64);
    }

    #[test]
    fn test_mock_time_provider_advance() {
        let timestamp = 1704067200u64;
        let provider = MockTimeProvider::from_timestamp(timestamp);

        provider.advance_secs(60);
        assert_eq!(provider.unix_timestamp(), (timestamp + 60) as i64);

        provider.advance_mins(5);
        assert_eq!(provider.unix_timestamp(), (timestamp + 60 + 300) as i64);

        provider.advance_hours(1);
        assert_eq!(
            provider.unix_timestamp(),
            (timestamp + 60 + 300 + 3600) as i64
        );
    }

    #[test]
    fn test_mock_time_provider_rewind() {
        let timestamp = 1704067200u64;
        let provider = MockTimeProvider::from_timestamp(timestamp);

        provider.rewind(Duration::from_secs(3600));
        assert_eq!(provider.unix_timestamp(), (timestamp - 3600) as i64);
    }

    #[test]
    fn test_mock_time_provider_set() {
        let provider = MockTimeProvider::new();

        provider.set_timestamp(1000);
        assert_eq!(provider.unix_timestamp(), 1000);

        provider.set_timestamp(2000);
        assert_eq!(provider.unix_timestamp(), 2000);
    }

    #[test]
    fn test_unix_timestamp_millis() {
        let timestamp = 1704067200u64;
        let provider = MockTimeProvider::from_timestamp(timestamp);

        assert_eq!(provider.unix_timestamp_millis(), (timestamp * 1000) as i64);
    }
}
