pub struct Entry {
    pub word: String,
    pub word_lower: String,
    pub source: String,
    pub dict_idx: usize,
    pub keyword_idx: usize,
}

impl Entry {
    pub fn new(word: String, dict_idx: usize, keyword_idx: usize, source: String) -> Self {
        Self {
            word_lower: word.to_lowercase(),
            word,
            source,
            dict_idx,
            keyword_idx,
        }
    }
}