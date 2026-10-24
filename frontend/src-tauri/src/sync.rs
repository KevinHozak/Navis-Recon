use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Phase {
    Idle,
    Analyzing,
    Review,
    Committing,
    Complete,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum AnalysisOutcome {
    Success,
    Partial,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncRun {
    run_id: Option<String>,
    revision: u64,
    sequence: u64,
    status: Phase,
    analysis_outcome: Option<AnalysisOutcome>,
    logs: Vec<Value>,
    matches: Vec<Value>,
    unmatched: Vec<Value>,
    errors: Vec<Value>,
    eligible_match_ids: Vec<String>,
    result: Option<Value>,
}

impl Default for SyncRun {
    fn default() -> Self {
        Self {
            run_id: None,
            revision: 0,
            sequence: 0,
            status: Phase::Idle,
            analysis_outcome: None,
            logs: vec![],
            matches: vec![],
            unmatched: vec![],
            errors: vec![],
            eligible_match_ids: vec![],
            result: None,
        }
    }
}

#[derive(Default)]
struct Coordinator {
    current: Mutex<SyncRun>,
    next_id: Mutex<u64>,
}

impl Coordinator {
    fn snapshot(&self) -> SyncRun {
        self.current.lock().unwrap().clone()
    }
    fn reserve(&self, phase: Phase) -> Result<SyncRun, String> {
        let mut run = self.current.lock().unwrap();
        if !matches!(
            run.status,
            Phase::Idle | Phase::Complete | Phase::Failed | Phase::Cancelled
        ) {
            return Err(format!("Cannot start while sync run is {:?}", run.status));
        }
        let mut n = self.next_id.lock().unwrap();
        *n += 1;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        *run = SyncRun {
            run_id: Some(format!("sync-{}-{timestamp}-{n}", std::process::id())),
            revision: run.revision + 1,
            sequence: 0,
            status: phase,
            ..SyncRun::default()
        };
        Ok(run.clone())
    }
    #[cfg(test)]
    fn transition(&self, id: &str, status: Phase) -> Option<SyncRun> {
        let mut run = self.current.lock().unwrap();
        if run.run_id.as_deref() != Some(id) {
            return None;
        }
        run.status = status;
        if run.status == Phase::Review {
            run.analysis_outcome = Some(AnalysisOutcome::Success);
        }
        run.sequence += 1;
        run.revision += 1;
        Some(run.clone())
    }
    fn event(&self, id: &str, event: &str, payload: Value) -> Option<(SyncRun, Value)> {
        let mut run = self.current.lock().unwrap();
        if run.run_id.as_deref() != Some(id) {
            return None;
        }
        let accepted = match run.status {
            Phase::Analyzing => matches!(event, "log" | "match" | "unmatched" | "error"),
            Phase::Committing => matches!(event, "log" | "error"),
            _ => false,
        };
        if !accepted {
            return None;
        }
        let mut data = payload;
        strip_worker_identity(&mut data);
        if event == "match" {
            if !valid_match(&data) {
                return None;
            }
            let canonical = serde_json::to_vec(&data).ok()?;
            let digest = sha2::Sha256::digest(canonical);
            data["match_id"] = json!(format!("{}:{}", id, hex(&digest)));
            if run
                .matches
                .iter()
                .any(|existing| existing.get("match_id") == data.get("match_id"))
            {
                return None;
            }
        }
        match event {
            "log" => run.logs.push(data.clone()),
            "match" => run.matches.push(data.clone()),
            "unmatched" => run.unmatched.push(sanitize_diagnostic(data.clone())),
            "error" => run.errors.push(sanitize_diagnostic(data.clone())),
            _ => {}
        }
        refresh_eligible(&mut run);
        run.sequence += 1;
        run.revision += 1;
        let envelope = json!({
            "run_id": id,
            "sequence": run.sequence,
            "revision": run.revision,
            "status": format!("{:?}", run.status).to_lowercase(),
            "payload": data,
            "snapshot": run.clone(),
        });
        Some((run.clone(), envelope))
    }
    fn begin_commit(&self, id: &str, ids: &[String]) -> Result<(SyncRun, Vec<Value>), String> {
        if ids.is_empty() {
            return Err("Select at least one match to commit".into());
        }
        let mut run = self.current.lock().unwrap();
        if run.run_id.as_deref() != Some(id) {
            return Err("Sync run is no longer current".into());
        }
        if run.status != Phase::Review {
            return Err("Sync run is not in review".into());
        }
        if !matches!(
            run.analysis_outcome,
            Some(AnalysisOutcome::Success | AnalysisOutcome::Partial)
        ) {
            return Err("Sync analysis has no positive outcome and cannot be committed".into());
        }
        let mut selected_ids = Vec::new();
        let mut selected = Vec::new();
        for match_id in ids {
            if selected_ids.contains(match_id) {
                return Err(
                    "Commit selection does not belong to this run's analysis results".into(),
                );
            }
            let mut resolved = run
                .matches
                .iter()
                .filter(|m| {
                    run.eligible_match_ids
                        .iter()
                        .any(|eligible| m["match_id"] == *eligible)
                })
                .find(|m| m["match_id"] == *match_id)
                .cloned()
                .ok_or("Commit selection does not belong to this run's analysis results")?;
            resolved
                .as_object_mut()
                .ok_or("Malformed stored match")?
                .remove("match_id");
            selected.push(resolved);
            selected_ids.push(match_id.clone());
        }
        run.status = Phase::Committing;
        run.sequence += 1;
        run.revision += 1;
        Ok((run.clone(), selected))
    }
    fn cancel_review(&self, id: &str) -> Result<SyncRun, String> {
        let mut run = self.current.lock().unwrap();
        if run.run_id.as_deref() != Some(id) {
            return Err("Sync run is no longer current".into());
        }
        if run.status != Phase::Review {
            return Err("Only a review run can be abandoned".into());
        }
        run.status = Phase::Cancelled;
        run.sequence += 1;
        run.revision += 1;
        Ok(run.clone())
    }
}

fn strip_worker_identity(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("run_id");
        object.remove("revision");
        object.remove("sequence");
        object.remove("match_id");
    }
}

fn valid_match(value: &Value) -> bool {
    value.is_object()
        && value
            .get("filename")
            .and_then(Value::as_str)
            .is_some_and(|v| !v.trim().is_empty())
        && value
            .get("new_name")
            .and_then(Value::as_str)
            .is_some_and(|v| !v.trim().is_empty())
}

fn refresh_eligible(run: &mut SyncRun) {
    if run.errors.iter().any(|item| {
        item.get("protocol_error").and_then(Value::as_bool) == Some(true)
            || item
                .get("filename")
                .and_then(Value::as_str)
                .is_none_or(|name| name.trim().is_empty())
    }) || run.unmatched.iter().any(|item| {
        item.get("filename")
            .and_then(Value::as_str)
            .is_none_or(|name| name.trim().is_empty())
    }) {
        run.eligible_match_ids.clear();
        return;
    }
    let blocked: std::collections::HashSet<&str> = run
        .errors
        .iter()
        .chain(run.unmatched.iter())
        .filter_map(|item| item.get("filename").and_then(Value::as_str))
        .collect();
    let mut counts = std::collections::HashMap::<&str, usize>::new();
    for item in &run.matches {
        if let Some(filename) = item.get("filename").and_then(Value::as_str) {
            *counts.entry(filename).or_default() += 1;
        }
    }
    run.eligible_match_ids = run
        .matches
        .iter()
        .filter_map(|item| {
            let filename = item.get("filename")?.as_str()?;
            let id = item.get("match_id")?.as_str()?;
            (!blocked.contains(filename) && counts.get(filename) == Some(&1)).then(|| id.to_owned())
        })
        .collect();
}

fn sanitize_diagnostic(value: Value) -> Value {
    if !value.is_object()
        || value.get("filename").is_some_and(|v| !v.is_string())
        || value.get("message").is_some_and(|v| !v.is_string())
    {
        return json!({"message":"Malformed diagnostic record", "protocol_error":true});
    }
    value
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

static COORDINATOR: OnceLock<Arc<Coordinator>> = OnceLock::new();
fn coordinator() -> Arc<Coordinator> {
    COORDINATOR
        .get_or_init(|| Arc::new(Coordinator::default()))
        .clone()
}

trait EventSink: Send + Sync {
    fn emit(&self, event: &str, payload: Value);
    fn duplicate(&self) -> Box<dyn EventSink>;
}
impl EventSink for AppHandle {
    fn emit(&self, event: &str, payload: Value) {
        let _ = Emitter::emit(self, event, payload);
    }
    fn duplicate(&self) -> Box<dyn EventSink> {
        Box::new(self.clone())
    }
}

fn publish(c: &Coordinator, sink: &dyn EventSink, id: &str, kind: &str, payload: Value) {
    if let Some((_, envelope)) = c.event(id, kind, payload) {
        sink.emit(&format!("sync:{kind}"), envelope);
    }
}

fn publish_state(sink: &dyn EventSink, run: &SyncRun) {
    sink.emit(
        "sync:state",
        serde_json::to_value(run).unwrap_or_else(|_| json!({})),
    );
}

fn terminal(
    c: &Coordinator,
    sink: &dyn EventSink,
    id: &str,
    expected: Phase,
    status: Phase,
    payload: Value,
) {
    let mut run = c.current.lock().unwrap();
    if run.run_id.as_deref() != Some(id) || run.status != expected {
        return;
    }
    run.status = status.clone();
    if expected == Phase::Analyzing {
        run.analysis_outcome = Some(if status == Phase::Failed {
            AnalysisOutcome::Failed
        } else {
            match payload
                .get("analysis_outcome")
                .and_then(Value::as_str)
                .unwrap_or("success")
            {
                "partial" => AnalysisOutcome::Partial,
                "failed" => AnalysisOutcome::Failed,
                _ => AnalysisOutcome::Success,
            }
        });
    }
    run.sequence += 1;
    run.revision += 1;
    let mut envelope = json!({
        "run_id": id,
        "sequence": run.sequence,
        "revision": run.revision,
        "status": format!("{:?}", run.status).to_lowercase(),
        "payload": payload,
    });
    run.result = envelope.get("payload").cloned();
    envelope["snapshot"] = serde_json::to_value(&*run).unwrap_or_else(|_| json!({}));
    drop(run);
    sink.emit("sync:done", envelope);
}

fn parse_line(c: &Coordinator, sink: &dyn EventSink, id: &str, line: &str) -> bool {
    let parsed = serde_json::from_str::<Value>(line);
    let (kind, payload): (&'static str, Value) = match parsed {
        Ok(value) => match value.get("type").and_then(Value::as_str) {
            Some("log") => ("log", value),
            Some("match") => ("match", value),
            Some("unmatched") => ("unmatched", value),
            Some("error") => ("error", value),
            _ if value.is_object() || value.is_array() => {
                publish(
                    c,
                    sink,
                    id,
                    "error",
                    json!({"message":"Unknown structured analyzer record type", "protocol_error":true}),
                );
                return true;
            }
            _ => ("log", json!({"message": line})),
        },
        Err(_) if line.trim_start().starts_with(['{', '[']) => {
            publish(
                c,
                sink,
                id,
                "error",
                json!({"message":"Malformed structured analyzer output", "protocol_error":true}),
            );
            return true;
        }
        Err(_) => ("log", json!({"message": line})),
    };
    if kind == "match" && !valid_match(&payload) {
        publish(
            c,
            sink,
            id,
            "error",
            json!({"message":"Malformed match: filename and new_name must be nonempty strings", "filename":payload.get("filename").cloned().unwrap_or(Value::Null), "protocol_error":true}),
        );
        return true;
    }
    publish(c, sink, id, kind, payload);
    false
}

fn analyze_worker(
    coordinator: Arc<Coordinator>,
    sink: &dyn EventSink,
    id: &str,
    program: &str,
    args: &[String],
) {
    let c = coordinator.as_ref();
    let spawn = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawn {
        Ok(child) => child,
        Err(e) => {
            publish(
                c,
                sink,
                id,
                "error",
                json!({"message": format!("Unable to start harvester: {e}")}),
            );
            terminal(
                c,
                sink,
                id,
                Phase::Analyzing,
                Phase::Failed,
                json!({"status":"error"}),
            );
            return;
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let c_err = coordinator.clone();
    let stderr_thread = stderr.map(|stream| {
        let id = id.to_owned();
        let sink = sink.duplicate();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines() {
                match line {
                    Ok(line) => publish(&c_err, sink.as_ref(), &id, "log", json!({"message": line, "level":"warning", "source":"stderr"})),
                    Err(error) => publish(&c_err, sink.as_ref(), &id, "error", json!({"message":format!("Unable to read analyzer stderr: {error}"), "protocol_error":true})),
                }
            }
        })
    });
    let mut fatal = false;
    if let Some(stdout) = stdout {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(line) => fatal |= parse_line(c, sink, id, &line),
                Err(error) => {
                    fatal = true;
                    publish(
                        c,
                        sink,
                        id,
                        "error",
                        json!({"message":format!("Unable to read analyzer output: {error}"), "protocol_error":true}),
                    );
                }
            }
        }
    }
    let status = child.wait();
    if let Some(thread) = stderr_thread {
        let _ = thread.join();
    }
    match status {
        Ok(status) if status.success() && !fatal => {
            let snapshot = c.snapshot();
            let global_error = snapshot.errors.iter().any(|e| {
                e.get("protocol_error").and_then(Value::as_bool) == Some(true)
                    || e.get("filename")
                        .and_then(Value::as_str)
                        .is_none_or(|f| f.trim().is_empty())
            }) || snapshot.unmatched.iter().any(|item| {
                item.get("filename")
                    .and_then(Value::as_str)
                    .is_none_or(|f| f.trim().is_empty())
            });
            let diagnostics = !snapshot.errors.is_empty() || !snapshot.unmatched.is_empty();
            let outcome = if global_error || (diagnostics && snapshot.eligible_match_ids.is_empty())
            {
                "failed"
            } else if diagnostics {
                "partial"
            } else {
                "success"
            };
            terminal(
                c,
                sink,
                id,
                Phase::Analyzing,
                if outcome == "failed" {
                    Phase::Failed
                } else {
                    Phase::Review
                },
                json!({"status":outcome,"analysis_outcome":outcome}),
            );
        }
        Ok(status) if status.success() => terminal(
            c,
            sink,
            id,
            Phase::Analyzing,
            Phase::Failed,
            json!({"status":"error","analysis_outcome":"failed"}),
        ),
        Ok(status) => {
            publish(
                c,
                sink,
                id,
                "error",
                json!({"message": format!("Harvester exited with status {status}")}),
            );
            terminal(
                c,
                sink,
                id,
                Phase::Analyzing,
                Phase::Failed,
                json!({"status":"error"}),
            );
        }
        Err(e) => {
            publish(
                c,
                sink,
                id,
                "error",
                json!({"message": format!("Unable to finish harvester: {e}")}),
            );
            terminal(
                c,
                sink,
                id,
                Phase::Analyzing,
                Phase::Failed,
                json!({"status":"error"}),
            );
        }
    }
}

