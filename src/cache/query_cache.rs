use std::collections::{hash_map::Entry, HashMap, VecDeque};

use crate::dict::DictionaryStore;

pub struct QueryResultCache {
    map: HashMap<String, Vec<usize>>,
    order: VecDeque<String>,
    capacity: usize,
}

impl QueryResultCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    pub fn query(&mut self, dict: &DictionaryStore, query: &str) -> Vec<usize> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }

        if let Some(result) = self.map.get(&needle) {
            return result.clone();
        }

        let cached_prefix_result = self.find_longest_prefix_result(&needle);
        let result = dict.search(&needle, cached_prefix_result.as_deref());
        self.insert(needle, result.clone());
        result
    }

    fn find_longest_prefix_result(&self, needle: &str) -> Option<Vec<usize>> {
        for (idx, _) in needle.char_indices().rev() {
            let prefix = &needle[..idx];
            if prefix.is_empty() {
                break;
            }
            if let Some(result) = self.map.get(prefix) {
                return Some(result.clone());
            }
        }
        None
    }

    fn insert(&mut self, key: String, value: Vec<usize>) {
        match self.map.entry(key.clone()) {
            Entry::Occupied(mut entry) => {
                entry.insert(value);
            }
            Entry::Vacant(entry) => {
                self.order.push_back(key);
                entry.insert(value);

                while self.order.len() > self.capacity {
                    if let Some(oldest) = self.order.pop_front() {
                        self.map.remove(&oldest);
                    }
                }
            }
        }
    }
}
