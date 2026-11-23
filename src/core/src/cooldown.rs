//! Ultimine cooldown math.
//!
//! Times are epoch milliseconds; cooldowns are game ticks (50 ms each).
//! A `last_used_ms` of `0` means "never used".

use std::collections::HashMap;
use std::hash::Hash;

const MS_PER_TICK: u64 = 50;

/// Whether a player who last ultimined at `last_used_ms` is still on cooldown at `now_ms`.
/// A clock that went backwards (`now_ms < last_used_ms`) counts as zero elapsed time.
pub fn is_on_cooldown(now_ms: u64, last_used_ms: u64, cooldown_ticks: u64) -> bool {
    if cooldown_ticks == 0 || last_used_ms == 0 {
        return false;
    }
    now_ms.saturating_sub(last_used_ms) < cooldown_ticks.saturating_mul(MS_PER_TICK)
}

/// Fraction of the cooldown that has elapsed, `0.0..=1.0`; `1.0` means ready.
pub fn remaining_progress(now_ms: u64, last_used_ms: u64, cooldown_ticks: u64) -> f32 {
    if cooldown_ticks == 0 || last_used_ms == 0 {
        return 1.0;
    }
    let elapsed_ticks = now_ms.saturating_sub(last_used_ms) / MS_PER_TICK;
    (elapsed_ticks as f32 / cooldown_ticks as f32).clamp(0.0, 1.0)
}

/// Cooldown left at `now_ms` in milliseconds; `0` means ready.
pub fn remaining_ms(now_ms: u64, last_used_ms: u64, cooldown_ticks: u64) -> u64 {
    if cooldown_ticks == 0 || last_used_ms == 0 {
        return 0;
    }
    cooldown_ticks
        .saturating_mul(MS_PER_TICK)
        .saturating_sub(now_ms.saturating_sub(last_used_ms))
}

/// Whether a cooldown that ends at `ready_at_ms` (local clock) is still running at `now_ms`.
pub fn is_on_cooldown_until(now_ms: u64, ready_at_ms: u64) -> bool {
    ready_at_ms > now_ms
}

/// Like [`remaining_progress`], but from a local `ready_at_ms` deadline instead of a server timestamp.
pub fn remaining_progress_until(now_ms: u64, ready_at_ms: u64, cooldown_ticks: u64) -> f32 {
    if cooldown_ticks == 0 || ready_at_ms <= now_ms {
        return 1.0;
    }
    let total_ms = cooldown_ticks.saturating_mul(MS_PER_TICK);
    let rem_ms = ready_at_ms.saturating_sub(now_ms).min(total_ms);
    let elapsed_ticks = total_ms.saturating_sub(rem_ms) / MS_PER_TICK;
    (elapsed_ticks as f32 / cooldown_ticks as f32).clamp(0.0, 1.0)
}

/// Per-player `last_used_ms` store.
#[derive(Debug)]
pub struct CooldownStore<K: Eq + Hash> {
    last_used: HashMap<K, u64>,
}