fn commit_worker(
    coordinator: Arc<Coordinator>,
    sink: &dyn EventSink,
    id: &str,
    program: &str,
    args: &[String],
    payload: Vec<Value>,
) {
    let c = coordinator.as_ref();
    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            publish(
                c,
                sink,
                id,
                "error",
                json!({"message":format!("Unable to start commit process: {e}")}),
            );
            terminal(
                c,
                sink,
                id,
                Phase::Committing,
                Phase::Failed,
                json!({"status":"error"}),
            );
            return;
        }
    };
    let bytes = match serde_json::to_vec(&payload) {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            publish(
                c,
                sink,
                id,
                "error",
                json!({"message":format!("Unable to encode commit payload: {error}")}),
            );
            terminal(
                c,
                sink,
                id,
                Phase::Committing,
                Phase::Failed,
                json!({"status":"error"}),
            );
            return;
        }
    };
    let stdin = child.stdin.take();
    let writer = std::thread::spawn(move || -> Result<(), String> {
        let mut stdin = stdin.ok_or("missing stdin".to_string())?;
        stdin.write_all(&bytes).map_err(|error| error.to_string())
    });
    let output = child.wait_with_output();
    let send = writer
        .join()
        .unwrap_or_else(|_| Err("commit input writer panicked".into()));
    match (send, output) {
        (_, Err(e)) => {
            publish(
                c,
                sink,
                id,
                "error",
                json!({"message":format!("Unable to finish commit process: {e}")}),
            );
            terminal(
                c,
                sink,
                id,
                Phase::Committing,
                Phase::Failed,
                json!({"status":"error"}),
            );
        }
        (Err(e), Ok(_out)) => {
            publish(
                c,
                sink,
                id,
                "error",
                json!({"message":format!("Unable to send commit payload: {e}")}),
            );
            terminal(
                c,
                sink,
                id,
                Phase::Committing,
                Phase::Failed,
                json!({"status":"error"}),
            );
        }
        (Ok(()), Ok(out)) => {
            for line in String::from_utf8_lossy(&out.stderr).lines() {
                if !line.trim().is_empty() {
                    publish(c, sink, id, "error", json!({"message":line}));
                }
            }
            let mut results = vec![];
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if let Ok(mut value) = serde_json::from_str::<Value>(line) {
                    strip_worker_identity(&mut value);
                    if value.get("type").and_then(Value::as_str) == Some("error") {
                        publish(c, sink, id, "error", value.clone());
                    } else if value.get("type").and_then(Value::as_str) == Some("log") {
                        publish(c, sink, id, "log", value.clone());
                    }
                    results.push(value);
                } else if !line.trim().is_empty() {
                    let value = json!({"type":"log","message":line});
                    publish(c, sink, id, "log", value.clone());
                    results.push(value);
                }
            }
            if !out.status.success() {
                publish(
                    c,
                    sink,
                    id,
                    "error",
                    json!({"message":format!("Commit process failed: {}",String::from_utf8_lossy(&out.stderr).trim())}),
                );
                terminal(
                    c,
                    sink,
                    id,
                    Phase::Committing,
                    Phase::Failed,
                    json!({"status":"error","results":results}),
                );
            } else if results
                .iter()
                .any(|v| v.get("type").and_then(Value::as_str) == Some("error"))
            {
                terminal(
                    c,
                    sink,
                    id,
                    Phase::Committing,
                    Phase::Failed,
                    json!({"status":"error","results":results}),
                );
            } else {
                terminal(
                    c,
                    sink,
                    id,
                    Phase::Committing,
                    Phase::Complete,
                    json!({"status":"success","results":results}),
                );
            }
        }
    }
}

