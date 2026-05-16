use std::collections::HashSet;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::app::Config;

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: MessageContent,
}

#[derive(Debug, Deserialize)]
struct MessageContent {
    content: String,
}

#[derive(Debug, Serialize)]
struct ResponsesRequest {
    model: String,
    instructions: String,
    input: String,
    tools: Vec<ResponseTool>,
    tool_choice: String,
    max_output_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ResponseTool {
    r#type: &'static str,
}

#[derive(Debug, Deserialize)]
struct ResponsesResponse {
    output_text: Option<String>,
    #[serde(default)]
    output: Vec<ResponseOutputItem>,
}

#[derive(Debug, Deserialize)]
struct ResponseOutputItem {
    #[serde(default)]
    content: Vec<ResponseContent>,
}

#[derive(Debug, Deserialize)]
struct ResponseContent {
    text: Option<String>,
    #[serde(default)]
    annotations: Vec<ResponseAnnotation>,
}

#[derive(Debug, Deserialize)]
struct ResponseAnnotation {
    url: Option<String>,
    title: Option<String>,
}

pub struct AiClient {
    client: Client,
    config: Config,
}

impl AiClient {
    pub fn new(config: Config) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("创建 HTTP 客户端失败");
        Self { client, config }
    }

    pub async fn query_with_context(&self, word: &str, context: &str) -> Result<String> {
        if self.config.enable_web_search {
            self.query_with_web_search(word, context).await
        } else {
            self.query_chat_completions(word, context).await
        }
    }

    async fn query_chat_completions(&self, word: &str, context: &str) -> Result<String> {
        let url = self.chat_completions_url();
        let system = self.config.system_prompt.clone();
        let user = crate::ai::prompt::build_user_prompt_with_context(
            word,
            &self.config.answer_language,
            context,
        );

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: system,
                },
                Message {
                    role: "user".to_string(),
                    content: user,
                },
            ],
            max_tokens: 4096,
            temperature: 0.7,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .with_context(|| format!("AI 请求失败: {url}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("AI API 返回错误 ({status}): {body}");
        }

        let chat_response: ChatResponse = response.json().await.context("解析 AI 响应失败")?;
        let content = chat_response
            .choices
            .first()
            .context("AI 响应为空")?
            .message
            .content
            .clone();

        Ok(content)
    }

    async fn query_with_web_search(&self, word: &str, context: &str) -> Result<String> {
        let url = self.responses_url();
        let request = ResponsesRequest {
            model: self.config.model.clone(),
            instructions: self.config.system_prompt.clone(),
            input: crate::ai::prompt::build_user_prompt_with_context(
                word,
                &self.config.answer_language,
                context,
            ),
            tools: vec![ResponseTool {
                r#type: "web_search",
            }],
            tool_choice: "auto".to_string(),
            max_output_tokens: 4096,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .with_context(|| format!("AI 网页搜索请求失败: {url}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("AI 网页搜索返回错误 ({status}): {body}");
        }

        let responses: ResponsesResponse =
            response.json().await.context("解析 AI 网页搜索响应失败")?;
        let text = extract_response_text(&responses).context("AI 网页搜索响应为空")?;
        Ok(append_citations(text, &responses))
    }

    fn api_base_url(&self) -> String {
        let endpoint = self.config.api_endpoint.trim_end_matches('/');
        endpoint
            .strip_suffix("/chat/completions")
            .or_else(|| endpoint.strip_suffix("/responses"))
            .unwrap_or(endpoint)
            .to_string()
    }

    fn chat_completions_url(&self) -> String {
        let endpoint = self.config.api_endpoint.trim_end_matches('/');
        if endpoint.ends_with("/chat/completions") {
            endpoint.to_string()
        } else {
            format!("{}/chat/completions", self.api_base_url())
        }
    }

    fn responses_url(&self) -> String {
        let endpoint = self.config.api_endpoint.trim_end_matches('/');
        if endpoint.ends_with("/responses") {
            endpoint.to_string()
        } else {
            format!("{}/responses", self.api_base_url())
        }
    }
}

fn extract_response_text(response: &ResponsesResponse) -> Option<String> {
    if let Some(text) = response
        .output_text
        .as_deref()
        .filter(|text| !text.trim().is_empty())
    {
        return Some(text.to_string());
    }

    let text = response
        .output
        .iter()
        .flat_map(|item| item.content.iter())
        .filter_map(|content| content.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n");

    (!text.trim().is_empty()).then_some(text)
}

fn append_citations(mut text: String, response: &ResponsesResponse) -> String {
    let mut seen = HashSet::new();
    let mut citations = Vec::new();

    for annotation in response
        .output
        .iter()
        .flat_map(|item| item.content.iter())
        .flat_map(|content| content.annotations.iter())
    {
        let Some(url) = annotation.url.as_deref() else {
            continue;
        };
        if !seen.insert(url.to_string()) {
            continue;
        }

        let title = annotation.title.as_deref().unwrap_or(url);
        citations.push(format!("{}: {}", title, url));
    }

    if !citations.is_empty() {
        text.push_str("\n\n来源:\n");
        text.push_str(&citations.join("\n"));
    }

    text
}
