use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;

static SCRIPT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<script\b[^>]*>.*?</script>").expect("valid script regex"));

pub fn build_preview_html_file(title: &str, raw_html: &str, asset_dir: &Path) -> Result<PathBuf> {
    let base_url = Url::from_directory_path(asset_dir.canonicalize()?)
        .map_err(|_| anyhow!("无法转换资源目录为 file URL: {}", asset_dir.display()))?;
    let body_html = SCRIPT_RE.replace_all(raw_html, "");
    let css_links = collect_css_links(asset_dir)?;

    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{}</title><base href=\"{}\">{}\
         </head><body>{}</body></html>",
        escape_html_text(title),
        base_url,
        css_links,
        body_html
    );

    let mut file_path = env::temp_dir();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    file_path.push(format!("dict-tui-preview-{}-{}.html", std::process::id(), ts));

    fs::write(&file_path, html)
        .with_context(|| format!("写入网页预览文件失败: {}", file_path.display()))?;
    Ok(file_path)
}

pub fn open_in_browser(path: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .with_context(|| format!("无法调用 xdg-open 打开: {}", path.display()))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .with_context(|| format!("无法调用 open 打开: {}", path.display()))?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", &path.display().to_string()])
            .spawn()
            .with_context(|| format!("无法调用 start 打开: {}", path.display()))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err(anyhow!("当前平台不支持自动打开浏览器"))
}

fn collect_css_links(asset_dir: &Path) -> Result<String> {
    let mut css_files = Vec::new();
    for entry in fs::read_dir(asset_dir)
        .with_context(|| format!("读取资源目录失败: {}", asset_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("css")) {
            css_files.push(path);
        }
    }

    css_files.sort();

    let mut links = String::new();
    for css in css_files {
        let url = Url::from_file_path(&css)
            .map_err(|_| anyhow!("无法转换 CSS 路径为 file URL: {}", css.display()))?;
        links.push_str(&format!("<link rel=\"stylesheet\" href=\"{}\">", url));
    }
    Ok(links)
}

fn escape_html_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}