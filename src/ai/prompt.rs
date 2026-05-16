const DEFAULT_SYSTEM_PROMPT: &str = r#"你是一位专业的英语学习助手，你擅长通过网络搜索查找单词正确解释，用中文解释英文单词，并能结合传统字典释义与真实网络用例，帮助用户全面理解单词的用法、含义和文化背景。

我是一个中文用户，想要查询一个英文单词的意思。请在搜索网页后为我提供以下内容：

基础信息

单词原文和音标
主要词性和中文翻译
多来源字典释义

至少2个权威词典的定义
用简洁的中文解释，避免过于学术的表述
现代网络用法

在社交媒体（如Twitter/X、Reddit、微博英文圈）中的实际使用方式
2-3个来自真实网络语境的例句
该单词是否有新的含义或俚语用法
实用例句

至少3个来自真实语境（书籍、新闻、影视、网络）的例句
每个例句附上中文翻译和使用场景说明
近义词辨析

该单词与2-3个近义词的区别
明确使用场景的差异（正式/非正式、口语/书面等）
文化注释和常见搭配

在英语文化中的特殊含义或使用禁忌
常见短语搭配和固定表达
请用Markdown格式清晰呈现上述内容，对于网络用法的部分，请特别标注你认为最能体现现代英语发展趋势的例子。"#;

pub fn build_user_prompt_with_context(word: &str, answer_lang: &str, context: &str) -> String {
    let context_section = if context.is_empty() {
        String::new()
    } else {
        format!("参考词典释义：\n{}\n\n", context)
    };

    format!(
        r#"请查询英文单词 "{}" 并用{}回答。
{}
请搜索网页后提供：
1. 单词原文、音标、主要词性和中文翻译
2. 至少2个权威词典的释义
3. 2-3个真实网络语境例句
4. 至少3个实用例句（附中文翻译和使用场景）
5. 近义词辨析
6. 文化注释和常见搭配

请用Markdown格式呈现。"#,
        word, answer_lang, context_section
    )
}

pub fn default_system_prompt() -> &'static str {
    DEFAULT_SYSTEM_PROMPT
}
