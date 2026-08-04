use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn frame(message: Value) -> Vec<u8> {
    let body = serde_json::to_vec(&message).unwrap();
    let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    framed.extend(body);
    framed
}

#[test]
fn publishes_source_diagnostics_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lab-language-server"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
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
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("textDocument/publishDiagnostics"));
    assert!(stdout.contains("module 'nowhere' cannot be resolved"));
    assert!(stdout.contains("\"code\":\"semantic\""));
}
