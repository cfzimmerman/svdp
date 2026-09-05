//! Packaging spike: minimal ServWare MCP server over stdio.
//!
//! Verifies the delivery vehicle end to end before the rewrite commits to it:
//! rmcp builds, tools carry `readOnlyHint`, and the binary speaks MCP on stdio.

use rmcp::ErrorData;
use rmcp::ServerHandler;
use rmcp::ServiceExt;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::model::ContentBlock;
use rmcp::model::ServerCapabilities;
use rmcp::model::ServerInfo;
use rmcp::tool;
use rmcp::tool_handler;
use rmcp::tool_router;
use rmcp::transport::stdio;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Clone)]
pub struct SvdpServer;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EchoParams {
    /// Text to echo back.
    pub text: String,
}

#[tool_router]
impl SvdpServer {
    /// Reports whether the server is reachable and configured.
    #[tool(name = "servware_health", annotations(read_only_hint = true))]
    pub async fn servware_health(&self) -> Result<CallToolResult, ErrorData> {
        let user = std::env::var("SERVWARE_USER").ok();
        let has_pass = std::env::var("SERVWARE_PASS").is_ok();
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "spike ok; username configured: {}; password configured: {has_pass}",
            user.is_some()
        ))]))
    }

    /// Echoes text back, to prove parameter schemas round-trip.
    #[tool(name = "echo", annotations(read_only_hint = true))]
    pub async fn echo(
        &self,
        Parameters(p): Parameters<EchoParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::success(vec![ContentBlock::text(p.text)]))
    }
}

#[tool_handler]
impl ServerHandler for SvdpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build());
        info.instructions = Some("SVdP ServWare tools.".into());
        info
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = SvdpServer.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
