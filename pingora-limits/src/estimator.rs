// Copyright 2024 Cloudflare, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The estimator module contains a Count-Min Sketch type to help estimate the frequency of an item.

use crate::hash;
use crate::RandomState;
use std::hash::Hash;
use std::sync::atomic::{AtomicIsize, Ordering};

/// An implementation of a lock-free count–min sketch estimator. See the [wikipedia] page for more
/// information.
///
/// [wikipedia]: https://en.wikipedia.org/wiki/Count%E2%80%93min_sketch
pub struct Estimator {
    estimator: Box<[(Box<[AtomicIsize]>, RandomState)]>,
}

impl Estimator {
    /// Create a new `Estimator` with the given amount of hashes and columns (slots).
    pub fn new(hashes: usize, slots: usize) -> Self {
        let mut estimator = Vec::with_capacity(hashes);
        for _ in 0..hashes {
            let mut slot = Vec::with_capacity(slots);
            for _ in 0..slots {
                slot.push(AtomicIsize::new(0));
            }
            estimator.push((slot.into_boxed_slice(), RandomState::new()));
        }

        Estimator {
            estimator: estimator.into_boxed_slice(),
        }
    }

    /// Increment `key` by the value given. Return the new estimated value as a result.
    /// Note: overflow can happen. When some of the internal counters overflow, a negative number
    /// will be returned. It is up to the caller to catch and handle this case.
    pub fn incr<T: Hash>(&self, key: T, value: isize) -> isize {
        let mut min = isize::MAX;
        for (slot, hasher) in self.estimator.iter() {
            let hash = hash(&key, hasher) as usize;
            let counter = &slot[hash % slot.len()];
            // Overflow is allowed for simplicity
            let current = counter.fetch_add(value, Ordering::Relaxed);
            min = std::cmp::min(min, current + value);
        }
        min
    }

    /// Decrement `key` by the value given.
    pub fn decr<T: Hash>(&self, key: T, value: isize) {
        for (slot, hasher) in self.estimator.iter() {
            let hash = hash(&key, hasher) as usize;
            let counter = &slot[hash % slot.len()];
            counter.fetch_sub(value, Ordering::Relaxed);
        }
    }

    /// Get the estimated frequency of `key`.
    pub fn get<T: Hash>(&self, key: T) -> isize {
        let mut min = isize::MAX;
        for (slot, hasher) in self.estimator.iter() {
            let hash = hash(&key, hasher) as usize;
            let counter = &slot[hash % slot.len()];
            let current = counter.load(Ordering::Relaxed);
            min = std::cmp::min(min, current);
        }
        min
    }

    /// Reset all values inside this `Estimator`.
    pub fn reset(&self) {
        for (slot, _) in self.estimator.iter() {
            for counter in slot.iter() {
                counter.store(0, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incr() {
        let est = Estimator::new(8, 8);
        let v = est.incr("a", 1);
        assert_eq!(v, 1);
        let v = est.incr("b", 1);
        assert_eq!(v, 1);
        let v = est.incr("a", 2);
        assert_eq!(v, 3);
        let v = est.incr("b", 2);
        assert_eq!(v, 3);
    }

    #[test]
    fn desc() {
        let est = Estimator::new(8, 8);
        est.incr("a", 3);
        est.incr("b", 3);
        est.decr("a", 1);
        est.decr("b", 1);
        assert_eq!(est.get("a"), 2);
        assert_eq!(est.get("b"), 2);
    }

    #[test]
    fn get() {
        let est = Estimator::new(8, 8);
        est.incr("a", 1);
        est.incr("a", 2);
        est.incr("b", 1);
        est.incr("b", 2);
        assert_eq!(est.get("a"), 3);
        assert_eq!(est.get("b"), 3);
    }
}
