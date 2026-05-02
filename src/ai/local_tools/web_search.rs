use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{LocalTool, LocalToolCtx};
use crate::ai::mcp_client::ToolDispatchError;

pub struct WebSearch {
    serper_api_key: Option<String>,
}

impl WebSearch {
    pub fn new(serper_api_key: Option<String>) -> Self {
        WebSearch { serper_api_key }
    }
}

#[derive(Deserialize)]
struct Args {
    query: String,
    #[serde(default)]
    num: Option<u32>,
}

#[async_trait]
impl LocalTool for WebSearch {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web via Serper. Returns top organic results with title, URL, and snippet. \
         Use to find current information from the public internet."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query. Prefer 2-5 keywords."
                },
                "num": {
                    "type": "integer",
                    "description": "Number of results (default 5, max 10)"
                }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, _ctx: &LocalToolCtx, args: Value) -> Result<Value, ToolDispatchError> {
        let args: Args = serde_json::from_value(args)
            .map_err(|e| ToolDispatchError::Execution(format!("Invalid arguments: {e}")))?;

        let api_key = self
            .serper_api_key
            .as_deref()
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| {
                ToolDispatchError::Execution(
                    "Web search not configured: SERPER_API_KEY not set".into(),
                )
            })?;

        let num = args.num.unwrap_or(5).min(10);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| ToolDispatchError::Execution(e.to_string()))?;

        let body = json!({
            "q": args.query,
            "num": num,
        });

        let resp = client
            .post("https://google.serper.dev/search")
            .header("X-API-KEY", api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ToolDispatchError::Execution(format!("Serper request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ToolDispatchError::Execution(format!(
                "Serper HTTP {status}: {body}"
            )));
        }

        let data: Value = resp
            .json()
            .await
            .map_err(|e| ToolDispatchError::Execution(format!("Serper JSON parse: {e}")))?;

        let mut out = String::new();
        if let Some(kg) = data.get("knowledgeGraph") {
            let title = kg.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let desc = kg.get("description").and_then(|v| v.as_str()).unwrap_or("");
            if !title.is_empty() || !desc.is_empty() {
                out.push_str(&format!("{title}\n{desc}\n\n"));
            }
        }
        if let Some(answer) = data
            .get("answerBox")
            .and_then(|a| a.get("answer").or_else(|| a.get("snippet")))
            .and_then(|v| v.as_str())
        {
            out.push_str(&format!("Answer: {answer}\n\n"));
        }
        if let Some(results) = data.get("organic").and_then(|v| v.as_array()) {
            for (i, r) in results.iter().enumerate() {
                let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let url = r.get("link").and_then(|v| v.as_str()).unwrap_or("");
                let snippet = r.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
                out.push_str(&format!("{}. {title}\n{url}\n{snippet}\n\n", i + 1));
            }
        }
        if out.is_empty() {
            out.push_str("No results.");
        }

        Ok(json!({ "text": out }))
    }
}
