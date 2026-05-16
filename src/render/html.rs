use once_cell::sync::Lazy;
use regex::{Captures, Regex};

static SCRIPT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<script\b[^>]*>.*?</script>").expect("valid script regex"));
static STYLE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<style\b[^>]*>.*?</style>").expect("valid style regex"));
static BR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<br\s*/?>").expect("valid br regex"));
static BLOCK_START_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)<(p|div|li|tr|h[1-6]|section|article|ul|ol|table|blockquote)\b[^>]*>")
        .expect("valid block start regex")
});
static BLOCK_END_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)</(p|div|li|tr|h[1-6]|section|article|ul|ol|table|blockquote)>")
        .expect("valid block end regex")
});
static TD_END_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)</(td|th)>").expect("valid td regex"));
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

pub fn html_to_plain_text(raw_html: &str) -> String {
    let no_script = SCRIPT_RE.replace_all(raw_html, "");
    let no_style = STYLE_RE.replace_all(&no_script, "");
    let with_breaks = BR_RE.replace_all(&no_style, "\n");
    let with_block_starts = BLOCK_START_RE.replace_all(&with_breaks, "\n");
    let with_block_breaks = BLOCK_END_RE.replace_all(&with_block_starts, "\n");
    let with_cells = TD_END_RE.replace_all(&with_block_breaks, "\t");
    let stripped = TAG_RE.replace_all(&with_cells, " ");
    let decoded = decode_basic_entities(&stripped);
    let bilingual_split = HAN_AFTER_PUNCT_RE.replace_all(&decoded, "$1\n$2");
    let sense_split = SENSE_SPLIT_RE.replace_all(&bilingual_split, "$1\n$2");
    let idiom_split = IDIOM_SPLIT_RE.replace_all(&sense_split, "$1\n$2");

    let normalized_lines = idiom_split
        .replace('\r', "")
        .lines()
        .map(str::trim)
        .map(|line| MULTI_SPACE_RE.replace_all(line, " ").to_string())
        .collect::<Vec<_>>()
        .join("\n");
    MULTI_NL_RE
        .replace_all(normalized_lines.trim(), "\n\n")
        .to_string()
}

fn decode_basic_entities(text: &str) -> String {
    let named = text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
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