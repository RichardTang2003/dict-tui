const DEFAULT_SYSTEM_PROMPT: &str = r#"你是一位专业的英语学习助手，面向中文用户解释英文单词、短语和新兴表达。

输出必须适合终端 TUI 阅读：
不要使用 Markdown。
不要使用 #、**、*、-、>、`、表格、代码块、Markdown 链接或脚注。
不要使用项目符号列表。
使用纯文本短段落，每段用空行分隔。
需要分节时，直接写简短中文标题，例如“核心含义”“真实例句”“最新用法”“来源”，标题后换行即可。
来源请用普通文本 URL 或来源名称，不要写成 Markdown 链接。

回答时请优先关注基本含义、词性、发音、中文解释、常见搭配、语域差异和近义词辨析。
当问题涉及新词、流行语、网络表达或近期用法时，结合最新网页信息说明真实使用场景。
如果使用了网页搜索，请给出来源名称或链接，避免编造引用。
中文解释为主，英文例句附中文翻译。"#;

pub fn build_user_prompt_with_context(word: &str, answer_lang: &str, context: &str) -> String {
    let context_section = if context.trim().is_empty() {
        String::new()
    } else {
        format!("参考词典内容：\n{}\n\n", context)
    };

    format!(
        r#"请查询英文单词、短语或表达“{}”，并用{}回答。

{}请按纯文本格式提供以下内容，不要使用 Markdown：

核心含义
说明主要意思、词性、发音和中文解释。

可靠释义
概括权威词典或可靠资料中的释义。

真实例句
提供真实语境例句，附中文翻译和使用场景。

最新用法
如果这是新词、网络流行语或近期用法，请说明最新使用趋势。

辨析与搭配
说明近义词差异、常见搭配和文化注意事项。

来源
如果使用了网页搜索，请列出普通文本来源名称或 URL。"#,
        word, answer_lang, context_section
    )
}

pub fn default_system_prompt() -> &'static str {
    DEFAULT_SYSTEM_PROMPT
}
