use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncRun {
    run_id: Option<String>,
    revision: u64,
    sequence: u64,
    status: Phase,
    logs: Vec<Value>,
    matches: Vec<Value>,
    unmatched: Vec<Value>,
    errors: Vec<Value>,
    result: Option<Value>,
}

impl Default for SyncRun {
    fn default() -> Self {
        Self {
            run_id: None,
            revision: 0,
            sequence: 0,
            status: Phase::Idle,
            logs: vec![],
            matches: vec![],
            unmatched: vec![],
            errors: vec![],
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
        match event {
            "log" => run.logs.push(data.clone()),
            "match" => run.matches.push(data.clone()),
            "unmatched" => run.unmatched.push(data.clone()),
            "error" => run.errors.push(data.clone()),
            _ => {}
        }
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
    fn begin_commit(&self, id: &str, payload: &[Value]) -> Result<SyncRun, String> {
        if payload.is_empty() {
            return Err("Select at least one match to commit".into());
        }
        let mut run = self.current.lock().unwrap();
        if run.run_id.as_deref() != Some(id) {
            return Err("Sync run is no longer current".into());
        }
        if run.status != Phase::Review {
            return Err("Sync run is not in review".into());
        }
        let mut selected = Vec::new();
        for item in payload {
            if !run.matches.contains(item) || selected.contains(&item) {
                return Err(
                    "Commit selection does not belong to this run's analysis results".into(),
                );
            }
            selected.push(item);
        }
        run.status = Phase::Committing;
        run.sequence += 1;
        run.revision += 1;
        Ok(run.clone())
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
    }
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
    run.status = status;
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
    let (kind, payload): (&'static str, Value) = match serde_json::from_str::<Value>(line) {
        Ok(value) => match value.get("type").and_then(Value::as_str) {
            Some("log") => ("log", value),
            Some("match") => ("match", value),
            Some("unmatched") => ("unmatched", value),
            Some("error") => ("error", value),
            _ => ("log", json!({"message": line})),
        },
        Err(_) => ("log", json!({"message": line})),
    };
    publish(c, sink, id, kind, payload);
    kind == "error"
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
            for line in BufReader::new(stream).lines().flatten() {
                publish(
                    &c_err,
                    sink.as_ref(),
                    &id,
                    "error",
                    json!({"message": line}),
                );
            }
        })
    });
    let mut structured_error = false;
    if let Some(stdout) = stdout {
        for line in BufReader::new(stdout).lines().flatten() {
            structured_error |= parse_line(c, sink, id, &line);
        }
    }
    let status = child.wait();
    if let Some(thread) = stderr_thread {
        let _ = thread.join();
    }
    match status {
        Ok(status) if status.success() && !structured_error => terminal(
            c,
            sink,
            id,
            Phase::Analyzing,
            Phase::Review,
            json!({"status":"success"}),
        ),
        Ok(status) if status.success() => terminal(
            c,
            sink,
            id,
            Phase::Analyzing,
            Phase::Failed,
            json!({"status":"error"}),
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
    payload: Vec<Value>,
) -> Result<SyncRun, String> {
    let c = coordinator();
    let run = c.begin_commit(&run_id, &payload)?;
    let id = run_id;
    publish_state(&app, &run);
    let args = vec![
        crate::config::get("harvester_script").to_owned(),
        "--commit".into(),
    ];
    tauri::async_runtime::spawn_blocking(move || {
        commit_worker(c, &app, &id, "python", &args, payload)
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
        c.event(&id, "match", json!({"filename":"synthetic.pdf"}))
            .unwrap();
        c.transition(&id, Phase::Review).unwrap();
        let payload = vec![json!({"filename":"synthetic.pdf"})];
        c.begin_commit(&id, &payload).unwrap();
        assert!(c.begin_commit(&id, &payload).is_err());
        let commit_script = temp.script(&format!(
            "import sys\nopen(sys.argv[1], 'a').write('commit\\n')\nsys.stdin.read()\n"
        ));
        let sink = FakeSink::default();
        commit_worker(
            c.clone(),
            &sink,
            &id,
            &python(),
            &[commit_script, counter],
            payload,
        );
        let launches = fs::read_to_string(temp.0.join("launches.txt")).unwrap();
        assert_eq!(launches.lines().count(), 2);
        assert_eq!(launches.lines().filter(|line| *line == "commit").count(), 1);
    }

    #[test]
    fn commit_is_run_bound_nonempty_and_reserved_once() {
        let c = Arc::new(Coordinator::default());
        let run = start(&c);
        let id = run.run_id.unwrap();
        c.event(&id, "match", json!({"filename":"synthetic.pdf"}))
            .unwrap();
        c.transition(&id, Phase::Review).unwrap();
        assert!(c
            .begin_commit("stale", &[json!({"filename":"synthetic.pdf"})])
            .is_err());
        assert!(c.begin_commit(&id, &[]).is_err());
        assert!(c
            .begin_commit(&id, &[json!({"filename":"forged.pdf"})])
            .is_err());
        assert!(c
            .begin_commit(&id, &[json!({"filename":"synthetic.pdf"})])
            .is_ok());
        assert!(c
            .begin_commit(&id, &[json!({"filename":"synthetic.pdf"})])
            .is_err());
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
    fn fake_commit_worker_is_async_path_and_structured_error_is_failure() {
        let temp = TempDir::new();
        let script = temp.script("import json,sys\nsys.stdin.read()\nprint(json.dumps({'type':'error','message':'synthetic rejected'}))\n");
        let c = Arc::new(Coordinator::default());
        let run = start(&c);
        let id = run.run_id.unwrap();
        c.event(&id, "match", json!({"filename":"synthetic.pdf"}))
            .unwrap();
        c.transition(&id, Phase::Review).unwrap();
        c.begin_commit(&id, &[json!({"filename":"synthetic.pdf"})])
            .unwrap();
        let events = Arc::new(Mutex::new(vec![]));
        let sink = SharedSink(events.clone());
        commit_worker(
            c.clone(),
            &sink,
            &id,
            &python(),
            &[script],
            vec![json!({"filename":"synthetic.pdf"})],
        );
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
        c.event(&id, "match", json!({"filename":"synthetic.pdf"}))
            .unwrap();
        c.transition(&id, Phase::Review).unwrap();
        let sink = FakeSink::default();
        assert!(c
            .event(&id, "match", json!({"filename":"late.pdf"}))
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
        let item = json!({"filename":"synthetic.pdf","detail":"x".repeat(128 * 1024)});
        c.event(&id, "match", item.clone()).unwrap();
        c.transition(&id, Phase::Review).unwrap();
        c.begin_commit(&id, &[item.clone()]).unwrap();
        let events = Arc::new(Mutex::new(vec![]));
        let sink = SharedSink(events.clone());
        let worker = std::thread::spawn({
            let c = c.clone();
            let script = script.clone();
            let id = id.clone();
            let item = item.clone();
            move || commit_worker(c, &sink, &id, &python(), &[script], vec![item])
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
