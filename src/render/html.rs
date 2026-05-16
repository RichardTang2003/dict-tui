use once_cell::sync::Lazy;
use regex::{Captures, Regex};

static SCRIPT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<script\b[^>]*>.*?</script>").expect("valid script regex"));
static STYLE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<style\b[^>]*>.*?</style>").expect("valid style regex"));
static HEAD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<head\b[^>]*>.*?</head>").expect("valid head regex"));
static HIDDEN_OPEN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<[a-z][a-z0-9]*\b[^>]*(?:display\s*:\s*none|visibility\s*:\s*hidden|hidden\b)[^>]*>"#)
        .expect("valid hidden open regex")
});
static RESOURCE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?is)<(img|audio|source|svg|canvas|iframe|link|meta)\b[^>]*>")
        .expect("valid resource regex")
});
static BR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<br\s*/?>").expect("valid br regex"));
static BLOCK_START_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)<(address|article|aside|blockquote|dd|details|div|dl|dt|figcaption|figure|footer|form|h[1-6]|header|hr|li|main|nav|ol|p|pre|section|table|tbody|td|tfoot|th|thead|tr|ul)\b[^>]*>")
        .expect("valid block start regex")
});
static BLOCK_END_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)</(address|article|aside|blockquote|dd|details|div|dl|dt|figcaption|figure|footer|form|h[1-6]|header|hr|li|main|nav|ol|p|pre|section|table|tbody|td|tfoot|th|thead|tr|ul)>")
        .expect("valid block end regex")
});
static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<[^>]+>").expect("valid tag regex"));
static MULTI_NL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{3,}").expect("valid newline regex"));
static MULTI_SPACE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[ \t]{2,}").expect("valid multi-space regex"));
static HAN_AFTER_PUNCT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"([.!?;:])\s*([\p{Han}])").expect("valid han-punct regex"));
static SENSE_SPLIT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"([^\n])\s+(\d+\.)").expect("valid sense split regex"));
static IDIOM_SPLIT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"([^\n])\s+(idiom\b)").expect("valid idiom split regex"));
static DEC_ENTITY_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"&#([0-9]{1,7});").expect("valid dec entity regex"));
static HEX_ENTITY_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"&#x([0-9a-fA-F]{1,6});").expect("valid hex entity regex"));
static CONTROL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]").expect("valid control regex"));

pub fn html_to_plain_text(raw_html: &str) -> String {
    let clean_input = CONTROL_RE.replace_all(raw_html, "");
    let no_head = HEAD_RE.replace_all(&clean_input, "");
    let no_script = SCRIPT_RE.replace_all(&no_head, "");
    let no_style = STYLE_RE.replace_all(&no_script, "");
    let no_hidden = HIDDEN_OPEN_RE.replace_all(&no_style, " ");
    let no_resources = RESOURCE_RE.replace_all(&no_hidden, " ");
    let with_breaks = BR_RE.replace_all(&no_resources, "\n");
    let with_block_starts = BLOCK_START_RE.replace_all(&with_breaks, "\n");
    let with_block_breaks = BLOCK_END_RE.replace_all(&with_block_starts, "\n");
    let stripped = TAG_RE.replace_all(&with_block_breaks, " ");
    let decoded = decode_basic_entities(&stripped);
    let bilingual_split = HAN_AFTER_PUNCT_RE.replace_all(&decoded, "$1\n$2");
    let sense_split = SENSE_SPLIT_RE.replace_all(&bilingual_split, "$1\n$2");
    let idiom_split = IDIOM_SPLIT_RE.replace_all(&sense_split, "$1\n$2");

    let normalized_lines = idiom_split
        .replace('\r', "")
        .lines()
        .map(str::trim)
        .map(|line| MULTI_SPACE_RE.replace_all(line, " ").to_string())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    let deduped = dedupe_adjacent_lines(&normalized_lines);
    MULTI_NL_RE
        .replace_all(deduped.join("\n").trim(), "\n\n")
        .to_string()
}

fn dedupe_adjacent_lines(lines: &[String]) -> Vec<String> {
    let mut output = Vec::with_capacity(lines.len());
    let mut last = "";

    for line in lines {
        let current = line.trim();
        if current.is_empty() || current == last {
            continue;
        }

        if is_noise_line(current) || is_low_value_duplicate(current, last) {
            continue;
        }

        output.push(current.to_string());
        last = current;
    }

    output
}

fn is_noise_line(line: &str) -> bool {
    let mut chars = line.chars();
    let Some(ch) = chars.next() else {
        return true;
    };
    chars.next().is_none() && ch.is_ascii_lowercase()
}

fn is_low_value_duplicate(current: &str, previous: &str) -> bool {
    current.len() <= 16
        && (current.starts_with('/') || current.starts_with('['))
        && current == previous
}

fn decode_basic_entities(text: &str) -> String {
    let named = text
        .replace("&nbsp;", " ")
        .replace("&ensp;", " ")
        .replace("&emsp;", " ")
        .replace("&thinsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'");
    let hex_decoded = HEX_ENTITY_RE.replace_all(&named, |caps: &Captures| {
        decode_entity_codepoint(&caps[1], 16)
    });
    DEC_ENTITY_RE
        .replace_all(&hex_decoded, |caps: &Captures| {
            decode_entity_codepoint(&caps[1], 10)
        })
        .to_string()
}

fn decode_entity_codepoint(raw: &str, radix: u32) -> String {
    u32::from_str_radix(raw, radix)
        .ok()
        .and_then(char::from_u32)
        .map(|ch| ch.to_string())
        .unwrap_or_default()
}
