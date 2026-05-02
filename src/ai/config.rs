use crate::markdown::MARKDOWN_EXTENSIONS_DOC;

/// Static AI config — the fallback system prompt used when the
/// `system/prompt` page is missing. Provider connections live in the
/// `llm_providers` table and are managed via `/api/assistant/providers`.
#[derive(Clone, Debug)]
pub struct AiConfig {
    pub system_prompt: String,
}

impl AiConfig {
    pub fn new() -> Self {
        AiConfig {
            system_prompt: default_system_prompt(),
        }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Path of the page whose markdown overrides the fallback prompt.
pub const SYSTEM_PROMPT_PAGE_PATH: &str = "system/prompt";

fn default_system_prompt() -> String {
    format!(
        "You are an assistant embedded in a personal site/CMS. You can manage pages, tags, \
         files, galleries, menu items, and tokens through the available tools. Prefer searching \
         existing pages before creating new ones.\n\n\
         When writing page markdown, use the site's custom directives:\n\n{MARKDOWN_EXTENSIONS_DOC}"
    )
}
