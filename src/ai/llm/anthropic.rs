use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    ChatRequest, ChatResponse, LlmProvider, ProviderError, ProviderMessage, ProviderToolCall,
};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    api_key: String,
    default_model: String,
    http: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(api_key: String, default_model: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("Failed to build HTTP client for Anthropic");
        AnthropicProvider {
            api_key,
            default_model,
            http,
        }
    }
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct AnthropicMessage {
    role: String,
    content: Value,
}

#[derive(Serialize, Deserialize, Debug)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
    usage: Option<AnthropicUsage>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
}

#[derive(Serialize, Deserialize, Debug)]
struct AnthropicUsage {
    input_tokens: i64,
    output_tokens: i64,
}

fn convert_messages(messages: &[ProviderMessage]) -> Vec<AnthropicMessage> {
    let mut out = Vec::new();

    for m in messages {
        match m {
            ProviderMessage::User { text } => {
                out.push(AnthropicMessage {
                    role: "user".into(),
                    content: Value::String(text.clone()),
                });
            }
            ProviderMessage::Assistant { text, tool_calls } => {
                if !tool_calls.is_empty() {
                    let mut blocks: Vec<Value> = Vec::new();
                    if let Some(t) = text
                        && !t.is_empty()
                    {
                        blocks.push(json!({ "type": "text", "text": t }));
                    }
                    for tc in tool_calls {
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.args,
                        }));
                    }
                    out.push(AnthropicMessage {
                        role: "assistant".into(),
                        content: Value::Array(blocks),
                    });
                } else if let Some(t) = text {
                    out.push(AnthropicMessage {
                        role: "assistant".into(),
                        content: Value::String(t.clone()),
                    });
                }
            }
            ProviderMessage::ToolResults { results } => {
                let blocks: Vec<Value> = results
                    .iter()
                    .map(|r| {
                        let content_text = r
                            .output
                            .get("text")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| r.output.to_string());
                        json!({
                            "type": "tool_result",
                            "tool_use_id": r.tool_call_id,
                            "content": content_text,
                            "is_error": r.is_error,
                        })
                    })
                    .collect();
                out.push(AnthropicMessage {
                    role: "user".into(),
                    content: Value::Array(blocks),
                });
            }
        }
    }

    out
}

fn convert_tools(tools: &[super::ToolSpecForProvider]) -> Option<Vec<Value>> {
    if tools.is_empty() {
        return None;
    }
    Some(
        tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.schema,
                })
            })
            .collect(),
    )
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let anthropic_messages = convert_messages(&req.messages);
        let anthropic_tools = convert_tools(&req.tools);

        let anthropic_req = AnthropicRequest {
            model: req.model.clone(),
            max_tokens: 16_000,
            system: req.system.clone(),
            messages: anthropic_messages,
            tools: anthropic_tools,
        };

        let response = self
            .http
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&anthropic_req)
            .send()
            .await
            .map_err(|e| ProviderError::Transport(format!("Anthropic request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::Transport(format!(
                "Anthropic HTTP {status}: {body}"
            )));
        }

        let anthropic_resp: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::Protocol(format!("Anthropic response parse: {e}")))?;

        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<ProviderToolCall> = Vec::new();

        for block in &anthropic_resp.content {
            match block {
                AnthropicContentBlock::Text { text } => {
                    text_parts.push(text.clone());
                }
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ProviderToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        args: input.clone(),
                        thought_signature: None,
                    });
                }
            }
        }

        let text = if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join("\n"))
        };

        let (input_tokens, output_tokens) = match &anthropic_resp.usage {
            Some(u) => (Some(u.input_tokens), Some(u.output_tokens)),
            None => (None, None),
        };

        Ok(ChatResponse {
            text,
            tool_calls,
            input_tokens,
            output_tokens,
        })
    }

    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }
}
