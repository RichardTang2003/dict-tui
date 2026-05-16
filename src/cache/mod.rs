pub mod query_cache;
pub mod definition_cache;

pub use query_cache::QueryResultCache;
pub use definition_cache::DefinitionCache;

pub const SEARCH_CACHE_CAPACITY: usize = 2048;
pub const DEFINITION_CACHE_CAPACITY: usize = 4096;