#[tauri::command]
pub fn get_sync_run() -> SyncRun {
    coordinator().snapshot()
}

#[tauri::command]
pub fn start_ledger_sync(app: AppHandle) -> Result<SyncRun, String> {
    let c = coordinator();
    let run = c.reserve(Phase::Analyzing)?;
    let id = run.run_id.clone().unwrap();
    publish_state(&app, &run);
    let program = "python".to_owned();
    let args = vec![
        crate::config::get("harvester_script").to_owned(),
        "--analyze".into(),
    ];
    tauri::async_runtime::spawn_blocking(move || analyze_worker(c, &app, &id, &program, &args));
    Ok(run)
}

#[tauri::command]
pub fn commit_ledger_sync(
    app: AppHandle,
    run_id: String,
    payload: Vec<String>,
) -> Result<SyncRun, String> {
    let c = coordinator();
    let (run, original) = c.begin_commit(&run_id, &payload)?;
    let id = run_id;
    publish_state(&app, &run);
    let args = vec![
        crate::config::get("harvester_script").to_owned(),
        "--commit".into(),
    ];
    tauri::async_runtime::spawn_blocking(move || {
        commit_worker(c, &app, &id, "python", &args, original)
    });
    Ok(run)
}

#[tauri::command]
pub fn cancel_sync_run(app: AppHandle, run_id: String) -> Result<SyncRun, String> {
    let run = coordinator().cancel_review(&run_id)?;
    publish_state(&app, &run);
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc::{self, Receiver, Sender},
            Arc, Mutex,
        },
        time::Duration,
    };
    static TEMP_ID: AtomicU64 = AtomicU64::new(0);
    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "navis-recon-sync-{}-{}",
                std::process::id(),
                TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn script(&self, body: &str) -> String {
            let path = self.0.join("fake_harvester.py");
            fs::write(&path, body).unwrap();
            path.to_string_lossy().into_owned()
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[derive(Default)]
    struct FakeSink(Mutex<Vec<(String, Value)>>);
    impl EventSink for FakeSink {
        fn emit(&self, event: &str, payload: Value) {
            self.0.lock().unwrap().push((event.into(), payload));
        }
        fn duplicate(&self) -> Box<dyn EventSink> {
            Box::new(FakeSink::default())
        }
    }
    // Tests that need to inspect the stderr reader use an Arc-backed concrete sink.
    struct SharedSink(Arc<Mutex<Vec<(String, Value)>>>);
    impl EventSink for SharedSink {
        fn emit(&self, event: &str, payload: Value) {
            self.0.lock().unwrap().push((event.into(), payload));
        }
        fn duplicate(&self) -> Box<dyn EventSink> {
            Box::new(SharedSink(self.0.clone()))
        }
    }
    struct DelayedSink {
        events: Arc<Mutex<Vec<(String, Value)>>>,
        reached: Sender<()>,
        release: Arc<Mutex<Receiver<()>>>,
    }
    impl EventSink for DelayedSink {
        fn emit(&self, event: &str, payload: Value) {
            if event == "sync:done" {
                let _ = self.reached.send(());
                let _ = self.release.lock().unwrap().recv();
            }
            self.events.lock().unwrap().push((event.into(), payload));
        }
        fn duplicate(&self) -> Box<dyn EventSink> {
            Box::new(Self {
                events: self.events.clone(),
                reached: self.reached.clone(),
                release: self.release.clone(),
            })
        }
    }
    fn python() -> String {
        if cfg!(windows) {
            "python".to_owned()
        } else {
            "python3".to_owned()
        }
    }
    fn start(c: &Coordinator) -> SyncRun {
        c.reserve(Phase::Analyzing).unwrap()
    }
    fn seed_review(c: &Coordinator, id: &str, mut item: Value) -> (String, Value) {
        item["type"] = json!("match");
        item["filename"] = json!("synthetic.pdf");
        item["new_name"] = json!("renamed.pdf");
        c.event(id, "match", item).unwrap();
        let stored = c.snapshot().matches[0].clone();
        c.transition(id, Phase::Review).unwrap();
        (stored["match_id"].as_str().unwrap().to_owned(), stored)
    }

    #[test]
    fn analysis_reservation_rejects_duplicate_and_review_blocks_new_run() {
        let c = Arc::new(Coordinator::default());
        let first = start(&c);
        assert!(c.reserve(Phase::Analyzing).is_err());
        c.transition(first.run_id.as_deref().unwrap(), Phase::Review)
            .unwrap();
        assert!(c.reserve(Phase::Analyzing).is_err());
    }

    #[test]
    fn accepted_analysis_and_commit_each_spawn_one_fake_child() {
        let temp = TempDir::new();
        let counter = temp
            .0
            .join("launches.txt")
            .to_string_lossy()
            .replace('\\', "\\\\");
        let script = temp.script(&format!(
            "import sys\nopen(sys.argv[1], 'a').write('analysis\\n')\n"
        ));
        let c = Arc::new(Coordinator::default());
        let run = start(&c);
        let id = run.run_id.unwrap();
        assert!(c.reserve(Phase::Analyzing).is_err());
        let sink = FakeSink::default();
        analyze_worker(c.clone(), &sink, &id, &python(), &[script, counter.clone()]);
        assert_eq!(
            fs::read_to_string(temp.0.join("launches.txt"))
                .unwrap()
                .lines()
                .count(),
            1
        );

        let c = Arc::new(Coordinator::default());
        let run = start(&c);
        let id = run.run_id.unwrap();
        let (match_id, _) = seed_review(&c, &id, json!({}));
        let ids = vec![match_id];
        let (_, resolved) = c.begin_commit(&id, &ids).unwrap();
        assert!(c.begin_commit(&id, &ids).is_err());
        let commit_script = temp.script(&format!(
            "import sys\nopen(sys.argv[1], 'a').write('commit\\n')\ndata=sys.stdin.read()\nopen(sys.argv[1]+'.payload','w').write(data)\n"
        ));
        let sink = FakeSink::default();
        commit_worker(
            c.clone(),
            &sink,
            &id,
            &python(),
            &[commit_script, counter.clone()],
            resolved.clone(),
        );
        let launches = fs::read_to_string(temp.0.join("launches.txt")).unwrap();
        assert_eq!(launches.lines().count(), 2);
        assert_eq!(launches.lines().filter(|line| *line == "commit").count(), 1);
        let received: Value =
            serde_json::from_str(&fs::read_to_string(format!("{}.payload", counter)).unwrap())
                .unwrap();
        assert_eq!(received, json!(resolved));
    }

    #[test]
    fn commit_is_run_bound_nonempty_and_reserved_once() {
        let c = Arc::new(Coordinator::default());
        let run = start(&c);
        let id = run.run_id.unwrap();
        let (match_id, _) = seed_review(&c, &id, json!({}));
        assert!(c.begin_commit("stale", &[match_id.clone()]).is_err());
        assert!(c.begin_commit(&id, &[]).is_err());
        assert!(c.begin_commit(&id, &["forged".to_owned()]).is_err());
        assert!(c
            .begin_commit(&id, &[match_id.clone(), match_id.clone()])
            .is_err());
        assert!(c.begin_commit(&id, &[match_id.clone()]).is_ok());
        assert!(c.begin_commit(&id, &[match_id]).is_err());
    }

    #[test]
    fn backend_identity_replay_and_commit_payload_are_authoritative() {
        let c = Coordinator::default();
        let first = start(&c);
        let id = first.run_id.unwrap();
        let original = json!({"filename":"synthetic.pdf","new_name":"renamed.pdf","run_id":"spoof","revision":999,"sequence":888,"match_id":"child"});
        let (snap, _) = c.event(&id, "match", original.clone()).unwrap();
        let backend_id = snap.matches[0]["match_id"].as_str().unwrap().to_owned();
        let revision = snap.revision;
        assert!(c.event(&id, "match", original.clone()).is_none());
        assert_eq!(c.snapshot().revision, revision);
        let changed = json!({"filename":"other-source.pdf","new_name":"other.pdf"});
        c.event(&id, "match", changed).unwrap();
        c.transition(&id, Phase::Review).unwrap();
        let resolved = c.begin_commit(&id, &[backend_id.clone()]).unwrap().1;
        assert_eq!(
            resolved,
            vec![json!({"filename":"synthetic.pdf","new_name":"renamed.pdf"})]
        );
        assert!(resolved[0].get("match_id").is_none());
        let other = Coordinator::default();
        let next = start(&other);
        other
            .event(next.run_id.as_deref().unwrap(), "match", original)
            .unwrap();
        assert_ne!(other.snapshot().matches[0]["match_id"], backend_id);
    }

    #[test]
    fn invalid_utf8_after_useful_match_fails_and_retains_diagnostic() {
        let temp = TempDir::new();
        let script = temp.script("import sys\nsys.stdout.buffer.write(b'{\\\"type\\\":\\\"match\\\",\\\"filename\\\":\\\"synthetic.pdf\\\",\\\"new_name\\\":\\\"renamed.pdf\\\"}\\n\\xff\\n')\n");
        let c = Arc::new(Coordinator::default());
        let id = start(&c).run_id.unwrap();
        analyze_worker(c.clone(), &FakeSink::default(), &id, &python(), &[script]);
        let snapshot = c.snapshot();
        assert_eq!(snapshot.status, Phase::Failed);
        assert_eq!(snapshot.matches.len(), 1);
        assert!(!snapshot.errors.is_empty());
        assert!(snapshot.eligible_match_ids.is_empty());
    }

    #[test]
    fn review_can_be_abandoned_but_active_run_cannot_be_marked_cancelled() {
        let c = Coordinator::default();
        let run = start(&c);
        let id = run.run_id.unwrap();
        assert!(c.cancel_review(&id).is_err());
        c.transition(&id, Phase::Review).unwrap();
        let cancelled = c.cancel_review(&id).unwrap();
        assert_eq!(cancelled.status, Phase::Cancelled);
        assert!(c.reserve(Phase::Analyzing).is_ok());
    }

    #[test]
    fn fake_worker_keeps_backend_identity_and_reconnect_snapshot_has_results() {
        let temp = TempDir::new();
        let script = temp.script("import json,sys\nprint(json.dumps({'type':'match','run_id':'fake','sequence':999,'filename':'synthetic.pdf','new_name':'renamed.pdf'}))\n");
        let c = Arc::new(Coordinator::default());
        let run = start(&c);
        let id = run.run_id.unwrap();
        let events = Arc::new(Mutex::new(vec![]));
        let sink = SharedSink(events.clone());
        analyze_worker(
            c.clone(),
            &sink,
            &id,
            &python(),
            &[script, "--analyze".into()],
        );
        let snapshot = c.snapshot();
        assert_eq!(snapshot.status, Phase::Review);
        assert_eq!(snapshot.matches.len(), 1);
        assert_eq!(snapshot.matches[0]["filename"], "synthetic.pdf");
        assert!(snapshot.matches[0].get("run_id").is_none());
        let emitted = events.lock().unwrap();
        let matched = emitted
            .iter()
            .find(|(name, _)| name == "sync:match")
            .unwrap();
        assert_eq!(matched.1["run_id"], id);
        assert_eq!(matched.1["sequence"], 1);
        assert_eq!(matched.1["payload"]["filename"], "synthetic.pdf");
        assert!(matched.1["payload"].get("run_id").is_none());
        assert_eq!(
            matched.1["snapshot"]["matches"][0]["filename"],
            "synthetic.pdf"
        );
        assert_eq!(matched.1["snapshot"]["revision"], matched.1["revision"]);
        assert_eq!(emitted.last().unwrap().0, "sync:done");
        assert_eq!(emitted.last().unwrap().1["snapshot"]["status"], "review");
        assert_eq!(
            emitted.last().unwrap().1["snapshot"]["matches"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            emitted.last().unwrap().1["snapshot"]["result"]["status"],
            "success"
        );
    }

    #[test]
    fn stale_output_and_terminal_worker_cannot_mutate_next_run() {
        let c = Arc::new(Coordinator::default());
        let old = start(&c);
        let old_id = old.run_id.unwrap();
        c.transition(&old_id, Phase::Failed).unwrap();
        let next = start(&c);
        let next_id = next.run_id.unwrap();
        let sink = FakeSink::default();
        publish(
            &c,
            &sink,
            &old_id,
            "match",
            json!({"type":"match","filename":"stale.pdf"}),
        );
        terminal(
            &c,
            &sink,
            &old_id,
            Phase::Analyzing,
            Phase::Complete,
            json!({"status":"success"}),
        );
        let current = c.snapshot();
        assert_eq!(current.run_id.as_deref(), Some(next_id.as_str()));
        assert_eq!(current.status, Phase::Analyzing);
        assert!(current.matches.is_empty());
    }

    #[test]
    fn late_real_worker_completion_is_ignored_after_new_run_reservation() {
        let temp = TempDir::new();
        let script = temp.script("import sys\nsys.exit(3)\n");
        let c = Arc::new(Coordinator::default());
        let first = start(&c);
        let first_id = first.run_id.unwrap();
        let events = Arc::new(Mutex::new(vec![]));
        let (reached_tx, reached_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let sink = Arc::new(DelayedSink {
            events: events.clone(),
            reached: reached_tx,
            release: Arc::new(Mutex::new(release_rx)),
        });
        let worker_c = c.clone();
        let worker_sink = sink.clone();
        let worker_script = script.clone();
        let worker_id = first_id.clone();
        let worker = std::thread::spawn(move || {
            analyze_worker(
                worker_c,
                worker_sink.as_ref(),
                &worker_id,
                &python(),
                &[worker_script],
            );
        });
        reached_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(c.snapshot().status, Phase::Failed);
        let next = start(&c);
        let next_id = next.run_id.unwrap();
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        let current = c.snapshot();
        assert_eq!(current.run_id.as_deref(), Some(next_id.as_str()));
        assert_eq!(current.status, Phase::Analyzing);
        let events = events.lock().unwrap();
        assert_eq!(events.last().unwrap().1["run_id"], first_id);
    }

    #[test]
    fn failed_spawn_and_nonzero_exit_finish_failed_after_stderr_drain() {
        let c = Arc::new(Coordinator::default());
        let run = start(&c);
        let id = run.run_id.unwrap();
        let events = Arc::new(Mutex::new(vec![]));
        let sink = SharedSink(events.clone());
        analyze_worker(c.clone(), &sink, &id, "missing-navis-sync-program", &[]);
        assert_eq!(c.snapshot().status, Phase::Failed);
        assert_eq!(events.lock().unwrap().last().unwrap().0, "sync:done");

        let c = Arc::new(Coordinator::default());
        let run = start(&c);
        let id = run.run_id.unwrap();
        let temp = TempDir::new();
        let script =
            temp.script("import sys\nsys.stderr.write('synthetic stderr final\\n')\nsys.exit(7)\n");
        let events = Arc::new(Mutex::new(vec![]));
        let sink = SharedSink(events.clone());
        analyze_worker(c.clone(), &sink, &id, &python(), &[script]);
        assert_eq!(c.snapshot().status, Phase::Failed);
        let events = events.lock().unwrap();
        let stderr = events
            .iter()
            .position(|(_, p)| p["payload"]["message"] == "synthetic stderr final")
            .unwrap();
        let done = events
            .iter()
            .position(|(name, _)| name == "sync:done")
            .unwrap();
        assert!(stderr < done);

        let c = Arc::new(Coordinator::default());
        let run = start(&c);
        let id = run.run_id.unwrap();
        let temp = TempDir::new();
        let script = temp.script("import json\nprint(json.dumps({'type':'error','message':'synthetic analysis failure'}))\n");
        let sink = FakeSink::default();
        analyze_worker(c.clone(), &sink, &id, &python(), &[script]);
        assert_eq!(c.snapshot().status, Phase::Failed);
    }

    #[test]
    fn analysis_outcome_classifies_empty_partial_global_and_protocol_failures() {
        let cases = [
            ("", Phase::Review, "success", 0),
            ("print(json.dumps({'type':'match','filename':'a.pdf','new_name':'b.pdf'}))\nprint(json.dumps({'type':'unmatched','filename':'c.pdf','message':'no rule'}))", Phase::Review, "partial", 1),
            ("print(json.dumps({'type':'match','filename':'a.pdf','new_name':'b.pdf'}))\nprint(json.dumps({'type':'unmatched','filename':'a.pdf','message':'no rule'}))", Phase::Failed, "failed", 1),
            ("print(json.dumps({'type':'match','filename':'a.pdf','new_name':'b.pdf'}))\nprint(json.dumps({'type':'error','filename':'a.pdf','message':'file issue'}))", Phase::Failed, "failed", 1),
            ("print(json.dumps({'type':'match','filename':'a.pdf','new_name':'b.pdf'}))\nprint(json.dumps({'type':'error','message':'global issue'}))", Phase::Failed, "failed", 1),
            ("print(json.dumps({'type':'error','filename':'a.pdf','message':'file issue'}))", Phase::Failed, "failed", 0),
            ("print(json.dumps({'type':'match','filename':'a.pdf','new_name':'b.pdf'}))\nprint(json.dumps({'type':'error','filename':'a.pdf','message':{'unsafe':'object'}}))", Phase::Failed, "failed", 1),
            ("print(json.dumps({'type':'match','filename':'a.pdf','new_name':'b.pdf'}))\nsys.exit(4)", Phase::Failed, "failed", 1),
            ("print(json.dumps({'type':'match','filename':'a.pdf'}))", Phase::Failed, "failed", 0),
            ("print('{broken json')", Phase::Failed, "failed", 0),
        ];
        for (body, expected_status, expected_outcome, expected_matches) in cases {
            let temp = TempDir::new();
            let script = temp.script(&format!("import json,sys\n{body}\n"));
            let c = Arc::new(Coordinator::default());
            let id = start(&c).run_id.unwrap();
            analyze_worker(c.clone(), &FakeSink::default(), &id, &python(), &[script]);
            let snapshot = c.snapshot();
            assert_eq!(snapshot.status, expected_status, "fixture: {body}");
            assert_eq!(
                snapshot.analysis_outcome,
                Some(match expected_outcome {
                    "success" => AnalysisOutcome::Success,
                    "partial" => AnalysisOutcome::Partial,
                    _ => AnalysisOutcome::Failed,
                })
            );
            assert_eq!(snapshot.matches.len(), expected_matches, "fixture: {body}");
        }
    }

    #[test]
    fn partial_commit_excludes_diagnostic_sources_and_conflicting_duplicates() {
        let temp = TempDir::new();
        let script = temp.script("import json\nprint(json.dumps({'type':'match','filename':'a.pdf','new_name':'a-new.pdf'}))\nprint(json.dumps({'type':'match','filename':'b.pdf','new_name':'b-new.pdf'}))\nprint(json.dumps({'type':'error','filename':'a.pdf','message':'blocked'}))\n");
        let c = Arc::new(Coordinator::default());
        let id = start(&c).run_id.unwrap();
        analyze_worker(c.clone(), &FakeSink::default(), &id, &python(), &[script]);
        let snapshot = c.snapshot();
        assert_eq!(snapshot.analysis_outcome, Some(AnalysisOutcome::Partial));
        assert_eq!(snapshot.eligible_match_ids.len(), 1);
        assert_eq!(
            snapshot
                .matches
                .iter()
                .find(|m| m["filename"] == "b.pdf")
                .unwrap()["match_id"],
            snapshot.eligible_match_ids[0]
        );
        let (committing, payload) = c.begin_commit(&id, &snapshot.eligible_match_ids).unwrap();
        assert_eq!(committing.status, Phase::Committing);
        assert_eq!(payload.len(), 1);
        assert_eq!(payload[0]["filename"], "b.pdf");

        let c = Coordinator::default();
        let id = start(&c).run_id.unwrap();
        let (match_id, _) = seed_review(&c, &id, json!({}));
        c.current.lock().unwrap().analysis_outcome = Some(AnalysisOutcome::Failed);
        assert!(c.begin_commit(&id, &[match_id]).is_err());

        let c = Coordinator::default();
        let id = start(&c).run_id.unwrap();
        c.event(
            &id,
            "match",
            json!({"filename":"same.pdf","new_name":"one.pdf"}),
        )
        .unwrap();
        c.event(
            &id,
            "match",
            json!({"filename":"same.pdf","new_name":"two.pdf"}),
        )
        .unwrap();
        c.transition(&id, Phase::Review).unwrap();
        let snapshot = c.snapshot();
        assert!(snapshot.eligible_match_ids.is_empty());
        assert!(c
            .begin_commit(
                &id,
                &[snapshot.matches[0]["match_id"].as_str().unwrap().to_owned()]
            )
            .is_err());
    }

    #[test]
    fn fake_commit_worker_is_async_path_and_structured_error_is_failure() {
        let temp = TempDir::new();
        let script = temp.script("import json,sys\nsys.stdin.read()\nprint(json.dumps({'type':'error','message':'synthetic rejected'}))\n");
        let c = Arc::new(Coordinator::default());
        let run = start(&c);
        let id = run.run_id.unwrap();
        let (match_id, _) = seed_review(&c, &id, json!({}));
        let (_, resolved) = c.begin_commit(&id, &[match_id]).unwrap();
        let events = Arc::new(Mutex::new(vec![]));
        let sink = SharedSink(events.clone());
        commit_worker(c.clone(), &sink, &id, &python(), &[script], resolved);
        assert_eq!(c.snapshot().status, Phase::Failed);
        assert!(events
            .lock()
            .unwrap()
            .iter()
            .any(|(_, p)| p["payload"]["message"] == "synthetic rejected"));
    }

    #[test]
    fn late_events_and_wrong_phase_terminal_cannot_reopen_or_mutate_review() {
        let c = Coordinator::default();
        let run = start(&c);
        let id = run.run_id.unwrap();
        c.event(
            &id,
            "match",
            json!({"filename":"synthetic.pdf", "new_name":"renamed.pdf"}),
        )
        .unwrap();
        c.transition(&id, Phase::Review).unwrap();
        let sink = FakeSink::default();
        assert!(c
            .event(
                &id,
                "match",
                json!({"filename":"late.pdf", "new_name":"late.pdf"})
            )
            .is_none());
        terminal(
            &c,
            &sink,
            &id,
            Phase::Analyzing,
            Phase::Failed,
            json!({"status":"error"}),
        );
        let state = c.snapshot();
        assert_eq!(state.status, Phase::Review);
        assert_eq!(state.matches.len(), 1);
    }

    #[test]
    fn commit_drains_large_child_output_while_sending_stdin() {
        let temp = TempDir::new();
        let script = temp.script(
            r#"import sys,threading,os
watchdog=threading.Timer(10, lambda: os._exit(3))
watchdog.daemon=True
watchdog.start()
sys.stdout.write('x' * 262144 + '\n')
sys.stdout.flush()
data=sys.stdin.read()
print('received:' + str(len(data)))
"#,
        );
        let c = Arc::new(Coordinator::default());
        let run = start(&c);
        let id = run.run_id.unwrap();
        let item = json!({"filename":"synthetic.pdf","new_name":"renamed.pdf","detail":"x".repeat(128 * 1024)});
        let (match_id, _) = seed_review(&c, &id, item);
        let (_, resolved) = c.begin_commit(&id, &[match_id]).unwrap();
        let events = Arc::new(Mutex::new(vec![]));
        let sink = SharedSink(events.clone());
        let worker = std::thread::spawn({
            let c = c.clone();
            let script = script.clone();
            let id = id.clone();
            move || commit_worker(c, &sink, &id, &python(), &[script], resolved)
        });
        assert!(worker.join().is_ok());
        assert_eq!(c.snapshot().status, Phase::Complete);
        assert!(events
            .lock()
            .unwrap()
            .iter()
            .any(|(_, event)| event["payload"]["results"]
                .as_array()
                .map(|rows| rows.iter().any(|result| result["message"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("received:")))
                .unwrap_or(false)));
    }
}
