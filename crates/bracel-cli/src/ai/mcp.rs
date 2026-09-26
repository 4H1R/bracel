use super::{execute, project::Config};
use rmcp::{
    ServiceExt,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars, tool, tool_router,
    transport::stdio,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone)]
struct Companion {
    root: PathBuf,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct Search {
    query: String,
    limit: Option<usize>,
}

impl Companion {
    fn call(&self, name: &str, query: Option<&str>, limit: usize) -> CallToolResult {
        let result = Config::load(&self.root)
            .and_then(|config| execute(&self.root, config, name, query, limit));
        match result {
            Ok(value) => {
                let content = vec![ContentBlock::text(value.to_string())];
                if value.get("ok") == Some(&serde_json::Value::Bool(false)) {
                    CallToolResult::error(content)
                } else {
                    CallToolResult::success(content)
                }
            }
            Err(error) => CallToolResult::error(vec![ContentBlock::text(
                serde_json::json!({"ok":false,"error":error}).to_string(),
            )]),
        }
    }
}
#[tool_router(server_handler)]
impl Companion {
    #[tool(
        description = "Read exact resolved Bracel dependencies and current source/knowledge freshness. Call at task start."
    )]
    fn project_info(&self) -> CallToolResult {
        self.call("info", None, 8)
    }
    #[tool(
        description = "Search documentation and source for the application's resolved Bracel revision, including live local edits. Returns paths, hashes and line references."
    )]
    fn search_docs(&self, Parameters(args): Parameters<Search>) -> CallToolResult {
        self.call("search", Some(&args.query), args.limit.unwrap_or(8))
    }
    #[tool(
        description = "Read framework capabilities, API references, resolution availability and explicit verification limits."
    )]
    fn capabilities(&self) -> CallToolResult {
        self.call("capabilities", None, 8)
    }
    #[tool(
        description = "Run the configured trusted application executable with fixed inspect --json arguments. Read-only; never compiles or runs migrations."
    )]
    fn inspect_application(&self) -> CallToolResult {
        self.call("inspect", None, 8)
    }
    #[tool(
        description = "Check generated instructions and knowledge freshness without writing files. Returns stable diagnostic codes."
    )]
    fn doctor(&self) -> CallToolResult {
        self.call("doctor", None, 8)
    }
}

pub fn serve(root: PathBuf) -> super::util::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async move {
        let service = Companion { root }
            .serve(stdio())
            .await
            .map_err(|e| e.to_string())?;
        service.waiting().await.map_err(|e| e.to_string())?;
        Ok(())
    })
}
