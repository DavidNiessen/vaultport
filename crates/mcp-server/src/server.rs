// SPDX-License-Identifier: GPL-3.0-or-later
//! JSON-RPC dispatch and the line-delimited transport.

use crate::protocol::{ClientInfo, Tool, ToolResult, negotiate};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

/// Largest accepted message (a line). Longer lines close the connection.
const MAX_MESSAGE: usize = 8 * 1024 * 1024;

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Identity and guidance sent in the `initialize` result.
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub name: String,
    pub title: String,
    pub version: String,
    /// Usage notes for the model (`instructions`).
    pub instructions: Option<String>,
}

/// Per-call context.
#[derive(Debug, Clone)]
pub struct CallContext {
    /// The client from `initialize`.
    pub client: ClientInfo,
    /// Identifies the connection (for "allow for this session" grants).
    pub connection_id: u64,
}

/// Implements the tools.
#[async_trait::async_trait]
pub trait Handler: Send + Sync + 'static {
    /// The tools offered in `tools/list`.
    fn tools(&self) -> Vec<Tool>;

    /// Runs a tool. `name` is one of [`Handler::tools`].
    async fn call(&self, ctx: CallContext, name: String, arguments: Value) -> ToolResult;

    /// Called once the connection ends (to drop session grants).
    fn disconnected(&self, _connection_id: u64) {}
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    jsonrpc: Option<String>,
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
}

