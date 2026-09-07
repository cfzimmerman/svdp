//! Logic layer shared by both front ends: the MCP server volunteers use and the
//! maintainer CLI. Nothing here knows which one is calling.

pub mod domain;
pub mod servware;
