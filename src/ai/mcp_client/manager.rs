use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::entity::{assistant_session, user_mcp_server};

use super::{McpClientPool, McpServerConfig};

fn parse_headers_json(raw: &serde_json::Value) -> HashMap<String, String> {
    let Some(obj) = raw.as_object() else {
        return HashMap::new();
    };
    obj.iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
        .collect()
}

fn parse_id_array(raw: &serde_json::Value) -> Vec<i32> {
    raw.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_i64().map(|n| n as i32))
                .collect()
        })
        .unwrap_or_default()
}

const POOL_TTL: Duration = Duration::from_secs(60);

struct CacheEntry {
    user_id: i32,
    pool: Arc<McpClientPool>,
    built_at: Instant,
}

/// UserMcpManager builds and caches an `McpClientPool` per assistant session.
/// The session row carries `enabled_mcp_server_ids` — a subset of the user's
/// enabled `user_mcp_servers` — and only those servers are queried for tool
/// discovery. Discovery is cached for 60s, keyed by session id.
pub struct UserMcpManager {
    db: DatabaseConnection,
    cache: DashMap<i32, CacheEntry>,
}

impl UserMcpManager {
    pub fn new(db: DatabaseConnection) -> Self {
        UserMcpManager {
            db,
            cache: DashMap::new(),
        }
    }

    /// Drop the cached pool for `session_id`. Call after the session's
    /// `enabled_mcp_server_ids` changes.
    pub fn invalidate_session(&self, session_id: i32) {
        self.cache.remove(&session_id);
    }

    /// Drop every cached pool that belongs to a given user. Call after CRUD
    /// on the user's `user_mcp_servers` rows — every session pool for that
    /// user may have stale data.
    pub fn invalidate_user(&self, user_id: i32) {
        self.cache.retain(|_, e| e.user_id != user_id);
    }

    /// Resolve (and cache) the pool for the session, restricted to the MCP
    /// servers selected on the session row.
    pub async fn pool_for_session(
        &self,
        session_id: i32,
        user_id: i32,
        user_token: &str,
    ) -> anyhow::Result<Arc<McpClientPool>> {
        if let Some(entry) = self.cache.get(&session_id)
            && entry.built_at.elapsed() < POOL_TTL
        {
            return Ok(entry.pool.clone());
        }

        let session = assistant_session::Entity::find_by_id(session_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Session {session_id} not found"))?;

        let selected_ids = parse_id_array(&session.enabled_mcp_server_ids);
        if selected_ids.is_empty() {
            let pool = Arc::new(McpClientPool::empty());
            self.cache.insert(
                session_id,
                CacheEntry {
                    user_id,
                    pool: pool.clone(),
                    built_at: Instant::now(),
                },
            );
            return Ok(pool);
        }

        let user_rows = user_mcp_server::Entity::find()
            .filter(user_mcp_server::Column::UserId.eq(user_id))
            .filter(user_mcp_server::Column::Enabled.eq(true))
            .filter(user_mcp_server::Column::Id.is_in(selected_ids))
            .all(&self.db)
            .await?;

        let configs: Vec<McpServerConfig> = user_rows
            .into_iter()
            .map(|row| McpServerConfig {
                name: row.name,
                url: row.url,
                enabled: true,
                forward_user_token: row.forward_user_token,
                custom_headers: parse_headers_json(&row.headers),
            })
            .collect();

        let pool = Arc::new(McpClientPool::build(&configs, user_token).await?);

        // Don't cache a fully-empty result: discovery probably hit auth or a
        // transient transport failure — try again on the next request.
        let any_connected = pool.servers().iter().any(|s| s.connected);
        if any_connected || configs.is_empty() {
            self.cache.insert(
                session_id,
                CacheEntry {
                    user_id,
                    pool: pool.clone(),
                    built_at: Instant::now(),
                },
            );
        }
        Ok(pool)
    }

    /// Build a one-shot pool from every enabled MCP server for a user. Used
    /// by the user-level management UI to discover tools regardless of which
    /// session is active. Not cached.
    pub async fn discover_user_servers(
        &self,
        user_id: i32,
        user_token: &str,
    ) -> anyhow::Result<McpClientPool> {
        let user_rows = user_mcp_server::Entity::find()
            .filter(user_mcp_server::Column::UserId.eq(user_id))
            .filter(user_mcp_server::Column::Enabled.eq(true))
            .all(&self.db)
            .await?;

        let configs: Vec<McpServerConfig> = user_rows
            .into_iter()
            .map(|row| McpServerConfig {
                name: row.name,
                url: row.url,
                enabled: true,
                forward_user_token: row.forward_user_token,
                custom_headers: parse_headers_json(&row.headers),
            })
            .collect();

        McpClientPool::build(&configs, user_token).await
    }
}