fn response(id: &Value, result: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn id_key(id: &Value) -> String {
    id.to_string()
}

/// Serves one MCP connection until the reader reaches EOF or the writer fails.
pub async fn serve<R, W>(
    handler: Arc<dyn Handler>,
    info: ServerInfo,
    connection_id: u64,
    mut reader: R,
    mut writer: W,
) -> std::io::Result<()>
where
    R: AsyncBufRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Value>();
    let writer_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let mut line = serde_json::to_vec(&msg).unwrap_or_default();
            line.push(b'\n');
            writer.write_all(&line).await?;
            writer.flush().await?;
        }
        Ok::<_, std::io::Error>(())
    });

    let client = Arc::new(Mutex::new(ClientInfo::default()));
    let running: Arc<Mutex<HashMap<String, tokio::task::AbortHandle>>> = Arc::default();
    let mut tasks = JoinSet::new();
    let mut buf = Vec::new();

    let result = loop {
        buf.clear();
        let n = match (&mut reader)
            .take(MAX_MESSAGE as u64 + 1)
            .read_until(b'\n', &mut buf)
            .await
        {
            Ok(n) => n,
            Err(e) => break Err(e),
        };
        if n == 0 {
            break Ok(());
        }
        if buf.len() > MAX_MESSAGE {
            let _ = out_tx.send(error(&Value::Null, INVALID_REQUEST, "message too large"));
            break Ok(());
        }
        let line = buf.trim_ascii();
        if line.is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_slice::<Message>(line) else {
            let _ = out_tx.send(error(&Value::Null, PARSE_ERROR, "parse error"));
            continue;
        };
        if msg.jsonrpc.as_deref() != Some("2.0") {
            let id = msg.id.unwrap_or(Value::Null);
            let _ = out_tx.send(error(&id, INVALID_REQUEST, "expected jsonrpc 2.0"));
            continue;
        }
        let Some(method) = msg.method else {
            // A response to a server request; this server sends none.
            continue;
        };
        let params = msg.params.unwrap_or(Value::Null);
        let Some(id) = msg.id else {
            // Notification.
            if method == "notifications/cancelled"
                && let Some(rid) = params.get("requestId")
                && let Some(h) = running.lock().map_or(None, |mut r| r.remove(&id_key(rid)))
            {
                h.abort();
            }
            continue;
        };
        match method.as_str() {
            "initialize" => {
                if let Some(c) = params.get("clientInfo")
                    && let Ok(c) = serde_json::from_value::<ClientInfo>(c.clone())
                    && let Ok(mut slot) = client.lock()
                {
                    *slot = c;
                }
                let version = negotiate(params.get("protocolVersion").and_then(Value::as_str));
                let mut result = json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {
                        "name": info.name,
                        "title": info.title,
                        "version": info.version,
                    },
                });
                if let Some(i) = &info.instructions {
                    result["instructions"] = Value::String(i.clone());
                }
                let _ = out_tx.send(response(&id, &result));
            }
            "ping" => {
                let _ = out_tx.send(response(&id, &json!({})));
            }
            "tools/list" => {
                let tools = handler.tools();
                let _ = out_tx.send(response(&id, &json!({ "tools": tools })));
            }
            "tools/call" => {
                let Some(name) = params
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                else {
                    let _ = out_tx.send(error(&id, INVALID_PARAMS, "missing tool name"));
                    continue;
                };
                if !handler.tools().iter().any(|t| t.name == name) {
                    let _ =
                        out_tx.send(error(&id, INVALID_PARAMS, &format!("unknown tool: {name}")));
                    continue;
                }
                let arguments = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if !arguments.is_object() {
                    let _ = out_tx.send(error(&id, INVALID_PARAMS, "arguments must be an object"));
                    continue;
                }
                let ctx = CallContext {
                    client: client.lock().map(|c| c.clone()).unwrap_or_default(),
                    connection_id,
                };
                let handler = handler.clone();
                let out = out_tx.clone();
                let running2 = running.clone();
                let key = id_key(&id);
                let key2 = key.clone();
                let abort = tasks.spawn(async move {
                    let result: ToolResult = handler.call(ctx, name, arguments).await;
                    if let Ok(mut r) = running2.lock() {
                        r.remove(&key2);
                    }
                    let value = serde_json::to_value(result).unwrap_or(Value::Null);
                    let _ = out.send(response(&id, &value));
                });
                if let Ok(mut r) = running.lock() {
                    r.insert(key, abort);
                }
            }
            other => {
                let _ = out_tx.send(error(
                    &id,
                    METHOD_NOT_FOUND,
                    &format!("method not found: {other}"),
                ));
            }
        }
        // Reap finished calls so the set does not grow without bound.
        while tasks.try_join_next().is_some() {}
    };

    tasks.abort_all();
    handler.disconnected(connection_id);
    drop(out_tx);
    let _ = writer_task.await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ToolAnnotations;
    use tokio::io::{BufReader, duplex};

    struct Echo;

    #[async_trait::async_trait]
    impl Handler for Echo {
        fn tools(&self) -> Vec<Tool> {
            vec![
                Tool {
                    name: "echo".into(),
                    title: None,
                    description: "Echoes".into(),
                    input_schema: json!({"type": "object"}),
                    output_schema: None,
                    annotations: Some(ToolAnnotations {
                        read_only_hint: Some(true),
                        ..ToolAnnotations::default()
                    }),
                },
                Tool {
                    name: "sleep".into(),
                    title: None,
                    description: "Never returns".into(),
                    input_schema: json!({"type": "object"}),
                    output_schema: None,
                    annotations: None,
                },
            ]
        }

        async fn call(&self, ctx: CallContext, name: String, arguments: Value) -> ToolResult {
            if name == "sleep" {
                std::future::pending::<()>().await;
            }
            ToolResult::json(json!({"client": ctx.client.name, "args": arguments}))
        }
    }

    async fn run(input: &str) -> Vec<Value> {
        let (mut cw, sr) = duplex(1 << 20);
        let (sw, mut cr) = duplex(1 << 20);
        let info = ServerInfo {
            name: "test".into(),
            title: "Test".into(),
            version: "1".into(),
            instructions: Some("be careful".into()),
        };
        let srv = tokio::spawn(serve(Arc::new(Echo), info, 1, BufReader::new(sr), sw));
        cw.write_all(input.as_bytes()).await.unwrap_or_default();
        // Give concurrent calls a moment before closing the input.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        drop(cw);
        let mut out = String::new();
        cr.read_to_string(&mut out).await.unwrap_or_default();
        let _ = srv.await;
        out.lines()
            .map(|l| serde_json::from_str(l).unwrap_or(Value::Null))
            .collect()
    }

    #[tokio::test]
    async fn lifecycle_and_tools() {
        let out = run(concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"tester","version":"0"}}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"x":1}}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":4,"method":"nope"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"missing"}}"#,
            "\n",
            "not json\n",
            r#"{"jsonrpc":"2.0","id":6,"method":"ping"}"#,
            "\n",
        ))
        .await;
        let by_id = |id: i64| {
            out.iter()
                .find(|m| m["id"] == json!(id))
                .cloned()
                .unwrap_or(Value::Null)
        };
        let init = by_id(1);
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(init["result"]["instructions"], "be careful");
        assert_eq!(
            init["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
        assert_eq!(
            by_id(2)["result"]["tools"][0]["annotations"]["readOnlyHint"],
            true
        );
        let call = by_id(3)["result"].clone();
        assert_eq!(call["structuredContent"]["client"], "tester");
        assert_eq!(call["structuredContent"]["args"]["x"], 1);
        assert_eq!(by_id(4)["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(by_id(5)["error"]["code"], INVALID_PARAMS);
        assert_eq!(by_id(6)["result"], json!({}));
        assert!(out.iter().any(|m| m["error"]["code"] == PARSE_ERROR));
    }

    #[tokio::test]
    async fn cancelled_calls_do_not_answer_and_others_still_do() {
        let out = run(concat!(
            r#"{"jsonrpc":"2.0","id":"a","method":"tools/call","params":{"name":"sleep"}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"a"}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":"b","method":"tools/call","params":{"name":"echo","arguments":{}}}"#,
            "\n",
        ))
        .await;
        assert!(out.iter().all(|m| m["id"] != json!("a")));
        assert!(out.iter().any(|m| m["id"] == json!("b")));
    }
}
