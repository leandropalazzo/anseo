//! stdio JSON-RPC transport.
//!
//! Line-delimited JSON: one message per line, both directions. The MCP spec's
//! 2024-11-05 revision accepts both line-delimited and `Content-Length:`
//! framing; we use the simpler line-delimited shape for Story 16.1. Logs go
//! to stderr (NEVER stdout — that's the protocol channel; see
//! architecture-phase3-mcp-server.md §2.2 framing rule).

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::dispatch::{Dispatcher, Outbound};
use crate::protocol::{ErrorResponse, Id, Request, PARSE_ERROR};

/// Run the stdio loop until EOF or fatal I/O error.
pub async fn run(dispatcher: Dispatcher) -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin).lines();

    while let Some(line) = reader.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Request>(trimmed) {
            Ok(req) => dispatcher.dispatch(req),
            Err(err) => Outbound::Failure(ErrorResponse::new(
                Id::Null,
                PARSE_ERROR,
                format!("parse error: {err}"),
            )),
        };
        match reply {
            Outbound::Success(resp) => write_message(&mut stdout, &resp).await?,
            Outbound::Failure(err) => write_message(&mut stdout, &err).await?,
            Outbound::Silent => {}
        }
    }
    Ok(())
}

async fn write_message<T: serde::Serialize>(
    out: &mut tokio::io::Stdout,
    msg: &T,
) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec(msg)?;
    bytes.push(b'\n');
    out.write_all(&bytes).await?;
    out.flush().await?;
    Ok(())
}
