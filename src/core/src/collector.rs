//! Merges dropped item stacks and re-splits them into max-size stacks.

use std::collections::HashMap;
use std::hash::Hash;

/// Item counts merged by key, kept in first-seen order so drops are deterministic.
#[derive(Clone, Debug)]
pub struct MergedStacks<K> {
    entries: Vec<(K, u64)>,
    index: HashMap<K, usize>,
}

impl<K: Eq + Hash + Clone> Default for MergedStacks<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash + Clone> MergedStacks<K> {
    /// Creates an empty collector.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Adds `count` items of `key`. Zero counts are ignored.
    pub fn add(&mut self, key: K, count: u32) {
        if count == 0 {
            return;
        }
        match self.index.get(&key) {
            Some(&i) => self.entries[i].1 = self.entries[i].1.saturating_add(u64::from(count)),
            None => {
                self.index.insert(key.clone(), self.entries.len());
                self.entries.push((key, u64::from(count)));
            }
        }
    }

    /// Whether nothing has been collected.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Discards everything collected.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
    }

    /// Empties the collector, returning `(key, count)` stacks in insertion order, each at most
    /// `max_stack_fn(&key).max(1)` items.
    pub fn drain_split<F: FnMut(&K) -> u32>(&mut self, mut max_stack_fn: F) -> Vec<(K, u32)> {
        self.index.clear();
        let mut out = Vec::new();
        for (key, mut total) in self.entries.drain(..) {
            let max = u64::from(max_stack_fn(&key).max(1));
            while total > 0 {
                let chunk = total.min(max);
                // chunk <= max <= u32::MAX
                out.push((key.clone(), chunk as u32));
                total -= chunk;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_in_insertion_order_and_splits() {
        let mut c = MergedStacks::new();
        c.add("cobble", 40);
        c.add("coal", 3);
        c.add("cobble", 40);
        c.add("pearl", 0);
        c.add("pearl", 20);
        let out = c.drain_split(|k| if *k == "pearl" { 16 } else { 64 });
        assert_eq!(
            out,
            vec![
                ("cobble", 64),
                ("cobble", 16),
                ("coal", 3),
                ("pearl", 16),
                ("pearl", 4)
            ]
        );
        assert!(c.is_empty());
        // Index cleared too: re-adding starts fresh.
        c.add("coal", 1);
        assert_eq!(c.drain_split(|_| 64), vec![("coal", 1)]);
    }

    #[test]
    fn zero_max_stack_means_one() {
        let mut c = MergedStacks::new();
        c.add(7u8, 3);
        assert_eq!(c.drain_split(|_| 0), vec![(7, 1), (7, 1), (7, 1)]);
    }

    #[test]
    fn totals_past_u32_split_safely() {
        let mut c = MergedStacks::new();
        c.add(1u8, u32::MAX);
        c.add(1u8, u32::MAX);
        let out = c.drain_split(|_| u32::MAX);
        assert_eq!(out, vec![(1, u32::MAX), (1, u32::MAX)]);
    }

    #[test]
    fn zero_adds_and_clear() {
        let mut c = MergedStacks::<u8>::default();
        c.add(1, 0);
        assert!(c.is_empty());
        c.add(1, 5);
        assert!(!c.is_empty());
        c.clear();
        assert!(c.is_empty());
        assert!(c.drain_split(|_| 64).is_empty());
    }
}
