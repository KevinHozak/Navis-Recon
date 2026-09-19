use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter};

const SCRIPT_PATH: &str = r"C:\Users\khoza\.gemini\config\plugins\workflow-tools-plugin\skills\codex-harvester\scripts\finance_processor.py";

fn emit(app: &AppHandle, event: &str, payload: Value) {
    let _ = app.emit(event, payload);
}

fn emit_error(app: &AppHandle, message: impl Into<String>) {
    emit(app, "sync:error", json!({ "message": message.into() }));
}

fn process_stdout_line(app: &AppHandle, line: &str) {
    match serde_json::from_str::<Value>(line) {
        Ok(payload) => match payload.get("type").and_then(Value::as_str) {
            Some("log") => emit(app, "sync:log", payload),
            Some("match") => emit(app, "sync:match", payload),
            Some("unmatched") => emit(app, "sync:unmatched", payload),
            Some("error") => emit(app, "sync:error", payload),
            _ => emit(app, "sync:log", json!({ "message": line })),
        },
        Err(_) => emit(app, "sync:log", json!({ "message": line })),
    }
}

fn run_analysis(app: AppHandle) {
    let mut child = match Command::new("python")
        .arg(SCRIPT_PATH)
        .arg("--analyze")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            emit_error(&app, format!("Unable to start Python harvester: {error}"));
            emit(&app, "sync:done", json!({ "status": "error" }));
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stderr_app = app.clone();
    if let Some(stderr) = stderr {
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().flatten() {
                emit_error(&stderr_app, line);
            }
        });
    }

    if let Some(stdout) = stdout {
        for line in BufReader::new(stdout).lines().flatten() {
            process_stdout_line(&app, &line);
        }
    }

    match child.wait() {
        Ok(status) if status.success() => emit(&app, "sync:done", json!({ "status": "success" })),
        Ok(status) => {
            emit_error(&app, format!("Harvester exited with status {status}"));
            emit(&app, "sync:done", json!({ "status": "error" }));
        }
        Err(error) => {
            emit_error(&app, format!("Unable to finish harvester: {error}"));
            emit(&app, "sync:done", json!({ "status": "error" }));
        }
    }
}

#[tauri::command]
pub fn start_ledger_sync(app: AppHandle) -> Result<(), String> {
    if !std::path::Path::new(SCRIPT_PATH).exists() {
        return Err(format!("Harvester script not found: {SCRIPT_PATH}"));
    }
    tauri::async_runtime::spawn_blocking(move || run_analysis(app));
    Ok(())
}

#[tauri::command]
pub fn commit_ledger_sync(payload: Vec<Value>) -> Result<Value, String> {
    let mut child = Command::new("python")
        .arg(SCRIPT_PATH)
        .arg("--commit")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Unable to start Python commit process: {error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let serialized = serde_json::to_vec(&payload)
            .map_err(|error| format!("Unable to serialize commit payload: {error}"))?;
        stdin
            .write_all(&serialized)
            .map_err(|error| format!("Unable to send commit payload: {error}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("Unable to finish Python commit process: {error}"))?;
    let mut results = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            results.push(value);
        } else if !line.trim().is_empty() {
            results.push(json!({ "type": "log", "message": line }));
        }
    }

    if !output.status.success() {
        return Err(format!(
            "Commit process failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(json!({ "status": "success", "results": results }))
}
