use crate::{
    error::EmberlaneError,
    model::{ChatMessage, ChatRequest},
    router::RuntimeRouter,
};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub async fn run_stdio(router: RuntimeRouter) -> Result<(), EmberlaneError> {
    let mut reader = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = reader.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(value) => dispatch_value(router.clone(), value).await,
            Err(err) => protocol_error(Value::Null, -32700, &format!("parse error: {err}")),
        };
        stdout.write_all(response.to_string().as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }
    Ok(())
}

pub async fn dispatch_value(router: RuntimeRouter, value: Value) -> Value {
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    let method = value.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "serverInfo": {"name": "emberlane", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"tools": {}}
            }
        }),
        "notifications/initialized" => json!({"jsonrpc":"2.0","id": id,"result": null}),
        "tools/list" => json!({"jsonrpc":"2.0","id": id,"result": {"tools": tools()}}),
        "tools/call" => {
            let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            json!({"jsonrpc":"2.0","id": id,"result": tool_result(call_tool(router, name, args).await)})
        }
        "" => protocol_error(id, -32600, "invalid JSON-RPC request"),
        other => protocol_error(id, -32601, &format!("unknown method: {other}")),
    }
}

fn protocol_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id": id, "error": {"code": code, "message": message}})
}

fn tool_result(value: Result<Value, EmberlaneError>) -> Value {
    match value {
        Ok(value) => json!({
            "content": [{"type": "text", "text": serde_json::to_string_pretty(&value).unwrap()}],
            "isError": false
        }),
        Err(err) => json!({
            "content": [{"type": "text", "text": err.to_string()}],
            "isError": true
        }),
    }
}

async fn call_tool(
    router: RuntimeRouter,
    name: &str,
    args: Value,
) -> Result<Value, EmberlaneError> {
    match name {
        "emberlane_list_runtimes" => router.list_runtimes().map(|v| json!(v)),
        "emberlane_status" => {
            if let Some(runtime_id) = args.get("runtime_id").and_then(Value::as_str) {
                router.status(runtime_id).await.map(|v| json!(v))
            } else {
                router.list_status().await.map(|v| json!(v))
            }
        }
        "emberlane_chat" => {
            let runtime_id = required(&args, "runtime_id")?;
            let message = required(&args, "message")?;
            router
                .chat(
                    runtime_id,
                    ChatRequest {
                        messages: vec![ChatMessage {
                            role: "user".to_string(),
                            content: message.to_string(),
                        }],
                        files: vec![],
                    },
                )
                .await
                .map(|v| v.body)
        }
        "emberlane_upload_file" => {
            let path = required(&args, "path")?;
            router.upload_path(path).await.map(|v| json!(v))
        }
        "emberlane_chat_file" => {
            let runtime_id = required(&args, "runtime_id")?;
            let file_id = required(&args, "file_id")?;
            let message = required(&args, "message")?;
            router
                .chat_file(runtime_id, file_id, message)
                .await
                .map(|v| v.body)
        }
        "emberlane_wake" => {
            let runtime_id = required(&args, "runtime_id")?;
            router
                .wake(runtime_id)
                .await
                .map(|_| json!({"runtime_id": runtime_id, "state": "ready"}))
        }
        "emberlane_sleep" => {
            let runtime_id = required(&args, "runtime_id")?;
            router
                .sleep(runtime_id)
                .await
                .map(|_| json!({"runtime_id": runtime_id, "state": "cold"}))
        }
        _ => Err(EmberlaneError::InvalidRequest(format!(
            "unknown MCP tool: {name}"
        ))),
    }
}

fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str, EmberlaneError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| EmberlaneError::InvalidRequest(format!("{key} is required")))
}

fn tools() -> Value {
    json!([
        {"name":"emberlane_list_runtimes","description":"List Emberlane runtimes","inputSchema":{"type":"object","properties":{}}},
        {"name":"emberlane_status","description":"Get runtime status","inputSchema":{"type":"object","properties":{"runtime_id":{"type":"string"}}}},
        {"name":"emberlane_chat","description":"Chat through a runtime","inputSchema":{"type":"object","properties":{"runtime_id":{"type":"string"},"message":{"type":"string"}},"required":["runtime_id","message"]}},
        {"name":"emberlane_upload_file","description":"Upload a local .txt or .md file","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}},
        {"name":"emberlane_chat_file","description":"Chat with uploaded file context","inputSchema":{"type":"object","properties":{"runtime_id":{"type":"string"},"file_id":{"type":"string"},"message":{"type":"string"}},"required":["runtime_id","file_id","message"]}},
        {"name":"emberlane_wake","description":"Wake a runtime","inputSchema":{"type":"object","properties":{"runtime_id":{"type":"string"}},"required":["runtime_id"]}},
        {"name":"emberlane_sleep","description":"Sleep a runtime","inputSchema":{"type":"object","properties":{"runtime_id":{"type":"string"}},"required":["runtime_id"]}}
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::EmberlaneConfig, model::ProviderKind, storage::Storage};

    fn test_router() -> RuntimeRouter {
        let mut cfg = EmberlaneConfig::default();
        cfg.runtimes[0].provider = ProviderKind::Mock;
        cfg.runtimes[0].config = json!({});
        cfg.runtimes.truncate(1);
        let storage = Storage::open_memory().unwrap();
        let router = RuntimeRouter::new(cfg, storage);
        router.seed_config_runtimes().unwrap();
        router
    }

    #[tokio::test]
    async fn initialize_works() {
        let out = dispatch_value(
            test_router(),
            json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        )
        .await;
        assert_eq!(out["id"], 1);
    }

    #[tokio::test]
    async fn tools_list_includes_expected_tools() {
        let out = dispatch_value(
            test_router(),
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        )
        .await;
        let tools = out["result"]["tools"].as_array().unwrap();
        let names = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "emberlane_list_runtimes",
                "emberlane_status",
                "emberlane_chat",
                "emberlane_upload_file",
                "emberlane_chat_file",
                "emberlane_wake",
                "emberlane_sleep"
            ]
        );
    }

    #[tokio::test]
    async fn tools_call_chat_works() {
        let out = dispatch_value(
            test_router(),
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"emberlane_chat","arguments":{"runtime_id":"echo","message":"hello"}}}),
        )
        .await;
        assert_eq!(out["result"]["isError"], false);
    }

    #[tokio::test]
    async fn tools_list_omits_non_core_surfaces() {
        let out = dispatch_value(
            test_router(),
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        )
        .await;
        let text = out.to_string();
        assert!(!text.contains("presign"));
        assert!(!text.contains("emberlane_route"));
        assert!(!text.contains("search"));
        assert!(!text.contains("index"));
    }
}
