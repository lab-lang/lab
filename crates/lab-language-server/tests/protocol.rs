use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn frame(message: Value) -> Vec<u8> {
    let body = serde_json::to_vec(&message).unwrap();
    let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    framed.extend(body);
    framed
}

fn run_server(messages: impl IntoIterator<Item = Value>) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lab-language-server"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for message in messages {
        stdin.write_all(&frame(message)).unwrap();
    }
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    parse_frames(&output.stdout)
}

fn parse_frames(output: &[u8]) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut remaining = output;
    while !remaining.is_empty() {
        let header_end = remaining
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("LSP frame header");
        let header = std::str::from_utf8(&remaining[..header_end]).unwrap();
        let content_length = header
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let body_start = header_end + 4;
        let body_end = body_start + content_length;
        messages.push(serde_json::from_slice(&remaining[body_start..body_end]).unwrap());
        remaining = &remaining[body_end..];
    }
    messages
}

fn response(messages: &[Value], id: i64) -> &Value {
    messages
        .iter()
        .find(|message| message.get("id") == Some(&json!(id)))
        .unwrap_or_else(|| panic!("missing response {id}: {messages:#?}"))
}

#[test]
fn publishes_source_diagnostics_over_stdio() {
    let messages = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "capabilities": {} }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///invalid.lab",
                    "languageId": "lab",
                    "version": 1,
                    "text": "use nowhere\n"
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "shutdown",
            "params": null
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "exit",
            "params": null
        }),
    ];
    let output = run_server(messages);
    let diagnostics = output
        .iter()
        .find(|message| message["method"] == "textDocument/publishDiagnostics")
        .unwrap();
    assert_eq!(diagnostics["params"]["diagnostics"][0]["code"], "semantic");
    assert_eq!(diagnostics["params"]["diagnostics"][0]["severity"], 1);
    assert_eq!(
        diagnostics["params"]["diagnostics"][0]["message"],
        "module 'nowhere' cannot be resolved"
    );
}

#[test]
fn supports_advertised_editor_features_over_stdio() {
    let uri = "file:///features.lab";
    let source = concat!(
        "observation PlateObservation:\n",
        "  image: Image  \n",
        "  colonies: ColonyMap\n",
        "\n",
        "value = PlateObservation\n",
    );
    let mut messages = vec![
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "capabilities": {} }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "lab",
                    "version": 1,
                    "text": source
                }
            }
        }),
    ];
    for (id, method, params) in [
        (
            2,
            "textDocument/completion",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 4, "character": 10 } }),
        ),
        (
            3,
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 4, "character": 10 } }),
        ),
        (
            4,
            "textDocument/definition",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 4, "character": 10 } }),
        ),
        (
            5,
            "textDocument/references",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": 4, "character": 10 },
                "context": { "includeDeclaration": false }
            }),
        ),
        (
            6,
            "textDocument/rename",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": 4, "character": 10 },
                "newName": "AssayObservation"
            }),
        ),
        (
            7,
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        ),
        (
            8,
            "textDocument/semanticTokens/full",
            json!({ "textDocument": { "uri": uri } }),
        ),
        (
            9,
            "textDocument/formatting",
            json!({
                "textDocument": { "uri": uri },
                "options": { "tabSize": 2, "insertSpaces": true }
            }),
        ),
    ] {
        messages.push(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }));
    }
    messages.extend([
        json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "shutdown",
            "params": null
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "exit",
            "params": null
        }),
    ]);

    let output = run_server(messages);
    let capabilities = &response(&output, 1)["result"]["capabilities"];
    assert_eq!(capabilities["hoverProvider"], true);
    assert_eq!(capabilities["renameProvider"], true);
    assert_eq!(capabilities["semanticTokensProvider"]["full"], true);

    assert!(
        response(&output, 2)["result"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "PlateObservation")
    );
    assert!(
        response(&output, 3)["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("Data PlateObservation")
    );
    assert_eq!(response(&output, 4)["result"]["range"]["start"]["line"], 0);
    assert_eq!(response(&output, 5)["result"].as_array().unwrap().len(), 1);
    assert_eq!(
        response(&output, 6)["result"]["documentChanges"][0]["edits"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        response(&output, 7)["result"][0]["children"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        !response(&output, 8)["result"]["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let formatted = response(&output, 9)["result"][0]["newText"]
        .as_str()
        .unwrap();
    assert!(formatted.contains("image: Image\n"));
    assert!(formatted.ends_with('\n'));
}
