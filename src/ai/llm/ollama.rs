use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    ChatRequest, ChatResponse, LlmProvider, ProviderError, ProviderMessage, ProviderToolCall,
};

pub struct OllamaProvider {
    base_url: String,
    default_model: String,
    http: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(base_url: String, default_model: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("Failed to build HTTP client for Ollama");
        OllamaProvider {
            base_url,
            default_model,
            http,
        }
    }
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OllamaToolCall {
    function: OllamaFunction,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OllamaFunction {
    name: String,
    arguments: Value,
}

#[derive(Serialize, Deserialize, Debug)]
struct OllamaResponse {
    message: Option<OllamaMessage>,
    #[serde(default)]
    prompt_eval_count: Option<i64>,
    #[serde(default)]
    eval_count: Option<i64>,
}

fn convert_messages(system: &str, messages: &[ProviderMessage]) -> Vec<OllamaMessage> {
    let mut out = vec![OllamaMessage {
        role: "system".into(),
        content: system.to_string(),
        tool_calls: None,
    }];

    for m in messages {
        match m {
            ProviderMessage::User { text } => {
                out.push(OllamaMessage {
                    role: "user".into(),
                    content: text.clone(),
                    tool_calls: None,
                });
            }
            ProviderMessage::Assistant { text, tool_calls } => {
                if !tool_calls.is_empty() {
                    let calls: Vec<OllamaToolCall> = tool_calls
                        .iter()
                        .map(|tc| OllamaToolCall {
                            function: OllamaFunction {
                                name: tc.name.clone(),
                                arguments: tc.args.clone(),
                            },
                        })
                        .collect();
                    out.push(OllamaMessage {
                        role: "assistant".into(),
                        content: text.clone().unwrap_or_default(),
                        tool_calls: Some(calls),
                    });
                } else {
                    out.push(OllamaMessage {
                        role: "assistant".into(),
                        content: text.clone().unwrap_or_default(),
                        tool_calls: None,
                    });
                }
            }
            ProviderMessage::ToolResults { results } => {
                for r in results {
                    let content = r
                        .output
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| r.output.to_string());
                    out.push(OllamaMessage {
                        role: "tool".into(),
                        content,
                        tool_calls: None,
                    });
                }
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
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.schema,
                    }
                })
            })
            .collect(),
    )
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let ollama_messages = convert_messages(&req.system, &req.messages);
        let ollama_tools = convert_tools(&req.tools);

        let ollama_req = OllamaRequest {
            model: req.model.clone(),
            messages: ollama_messages,
            stream: false,
            tools: ollama_tools,
        };

        let response = self
            .http
            .post(format!("{}/api/chat", self.base_url))
            .json(&ollama_req)
            .send()
            .await
            .map_err(|e| ProviderError::Transport(format!("Ollama request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::Transport(format!(
                "Ollama HTTP {status}: {body}"
            )));
        }

        let ollama_resp: OllamaResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::Protocol(format!("Ollama response parse: {e}")))?;

        let (text, tool_calls) = match ollama_resp.message {
            Some(msg) => {
                let text = if msg.content.is_empty() {
                    None
                } else {
                    Some(msg.content)
                };
                let calls = msg
                    .tool_calls
                    .unwrap_or_default()
                    .into_iter()
                    .enumerate()
                    .map(|(i, tc)| ProviderToolCall {
                        id: format!("tc_{i}"),
                        name: tc.function.name,
                        args: tc.function.arguments,
                        thought_signature: None,
                    })
                    .collect();
                (text, calls)
            }
            None => (None, vec![]),
        };

        Ok(ChatResponse {
            text,
            tool_calls,
            input_tokens: ollama_resp.prompt_eval_count,
            output_tokens: ollama_resp.eval_count,
        })
    }

    fn name(&self) -> &'static str {
        "ollama"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }
}
