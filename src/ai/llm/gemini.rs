use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::{
    ChatRequest, ChatResponse, LlmProvider, ProviderError, ProviderMessage, ProviderToolCall,
};

const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";

pub struct GeminiProvider {
    api_key: String,
    default_model: String,
    http: reqwest::Client,
}

impl GeminiProvider {
    pub fn new(api_key: String, default_model: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("Failed to build HTTP client for Gemini");
        GeminiProvider {
            api_key,
            default_model,
            http,
        }
    }
}

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GeminiContent {
    role: String,
    parts: Vec<Value>,
}

#[derive(Deserialize, Serialize, Debug)]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata", default)]
    usage_metadata: Option<GeminiUsage>,
}

#[derive(Deserialize, Serialize, Debug)]
struct GeminiCandidate {
    #[serde(default)]
    content: Option<GeminiContent>,
}

#[derive(Deserialize, Serialize, Debug)]
struct GeminiUsage {
    #[serde(rename = "promptTokenCount", default)]
    prompt_token_count: Option<i64>,
    #[serde(rename = "candidatesTokenCount", default)]
    candidates_token_count: Option<i64>,
}

/// Gemini rejects JSON Schema fields like `$schema`, `additionalProperties`,
/// `$ref`, `definitions`, `title`, etc. Strip them recursively. Also flatten
/// `type: ["string", "null"]` (JSON Schema union form) into a single string
/// type plus `nullable: true`. Finally, filter `required` so it only lists
/// keys actually present in `properties` — Gemini rejects the request
/// otherwise (`property is not defined`).
fn sanitize_schema(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            let mut nullable_from_type = false;
            for (k, v) in map {
                if matches!(
                    k.as_str(),
                    "$schema"
                        | "$ref"
                        | "$id"
                        | "$defs"
                        | "definitions"
                        | "additionalProperties"
                        | "title"
                        | "examples"
                        | "default"
                        | "const"
                        | "oneOf"
                        | "anyOf"
                        | "allOf"
                        | "not"
                        | "format"
                ) {
                    continue;
                }
                if k == "type"
                    && let Value::Array(types) = v
                {
                    let mut primary: Option<String> = None;
                    for t in types {
                        if let Some(s) = t.as_str() {
                            if s == "null" {
                                nullable_from_type = true;
                            } else if primary.is_none() {
                                primary = Some(s.to_string());
                            }
                        }
                    }
                    if let Some(t) = primary {
                        out.insert("type".into(), Value::String(t));
                    }
                    continue;
                }
                out.insert(k.clone(), sanitize_schema(v));
            }
            if nullable_from_type {
                out.insert("nullable".into(), Value::Bool(true));
            }
            // Drop `required` entries that aren't present in `properties`,
            // and remove `required` entirely if empty (or if no properties).
            let prop_keys: Option<Vec<String>> = out
                .get("properties")
                .and_then(|p| p.as_object())
                .map(|m| m.keys().cloned().collect());
            if let Some(req) = out.get("required").cloned() {
                if let Value::Array(arr) = req {
                    let allowed: Vec<Value> = arr
                        .into_iter()
                        .filter(|v| match (v.as_str(), &prop_keys) {
                            (Some(name), Some(keys)) => keys.iter().any(|k| k == name),
                            _ => false,
                        })
                        .collect();
                    if allowed.is_empty() {
                        out.remove("required");
                    } else {
                        out.insert("required".into(), Value::Array(allowed));
                    }
                } else {
                    out.remove("required");
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(sanitize_schema).collect()),
        _ => value.clone(),
    }
}

fn convert_messages(messages: &[ProviderMessage]) -> Vec<GeminiContent> {
    let mut out = Vec::new();

    for m in messages {
        match m {
            ProviderMessage::User { text } => {
                out.push(GeminiContent {
                    role: "user".into(),
                    parts: vec![json!({ "text": text })],
                });
            }
            ProviderMessage::Assistant { text, tool_calls } => {
                let mut parts: Vec<Value> = Vec::new();
                if let Some(t) = text
                    && !t.is_empty()
                {
                    parts.push(json!({ "text": t }));
                }
                for tc in tool_calls {
                    let mut part = Map::new();
                    let mut fc = Map::new();
                    fc.insert("name".into(), Value::String(tc.name.clone()));
                    fc.insert("args".into(), tc.args.clone());
                    part.insert("functionCall".into(), Value::Object(fc));
                    if let Some(sig) = &tc.thought_signature {
                        part.insert("thoughtSignature".into(), Value::String(sig.clone()));
                    }
                    parts.push(Value::Object(part));
                }
                if parts.is_empty() {
                    continue;
                }
                out.push(GeminiContent {
                    role: "model".into(),
                    parts,
                });
            }
            ProviderMessage::ToolResults { results } => {
                let parts: Vec<Value> = results
                    .iter()
                    .map(|r| {
                        json!({
                            "functionResponse": {
                                "name": r.tool_call_id,
                                "response": {
                                    "result": r.output,
                                    "is_error": r.is_error,
                                }
                            }
                        })
                    })
                    .collect();
                out.push(GeminiContent {
                    role: "user".into(),
                    parts,
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
    let declarations: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "parameters": sanitize_schema(&t.schema),
            })
        })
        .collect();
    Some(vec![json!({ "functionDeclarations": declarations })])
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let gemini_messages = convert_messages(&req.messages);
        let gemini_tools = convert_tools(&req.tools);

        let system_instruction = if req.system.is_empty() {
            None
        } else {
            Some(GeminiContent {
                role: "system".into(),
                parts: vec![json!({ "text": req.system })],
            })
        };

        let gemini_req = GeminiRequest {
            contents: gemini_messages,
            system_instruction,
            tools: gemini_tools,
        };

        let url = format!("{}/{}:generateContent", GEMINI_API_BASE, req.model);

        let response = self
            .http
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .header("content-type", "application/json")
            .json(&gemini_req)
            .send()
            .await
            .map_err(|e| ProviderError::Transport(format!("Gemini request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::Transport(format!(
                "Gemini HTTP {status}: {body}"
            )));
        }

        let gemini_resp: GeminiResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::Protocol(format!("Gemini response parse: {e}")))?;

        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<ProviderToolCall> = Vec::new();

        for cand in &gemini_resp.candidates {
            let Some(content) = &cand.content else {
                continue;
            };
            for part in &content.parts {
                if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                    if !t.is_empty() {
                        text_parts.push(t.to_string());
                    }
                } else if let Some(fc) = part.get("functionCall") {
                    let name = fc
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let args = fc.get("args").cloned().unwrap_or(Value::Object(Map::new()));
                    let thought_signature = part
                        .get("thoughtSignature")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    tool_calls.push(ProviderToolCall {
                        id: name.clone(),
                        name,
                        args,
                        thought_signature,
                    });
                }
            }
        }

        let text = if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join("\n"))
        };

        let (input_tokens, output_tokens) = match &gemini_resp.usage_metadata {
            Some(u) => (u.prompt_token_count, u.candidates_token_count),
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
        "gemini"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }
}
