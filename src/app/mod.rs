use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_endpoint: String,
    pub api_key: String,
    pub answer_language: String,
    pub system_prompt: String,
    pub model: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key: String::new(),
            answer_language: "中文".to_string(),
            model: "gpt-4o-mini".to_string(),
            system_prompt: crate::ai::prompt::default_system_prompt().to_string(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = config_file_path()?;
        if !config_path.exists() {
            return Ok(Config::default());
        }
        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("读取配置文件失败: {}", config_path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("解析配置文件失败: {}", config_path.display()))
    }

    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        let config_path = config_file_path()?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("创建配置目录失败: {}", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(self)
            .context("序列化配置失败")?;
        fs::write(&config_path, content)
            .with_context(|| format!("写入配置文件失败: {}", config_path.display()))?;
        Ok(())
    }
}

fn config_file_path() -> Result<PathBuf> {
    let mut path = dirs::config_dir()
        .context("无法获取配置目录")?;
    path.push("dict-tui");
    path.push("config.json");
    Ok(path)
}