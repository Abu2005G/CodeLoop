use super::*;
use serde_json::json;

#[derive(Debug, Clone)]
pub struct DeepSeekClient {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl DeepSeekClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl LlmClient for DeepSeekClient {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> anyhow::Result<ChatResponse> {
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::to_value(tools)?;
        }

        let resp = self
            .client
            .post("https://api.deepseek.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("DeepSeek API error ({status}): {text}");
        }

        let data: serde_json::Value = resp.json().await?;

        let choice = data["choices"]
            .as_array()
            .and_then(|c| c.first())
            .ok_or_else(|| anyhow::anyhow!("No choices in DeepSeek API response: {data}"))?;

        let message = &choice["message"];
        let finish_reason = choice["finish_reason"].as_str().map(String::from);

        let content = message["content"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(String::from);

        let tool_calls: Vec<ToolCall> = message["tool_calls"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| serde_json::from_value(tc.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        Ok(ChatResponse {
            content,
            tool_calls,
            finish_reason,
        })
    }
}
