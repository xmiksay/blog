use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::DatabaseConnection;
use serde_json::Value;

use crate::ai::llm::ToolSpecForProvider;
use crate::ai::local_tools::{LocalTool, LocalToolCtx};
use crate::ai::mcp_client::{McpClientPool, ToolDispatchError};

pub struct DispatchCtx {
    pub db: DatabaseConnection,
    pub session_id: i32,
    pub user_id: i32,
    pub user_token: String,
    pub mcp_pool: Arc<McpClientPool>,
}

/// Stateless registry of in-process local tools. MCP tools come from the
/// per-user pool passed at call time.
pub struct ToolRegistry {
    local: HashMap<String, Arc<dyn LocalTool>>,
}

impl ToolRegistry {
    pub fn new(local_tools: Vec<Arc<dyn LocalTool>>) -> Self {
        let local = local_tools
            .into_iter()
            .map(|t| (t.name().to_string(), t))
            .collect();
        ToolRegistry { local }
    }

    /// All tool specs (MCP + local) for the LLM, with the per-user pool.
    pub fn aggregated_specs(&self, mcp_pool: &McpClientPool) -> Vec<ToolSpecForProvider> {
        let mut specs = mcp_pool.aggregated_tool_specs();
        for tool in self.local.values() {
            specs.push(ToolSpecForProvider {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                schema: tool.input_schema(),
            });
        }
        specs
    }

    pub async fn dispatch(
        &self,
        name: &str,
        args: Value,
        ctx: &DispatchCtx,
    ) -> Result<Value, ToolDispatchError> {
        if let Some(tool) = self.local.get(name) {
            let local_ctx = LocalToolCtx {
                db: ctx.db.clone(),
                user_id: ctx.user_id,
                session_id: ctx.session_id,
            };
            return tool.call(&local_ctx, args).await;
        }

        if ctx.mcp_pool.has_tool(name) {
            return ctx.mcp_pool.dispatch(name, args, &ctx.user_token).await;
        }

        Err(ToolDispatchError::UnknownTool(name.to_string()))
    }
}
