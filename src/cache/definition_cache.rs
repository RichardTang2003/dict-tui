use std::collections::{hash_map::Entry, HashMap, VecDeque};

use anyhow::{Context, Result};

use crate::dict::DictionaryStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DefinitionKey {
    dict_idx: usize,
    keyword_idx: usize,
}

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
        match self.map.entry(key) {
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
