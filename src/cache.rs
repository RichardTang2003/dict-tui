use std::collections::{HashMap, VecDeque};

use anyhow::{Context, Result};

use crate::dictionary::DictionaryStore;

pub const SEARCH_CACHE_CAPACITY: usize = 2048;
pub const DEFINITION_CACHE_CAPACITY: usize = 4096;

#[derive(Debug)]
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
        if self.map.contains_key(&key) {
            self.map.insert(key, value);
            return;
        }

        self.order.push_back(key.clone());
        self.map.insert(key, value);

        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DefinitionKey {
    dict_idx: usize,
    keyword_idx: usize,
}

#[derive(Debug)]
pub struct DefinitionCache {
    map: HashMap<DefinitionKey, String>,
    order: VecDeque<DefinitionKey>,
    capacity: usize,
}

impl DefinitionCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    pub fn get_or_load(&mut self, dict: &mut DictionaryStore, entry_idx: usize) -> Result<String> {
        let entry = dict
            .entries
            .get(entry_idx)
            .with_context(|| format!("无效词条索引: {}", entry_idx))?;
        let key = DefinitionKey {
            dict_idx: entry.dict_idx,
            keyword_idx: entry.keyword_idx,
        };

        if let Some(definition) = self.map.get(&key) {
            return Ok(definition.clone());
        }

        let definition = dict.fetch_definition(entry_idx)?;
        self.insert(key, definition.clone());
        Ok(definition)
    }

    fn insert(&mut self, key: DefinitionKey, value: String) {
        if self.map.contains_key(&key) {
            self.map.insert(key, value);
            return;
        }

        self.order.push_back(key);
        self.map.insert(key, value);
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
    }
}
