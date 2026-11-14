//! A small Model Context Protocol server over stdio: JSON-RPC 2.0, one message on each line. It serves tools
//! only: initialize, tools/list, tools/call and ping. A notification from the client needs no reply and is
//! dropped. Ported from contr's `internal/mcp`. The calls run one after the other, in the order they come.
use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

/// The newest version that this server speaks. A client that asks for another known version gets that
/// version; any other client gets this one.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
const KNOWN_VERSIONS: [&str; 3] = ["2024-11-05", "2025-03-26", PROTOCOL_VERSION];

/// The work of a tool: the arguments in, the text of the result out.
pub type Run<'a> = Box<dyn Fn(&Value) -> Result<String, String> + 'a>;

/// One tool. `schema` is the JSON Schema of its arguments. An error of `run` becomes a tool result with
/// isError set, so the model sees it.
pub struct Tool<'a> {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
    pub run: Run<'a>,
}

fn tool_result(text: String, is_error: bool) -> Value {
    let mut r = json!({"content": [{"type": "text", "text": text}]});
    if is_error {
        r["isError"] = json!(true);
    }
    r
}

/// The result of a request, or its error: the code and the message.
fn handle(name: &str, version: &str, tools: &[Tool], method: &str, params: &Value) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or_default();
            let v = if KNOWN_VERSIONS.contains(&asked) { asked } else { PROTOCOL_VERSION };
            Ok(json!({"protocolVersion": v, "capabilities": {"tools": {}}, "serverInfo": {"name": name, "version": version}}))
        }
        "ping" => Ok(json!({})),
        "tools/list" => {
            let list: Vec<Value> = tools.iter().map(|t| json!({"name": t.name, "description": t.description, "inputSchema": t.schema})).collect();
            Ok(json!({"tools": list}))
        }
        "tools/call" => {
            if !params.is_object() {
                return Err((-32602, "bad params: not an object".into()));
            }
            let tool = params["name"].as_str().unwrap_or_default();
            let Some(t) = tools.iter().find(|t| t.name == tool) else {
                return Err((-32602, format!("unknown tool: {tool}")));
            };
            let args = match &params["arguments"] {
                Value::Null => json!({}),
                a => a.clone(),
            };
            Ok(match (t.run)(&args) {
                Ok(text) => tool_result(text, false),
                Err(e) => tool_result(format!("{tool}: {e}"), true),
            })
        }
        _ => Err((-32601, format!("method not found: {method}"))),
    }
}

/// Reads requests from `input` and writes each reply to `out`, until `input` ends.
pub fn serve(name: &str, version: &str, tools: &[Tool], input: &mut dyn BufRead, out: &mut dyn Write) -> io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Err(e) => json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}}),
            // A message with no id is a notification.
            Ok(req) if req["id"].is_null() => continue,
            Ok(req) => match handle(name, version, tools, req["method"].as_str().unwrap_or_default(), &req["params"]) {
                Ok(result) => json!({"jsonrpc": "2.0", "id": req["id"], "result": result}),
                Err((code, message)) => json!({"jsonrpc": "2.0", "id": req["id"], "error": {"code": code, "message": message}}),
            },
        };
        writeln!(out, "{reply}")?;
        out.flush()?;
    }
}
