//! Transport implementations.
//!
//! Story 16.1 shipped stdio only. Story 16.6 adds HTTP+SSE per
//! AD-Phase3-MCP-TransportDefault (architecture-phase3-mcp-server.md §2.2).

pub mod http;
pub mod stdio;
