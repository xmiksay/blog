pub mod anthropic;
pub mod gemini;
pub mod ollama;
pub mod registry;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolSpecForProvider {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ProviderMessage {
    User {
        text: String,
    },
    Assistant {
        text: Option<String>,
        tool_calls: Vec<ProviderToolCall>,
    },
    ToolResults {
        results: Vec<ProviderToolResult>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
    /// Opaque per-provider round-trip token. Gemini 2.5 thinking models
    /// require the `thoughtSignature` from each `functionCall` part to be
    /// echoed back on the next turn. Other providers ignore this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderToolResult {
    pub tool_call_id: String,
    pub output: Value,
    pub is_error: bool,
}

#[derive(Clone, Debug)]
pub struct ChatRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<ProviderMessage>,
    pub tools: Vec<ToolSpecForProvider>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ProviderToolCall>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Provider config error: {0}")]
    Config(String),
    #[error("Provider transport error: {0}")]
    Transport(String),
    #[error("Provider protocol error: {0}")]
    Protocol(String),
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError>;
    fn name(&self) -> &'static str;
    fn default_model(&self) -> &str;
}