impl<K: Eq + Hash> Default for CooldownStore<K> {
    fn default() -> Self {
        Self {
            last_used: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash> CooldownStore<K> {
    /// Last use time for `key`, or `0` (never used).
    pub fn get(&self, key: &K) -> u64 {
        self.last_used.get(key).copied().unwrap_or(0)
    }

    /// Records that `key` ultimined at `last_used_ms`.
    pub fn set(&mut self, key: K, last_used_ms: u64) {
        self.last_used.insert(key, last_used_ms);
    }

    /// Forgets `key` (e.g. on logout).
    pub fn remove(&mut self, key: &K) {
        self.last_used.remove(key);
    }

    /// Forgets every key.
    pub fn clear(&mut self) {
        self.last_used.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_cooldown_or_never_used_is_ready() {
        assert!(!is_on_cooldown(1_000, 900, 0));
        assert!(!is_on_cooldown(1_000, 0, 100));
        assert_eq!(remaining_progress(1_000, 900, 0), 1.0);
        assert_eq!(remaining_progress(1_000, 0, 100), 1.0);
    }

    #[test]
    fn cooldown_boundary() {
        // 10 ticks = 500 ms.
        assert!(is_on_cooldown(10_499, 10_000, 10));
        assert!(!is_on_cooldown(10_500, 10_000, 10));
        assert!(is_on_cooldown(10_000, 10_000, 10));
    }

    #[test]
    fn clock_going_backwards_is_safe() {
        assert!(is_on_cooldown(5_000, 10_000, 10));
        assert_eq!(remaining_progress(5_000, 10_000, 10), 0.0);
    }

    #[test]
    fn huge_cooldown_saturates() {
        assert!(is_on_cooldown(u64::MAX, 1, u64::MAX));
        assert!(remaining_progress(u64::MAX, 1, u64::MAX) < 0.1);
    }

    #[test]
    fn progress_counts_whole_ticks() {
        assert_eq!(remaining_progress(10_000, 10_000, 10), 0.0);
        assert_eq!(remaining_progress(10_049, 10_000, 10), 0.0);
        assert_eq!(remaining_progress(10_250, 10_000, 10), 0.5);
        assert_eq!(remaining_progress(99_999, 10_000, 10), 1.0);
    }

    #[test]
    fn remaining_ms_values() {
        assert_eq!(remaining_ms(1_000, 900, 0), 0);
        assert_eq!(remaining_ms(1_000, 0, 10), 0);
        assert_eq!(remaining_ms(10_000, 10_000, 10), 500);
        assert_eq!(remaining_ms(10_200, 10_000, 10), 300);
        assert_eq!(remaining_ms(10_500, 10_000, 10), 0);
        assert_eq!(remaining_ms(99_999, 10_000, 10), 0);
        // Clock went backwards: full cooldown remains.
        assert_eq!(remaining_ms(5_000, 10_000, 10), 500);
        assert_eq!(remaining_ms(1, 1, u64::MAX), u64::MAX);
    }

    #[test]
    fn on_cooldown_until_boundary() {
        assert!(is_on_cooldown_until(999, 1_000));
        assert!(!is_on_cooldown_until(1_000, 1_000));
        assert!(!is_on_cooldown_until(1_001, 1_000));
        assert!(!is_on_cooldown_until(0, 0));
    }

    #[test]
    fn progress_until_counts_whole_ticks() {
        // 10 ticks = 500 ms, ready at 10_500.
        assert_eq!(remaining_progress_until(10_000, 10_500, 0), 1.0);
        assert_eq!(remaining_progress_until(10_500, 10_500, 10), 1.0);
        assert_eq!(remaining_progress_until(11_000, 10_500, 10), 1.0);
        assert_eq!(remaining_progress_until(10_000, 10_500, 10), 0.0);
        assert_eq!(remaining_progress_until(10_049, 10_500, 10), 0.0);
        assert_eq!(remaining_progress_until(10_250, 10_500, 10), 0.5);
        assert_eq!(remaining_progress_until(10_499, 10_500, 10), 0.9);
        // Deadline beyond the cooldown length clamps to no progress.
        assert_eq!(remaining_progress_until(0, 1_000_000, 10), 0.0);
        assert!(remaining_progress_until(0, u64::MAX, u64::MAX) < 0.1);
    }

    #[test]
    fn store_roundtrip() {
        let mut s = CooldownStore::<u32>::default();
        assert_eq!(s.get(&1), 0);
        s.set(1, 123);
        s.set(2, 456);
        assert_eq!(s.get(&1), 123);
        s.remove(&1);
        assert_eq!(s.get(&1), 0);
        assert_eq!(s.get(&2), 456);
        s.clear();
        assert_eq!(s.get(&2), 0);
    }
}
