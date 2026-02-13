use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rust_mdict::Mdx;

const DEFAULT_DICT_DIR: &str = "./dictionary";

#[derive(Debug)]
pub struct Entry {
    pub word: String,
    pub word_lower: String,
    pub source: String,
    pub dict_idx: usize,
    pub keyword_idx: usize,
}

pub struct DictionarySource {
    pub mdx: Mdx,
    pub keywords: Vec<rust_mdict::KeyWordItem>,
    pub asset_dir: PathBuf,
}

#[derive(Default)]
pub struct DictionaryStore {
    pub entries: Vec<Entry>,
    pub sources: Vec<DictionarySource>,
}

impl DictionaryStore {
    pub fn load() -> Result<Self> {
        Self::load_from_dir(Path::new(DEFAULT_DICT_DIR))
    }

    pub fn load_from_dir(dict_dir: &Path) -> Result<Self> {
        if !dict_dir.is_dir() {
            bail!("词典目录不存在: {}", dict_dir.display());
        }

        let mdx_files = collect_mdx_files(dict_dir)?;
        if mdx_files.is_empty() {
            bail!("词典目录 {} 下没有 .mdx 文件", dict_dir.display());
        }

        let mut entries = Vec::new();
        let mut sources = Vec::new();
        let mut load_errors = Vec::new();

        for mdx_path in mdx_files {
            eprintln!("加载词典: {}", mdx_path.display());
            let source = dictionary_name_from_folder(dict_dir, &mdx_path);

            let source_index = sources.len();
            match load_single_mdx(&mdx_path, &source) {
                Ok((loaded_source, mut loaded_entries)) => {
                    for entry in &mut loaded_entries {
                        entry.dict_idx = source_index;
                    }
                    entries.append(&mut loaded_entries);
                    sources.push(loaded_source);
                }
                Err(err) => {
                    load_errors.push(format!("{}: {err}", mdx_path.display()));
                    eprintln!("跳过词典 {}，原因: {err}", mdx_path.display());
                }
            }
        }

        if entries.is_empty() {
            if load_errors.is_empty() {
                bail!("没有可用词条，词典文件可能为空");
            }
            bail!(
                "所有词典加载失败:\n{}",
                load_errors
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }

        entries.sort_by(|a, b| a.word_lower.cmp(&b.word_lower));
        Ok(Self { entries, sources })
    }

    pub fn search(&self, needle: &str, candidates: Option<&[usize]>) -> Vec<usize> {
        if needle.is_empty() {
            return Vec::new();
        }

        let mut exact = Vec::new();
        let mut prefix = Vec::new();
        let mut contains = Vec::new();

        match candidates {
            Some(indexes) => {
                for &index in indexes {
                    let entry = &self.entries[index];
                    if entry.word_lower == needle {
                        exact.push(index);
                    } else if entry.word_lower.starts_with(needle) {
                        prefix.push(index);
                    } else if entry.word_lower.contains(needle) {
                        contains.push(index);
                    }
                }
            }
            None => {
                for (index, entry) in self.entries.iter().enumerate() {
                    if entry.word_lower == needle {
                        exact.push(index);
                    } else if entry.word_lower.starts_with(needle) {
                        prefix.push(index);
                    } else if entry.word_lower.contains(needle) {
                        contains.push(index);
                    }
                }
            }
        }

        let mut merged = Vec::with_capacity(exact.len() + prefix.len() + contains.len());
        merged.extend(exact);
        merged.extend(prefix);
        merged.extend(contains);
        merged
    }

    pub fn fetch_definition(&mut self, entry_idx: usize) -> Result<String> {
        let entry = self
            .entries
            .get(entry_idx)
            .with_context(|| format!("无效词条索引: {}", entry_idx))?;
        let source = self
            .sources
            .get_mut(entry.dict_idx)
            .with_context(|| format!("无效词典索引: {}", entry.dict_idx))?;
        let keyword = source
            .keywords
            .get(entry.keyword_idx)
            .with_context(|| format!("无效关键词索引: {}", entry.keyword_idx))?;

        if let Some(result) = source.mdx.fetch(keyword) {
            return Ok(result.definition);
        }
        bail!("无法读取词条定义: {}", entry.word)
    }

    pub fn entry_web_context(&self, entry_idx: usize) -> Result<(String, PathBuf)> {
        let entry = self
            .entries
            .get(entry_idx)
            .with_context(|| format!("无效词条索引: {}", entry_idx))?;
        let source = self
            .sources
            .get(entry.dict_idx)
            .with_context(|| format!("无效词典索引: {}", entry.dict_idx))?;
        Ok((entry.word.clone(), source.asset_dir.clone()))
    }
}

fn load_single_mdx(path: &Path, source: &str) -> Result<(DictionarySource, Vec<Entry>)> {
    let mdx = Mdx::new(path).with_context(|| format!("打开词典失败: {}", path.display()))?;
    let keywords = mdx.keyword_list().to_vec();

    let mut entries = Vec::with_capacity(keywords.len());
    for (idx, keyword) in keywords.iter().enumerate() {
        let word = keyword.key_text.trim().to_string();
        if word.is_empty() {
            continue;
        }
        entries.push(Entry {
            word_lower: word.to_lowercase(),
            word,
            source: source.to_string(),
            dict_idx: 0,
            keyword_idx: idx,
        });
    }

    let asset_dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    Ok((
        DictionarySource {
            mdx,
            keywords,
            asset_dir,
        },
        entries,
    ))
}

fn collect_mdx_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)
            .with_context(|| format!("读取目录失败: {}", current.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let is_mdx = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mdx"));
            if is_mdx {
                files.push(path);
            }
        }
    }

    files.sort();
    Ok(files)
}

fn dictionary_name_from_folder(root: &Path, mdx_path: &Path) -> String {
    if let Ok(relative) = mdx_path.strip_prefix(root) {
        let mut components = relative.components();
        if let Some(first) = components.next() {
            let first_str = first.as_os_str().to_string_lossy().trim().to_string();
            if !first_str.is_empty() {
                return first_str;
            }
        }
    }

    mdx_path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            mdx_path
                .file_stem()
                .and_then(|name| name.to_str())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "unknown".to_string())
}
