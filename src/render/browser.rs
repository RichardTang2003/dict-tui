use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, path::Component};

use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use rust_mdict::Mdd;
use url::Url;

static SCRIPT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<script\b[^>]*>.*?</script>").expect("valid script regex"));
static RESOURCE_ATTR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\b(?:src|href|data-src|data-href)\s*=\s*["']([^"']+)["']"#)
        .expect("valid resource attr regex")
});

pub fn build_preview_html_file(
    title: &str,
    raw_html: &str,
    asset_dir: &Path,
    allow_javascript: bool,
) -> Result<PathBuf> {
    let asset_dir = absolute_path(asset_dir)?;
    let base_url = Url::from_directory_path(&asset_dir)
        .map_err(|_| anyhow!("无法转换资源目录为 file URL: {}", asset_dir.display()))?;
    let preview_asset_dir = create_preview_asset_dir()?;
    let mut extra_head = collect_mdd_resources(raw_html, &asset_dir, &preview_asset_dir)?;
    let raw_html = rewrite_mdd_resource_refs(raw_html, &asset_dir, &preview_asset_dir)?;
    let body_html = if allow_javascript {
        raw_html.to_string()
    } else {
        SCRIPT_RE.replace_all(&raw_html, "").to_string()
    };
    let css_links = collect_css_links(&asset_dir)?;
    extra_head.insert_str(0, &css_links);

    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{}</title><base href=\"{}\">{}\
         </head><body>{}</body></html>",
        escape_html_text(title),
        base_url,
        extra_head,
        body_html
    );

    let mut file_path = env::temp_dir();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    file_path.push(format!(
        "dict-tui-preview-{}-{}.html",
        std::process::id(),
        ts
    ));

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
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("css"))
        {
            css_files.push(path);
        }
    }

    css_files.sort();

    let mut links = String::new();
    for css in css_files {
        let css = absolute_path(&css)?;
        let Ok(url) = Url::from_file_path(&css) else {
            continue;
        };
        links.push_str(&format!("<link rel=\"stylesheet\" href=\"{}\">", url));
    }
    Ok(links)
}

fn create_preview_asset_dir() -> Result<PathBuf> {
    let mut dir = env::temp_dir();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    dir.push(format!(
        "dict-tui-preview-assets-{}-{}",
        std::process::id(),
        ts
    ));
    fs::create_dir_all(&dir).with_context(|| format!("创建预览资源目录失败: {}", dir.display()))?;
    Ok(dir)
}

fn collect_mdd_resources(raw_html: &str, asset_dir: &Path, output_dir: &Path) -> Result<String> {
    let mdd_files = collect_mdd_files(asset_dir)?;
    if mdd_files.is_empty() {
        return Ok(String::new());
    }

    let references = referenced_resources(raw_html);
    let mut head = String::new();

    for mdd_file in mdd_files {
        let mut mdd = Mdd::new(&mdd_file)
            .with_context(|| format!("打开 MDD 资源包失败: {}", mdd_file.display()))?;

        let css_keys = mdd
            .resource_keys()
            .into_iter()
            .filter(|key| key.to_ascii_lowercase().ends_with(".css"))
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        for key in css_keys {
            if let Some(path) = extract_mdd_resource(&mut mdd, &key, output_dir)? {
                if let Ok(url) = Url::from_file_path(&path) {
                    head.push_str(&format!("<link rel=\"stylesheet\" href=\"{}\">", url));
                }
            }
        }

        for reference in &references {
            for candidate in resource_key_candidates(reference) {
                if extract_mdd_resource(&mut mdd, &candidate, output_dir)?.is_some() {
                    break;
                }
            }
        }
    }

    Ok(head)
}

fn rewrite_mdd_resource_refs(
    raw_html: &str,
    asset_dir: &Path,
    output_dir: &Path,
) -> Result<String> {
    let mdd_files = collect_mdd_files(asset_dir)?;
    if mdd_files.is_empty() {
        return Ok(raw_html.to_string());
    }

    let references = referenced_resources(raw_html);
    let mut replacements = HashMap::new();

    for reference in references {
        let Some(path) = find_extracted_resource(&reference, output_dir) else {
            continue;
        };
        if let Ok(url) = Url::from_file_path(path) {
            replacements.insert(reference, url.to_string());
        }
    }

    if replacements.is_empty() {
        return Ok(raw_html.to_string());
    }

    Ok(RESOURCE_ATTR_RE
        .replace_all(raw_html, |caps: &regex::Captures| {
            let full = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
            let value = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let Some(replacement) = replacements.get(value) else {
                return full.to_string();
            };
            full.replacen(value, replacement, 1)
        })
        .to_string())
}

fn collect_mdd_files(asset_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(asset_dir)
        .with_context(|| format!("读取资源目录失败: {}", asset_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("mdd"))
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn referenced_resources(raw_html: &str) -> Vec<String> {
    let mut resources = RESOURCE_ATTR_RE
        .captures_iter(raw_html)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().trim().to_string()))
        .filter(|value| is_local_resource_ref(value))
        .collect::<Vec<_>>();
    resources.sort();
    resources.dedup();
    resources
}

fn is_local_resource_ref(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    !value.is_empty()
        && !value.starts_with('#')
        && !lower.starts_with("http://")
        && !lower.starts_with("https://")
        && !lower.starts_with("data:")
        && !lower.starts_with("mailto:")
        && !lower.starts_with("javascript:")
}

fn resource_key_candidates(reference: &str) -> Vec<String> {
    let clean = reference
        .split(['?', '#'])
        .next()
        .unwrap_or(reference)
        .trim()
        .trim_start_matches("./");
    let slash = clean.replace('\\', "/");
    let backslash = clean.replace('/', "\\");
    let mut candidates = vec![
        clean.to_string(),
        slash.clone(),
        backslash.clone(),
        format!("\\{}", backslash.trim_start_matches('\\')),
        format!("/{}", slash.trim_start_matches('/')),
    ];
    candidates.sort();
    candidates.dedup();
    candidates
}

fn extract_mdd_resource(mdd: &mut Mdd, key: &str, output_dir: &Path) -> Result<Option<PathBuf>> {
    let Some(bytes) = mdd.locate_raw(key) else {
        return Ok(None);
    };
    let path = output_path_for_resource(output_dir, key)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("创建 MDD 资源目录失败: {}", parent.display()))?;
    }
    fs::write(&path, bytes).with_context(|| format!("写入 MDD 资源失败: {}", path.display()))?;
    Ok(Some(path))
}

fn find_extracted_resource(reference: &str, output_dir: &Path) -> Option<PathBuf> {
    resource_key_candidates(reference)
        .into_iter()
        .filter_map(|candidate| output_path_for_resource(output_dir, &candidate).ok())
        .find(|path| path.exists())
}

fn output_path_for_resource(output_dir: &Path, key: &str) -> Result<PathBuf> {
    let clean = key
        .split(['?', '#'])
        .next()
        .unwrap_or(key)
        .trim()
        .trim_start_matches(['\\', '/']);
    let mut path = output_dir.to_path_buf();
    for component in Path::new(clean).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            _ => continue,
        }
    }
    Ok(path)
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir().context("无法获取当前目录")?.join(path))
    }
}

fn escape_html_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
