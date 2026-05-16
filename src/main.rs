mod app;
mod cache;
mod dict;
mod ai;
mod ui;
mod render;

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::app::Config;
use crate::dict::DictionaryStore;
use crate::ui::run_search;

fn run() -> Result<()> {
    let config = Config::load().unwrap_or_default();
    println!("正在扫描并缓存词典...");
    let mut store = load_store_with_prompt()?;
    println!(
        "缓存完成，共加载 {} 条词条，来自 {} 本词典。",
        store.entries.len(),
        store.sources.len()
    );
    println!("进入动态搜索：输入/删除字符会实时查询。");
    println!("按 Esc 退出程序。");

    run_search(&mut store, config)
}

fn load_store_with_prompt() -> Result<DictionaryStore> {
    match DictionaryStore::load() {
        Ok(store) => return Ok(store),
        Err(err) => {
            eprintln!("默认目录 ./dictionary 加载失败: {err}");
            eprintln!("请手动输入词典目录路径。输入 exit 可退出。");
        }
    }

    let stdin = io::stdin();
    loop {
        print!("词典路径> ");
        io::stdout().flush()?;

        let mut input = String::new();
        if stdin.read_line(&mut input)? == 0 {
            bail!("未读取到词典路径输入");
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
            bail!("用户取消输入词典路径");
        }

        let path = PathBuf::from(input);
        match DictionaryStore::load_from_dir(&path) {
            Ok(store) => return Ok(store),
            Err(err) => {
                eprintln!("路径 {} 加载失败: {err}", path.display());
            }
        }
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("程序异常退出: {err:#}");
        std::process::exit(1);
    }
}