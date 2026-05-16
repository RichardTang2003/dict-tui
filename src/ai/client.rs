use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

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

pub struct AiClient {
    client: Client,
    config: Config,
}

impl AiClient {
    pub fn new(config: Config) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("创建 HTTP 客户端失败");
        Self { client, config }
    }

    pub async fn query_with_context(&self, word: &str, context: &str) -> Result<String> {
        let url = format!(
            "{}/chat/completions",
            self.config.api_endpoint.trim_end_matches('/')
        );

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
            .with_context(|| format!("AI 请求失败: {}", url))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("AI API 返回错误 ({}): {}", status, body);
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
}
