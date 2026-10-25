use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;
#[cfg(test)]
use std::io::Write;
#[cfg(test)]
use std::process::{Command, Stdio};
use std::{
    collections::{HashMap, HashSet},
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
    #[serde(default)]
    cancelling: bool,
    analysis_outcome: Option<AnalysisOutcome>,
    logs: Vec<Value>,
    matches: Vec<Value>,
    unmatched: Vec<Value>,
    errors: Vec<Value>,
    eligible_match_ids: Vec<String>,
    result: Option<Value>,
    #[serde(default)]
    managed_outcomes: Vec<crate::managed_import::ItemOutcome>,
    #[serde(default)]
    operation_kind: Option<String>,
    #[serde(default)]
    started_at_ms: Option<u64>,
    #[serde(default)]
    elapsed_ms: Option<u64>,
    #[serde(default)]
    receipt_id: Option<String>,
    #[serde(default)]
    receipt_error: Option<String>,
}

impl Default for SyncRun {
    fn default() -> Self {
        Self {
            run_id: None,
            revision: 0,
            sequence: 0,
            status: Phase::Idle,
            cancelling: false,
            analysis_outcome: None,
            logs: vec![],
            matches: vec![],
            unmatched: vec![],
            errors: vec![],
            eligible_match_ids: vec![],
            result: None,
            managed_outcomes: vec![],
            operation_kind: None,
            started_at_ms: None,
            elapsed_ms: None,
            receipt_id: None,
            receipt_error: None,
        }
    }
}

#[derive(Default)]
struct Coordinator {
    current: Mutex<SyncRun>,
    next_id: Mutex<u64>,
    plans: Mutex<HashMap<String, crate::managed_import::PreparedPlan>>,
    import_config: Mutex<Option<crate::managed_import::ImportConfig>>,
    cancel_requested: Mutex<Option<String>>,
    receipt_store: Mutex<Option<Arc<dyn crate::sync_receipts::ReceiptStore>>>,
    phase_started: Mutex<HashMap<String, std::time::Instant>>,
    finalizing: Mutex<HashSet<String>>,
}

impl Coordinator {
    fn configure_receipts(&self) {
        if let Ok(mut slot) = self.receipt_store.lock() {
            *slot = crate::sync_receipts::FileStore::production()
                .ok()
                .map(|s| Arc::new(s) as Arc<dyn crate::sync_receipts::ReceiptStore>);
        }
    }
    fn snapshot(&self) -> SyncRun {
        self.current.lock().unwrap().clone()
    }
    fn reserve_analysis(
        &self,
        cfg: crate::managed_import::ImportConfig,
    ) -> Result<SyncRun, String> {
        let mut run = self
            .current
            .lock()
            .map_err(|_| "Sync coordinator unavailable")?;
        if !matches!(
            run.status,
            Phase::Idle | Phase::Complete | Phase::Failed | Phase::Cancelled
        ) {
            return Err(format!("Cannot start while sync run is {:?}", run.status));
        }
        let mut config = self
            .import_config
            .lock()
            .map_err(|_| "Managed configuration unavailable")?;
        let mut plans = self.plans.lock().map_err(|_| "Managed plans unavailable")?;
        let mut n = self
            .next_id
            .lock()
            .map_err(|_| "Sync coordinator unavailable")?;
        *n += 1;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let revision = run.revision + 1;
        *run = SyncRun {
            run_id: Some(format!("sync-{}-{timestamp}-{n}", std::process::id())),
            revision,
            sequence: 0,
            status: Phase::Analyzing,
            started_at_ms: Some(crate::sync_receipts::now_ms()),
            ..SyncRun::default()
        };
        if let Some(id) = run.run_id.as_ref() {
            self.phase_started
                .lock()
                .unwrap()
                .insert(id.clone(), std::time::Instant::now());
        }
        *config = Some(cfg);
        plans.clear();
        Ok(run.clone())
    }
    fn reserve_retry(&self) -> Result<SyncRun, String> {
        let mut run = self
            .current
            .lock()
            .map_err(|_| "Sync coordinator unavailable")?;
        if !matches!(
            run.status,
            Phase::Idle | Phase::Complete | Phase::Failed | Phase::Cancelled
        ) {
            return Err(format!("Cannot retry while sync run is {:?}", run.status));
        }
        let mut n = self
            .next_id
            .lock()
            .map_err(|_| "Sync coordinator unavailable")?;
        *n += 1;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let revision = run.revision + 1;
        *run = SyncRun {
            run_id: Some(format!("sync-{}-{timestamp}-{n}", std::process::id())),
            revision,
            sequence: 0,
            status: Phase::Committing,
            analysis_outcome: Some(AnalysisOutcome::Success),
            operation_kind: Some("managed_retry".into()),
            started_at_ms: Some(crate::sync_receipts::now_ms()),
            ..SyncRun::default()
        };
        if let Some(id) = run.run_id.as_ref() {
            self.phase_started
                .lock()
                .unwrap()
                .insert(id.clone(), std::time::Instant::now());
        }
        Ok(run.clone())
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
            started_at_ms: Some(crate::sync_receipts::now_ms()),
            ..SyncRun::default()
        };
        if let Some(id) = run.run_id.as_ref() {
            self.phase_started
                .lock()
                .unwrap()
                .insert(id.clone(), std::time::Instant::now());
        }
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
            if let Some(cfg) = self.import_config.lock().ok()?.as_ref() {
                match crate::managed_import::prepare(&data, cfg) {
                    Ok(plan) => {
                        self.plans
                            .lock()
                            .ok()?
                            .insert(data["match_id"].as_str()?.to_owned(), plan);
                    }
                    Err(message) => run
                        .errors
                        .push(json!({"filename":data["filename"], "message":message})),
                }
            }
            attach_source_ref(&mut data, id);
        }
        match event {
            "log" => {
                run.logs.push(data.clone());
                if run.logs.len() > crate::sync_process::MAX_RETAINED_LOGS {
                    run.logs.remove(0);
                }
            }
            "match" => run.matches.push(data.clone()),
            "unmatched" => {
                let mut item = sanitize_diagnostic(data.clone());
                attach_source_ref(&mut item, id);
                run.unmatched.push(item)
            }
            "error" => {
                let mut item = sanitize_diagnostic(data.clone());
                attach_source_ref(&mut item, id);
                run.errors.push(item)
            }
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
            resolved.as_object_mut().unwrap().remove("source_ref");
            resolved["_managed_item_id"] = json!(match_id);
            selected.push(resolved);
            selected_ids.push(match_id.clone());
        }
        run.status = Phase::Committing;
        run.started_at_ms = Some(crate::sync_receipts::now_ms());
        run.elapsed_ms = None;
        run.receipt_id = None;
        run.receipt_error = None;
        if let Ok(mut timers) = self.phase_started.lock() {
            timers.insert(id.to_owned(), std::time::Instant::now());
        }
        run.sequence += 1;
        run.revision += 1;
        Ok((run.clone(), selected))
    }
    fn begin_managed_commit(
        &self,
        id: &str,
        ids: &[String],
    ) -> Result<
        (
            SyncRun,
            Vec<(String, crate::managed_import::PreparedPlan)>,
            crate::managed_import::ImportConfig,
        ),
        String,
    > {
        if ids.is_empty() {
            return Err("Select at least one match to commit".into());
        }
        let mut run = self
            .current
            .lock()
            .map_err(|_| "Sync coordinator unavailable")?;
        if run.run_id.as_deref() != Some(id) || run.status != Phase::Review {
            return Err("Sync run is no longer in review".into());
        }
        if !matches!(
            run.analysis_outcome,
            Some(AnalysisOutcome::Success | AnalysisOutcome::Partial)
        ) {
            return Err("Sync analysis has no positive outcome and cannot be committed".into());
        }
        let cfg = self
            .import_config
            .lock()
            .map_err(|_| "Managed configuration unavailable")?
            .as_ref()
            .cloned()
            .ok_or("Managed configuration unavailable")?;
        let plans = self.plans.lock().map_err(|_| "Managed plans unavailable")?;
        let selected = ids
            .iter()
            .map(|item_id| {
                plans
                    .get(item_id)
                    .cloned()
                    .map(|plan| (item_id.clone(), plan))
                    .ok_or("A selected match has no valid analysis-time prepared plan")
            })
            .collect::<Result<Vec<_>, _>>()?;
        if ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len() {
            return Err("Commit selection contains duplicate items".into());
        }
        for item_id in ids {
            if !run.eligible_match_ids.contains(item_id) {
                return Err(
                    "Commit selection does not belong to this run's analysis results".into(),
                );
            }
        }
        run.status = Phase::Committing;
        run.started_at_ms = Some(crate::sync_receipts::now_ms());
        run.elapsed_ms = None;
        run.receipt_id = None;
        run.receipt_error = None;
        if let Ok(mut timers) = self.phase_started.lock() {
            timers.insert(id.to_owned(), std::time::Instant::now());
        }
        run.sequence += 1;
        run.revision += 1;
        Ok((run.clone(), selected, cfg))
    }
    #[cfg(test)]
    fn approved_config(&self) -> Result<crate::managed_import::ImportConfig, String> {
        let cfg = self
            .import_config
            .lock()
            .map_err(|_| "Managed configuration unavailable")?;
        let cfg = cfg.as_ref().ok_or("Managed configuration unavailable")?;
        Ok(cfg.clone())
    }
    fn configure_import(&self, cfg: crate::managed_import::ImportConfig) {
        if let Ok(mut slot) = self.import_config.lock() {
            *slot = Some(cfg);
        }
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
    fn request_cancel(&self, id: &str) -> Result<SyncRun, String> {
        let mut run = self
            .current
            .lock()
            .map_err(|_| "Sync coordinator unavailable")?;
        if run.run_id.as_deref() != Some(id) {
            return Err("Sync run is no longer current".into());
        }
        if run.status == Phase::Review {
            run.status = Phase::Cancelled;
        } else if matches!(run.status, Phase::Analyzing | Phase::Committing) {
            let mut requested = self
                .cancel_requested
                .lock()
                .map_err(|_| "Sync coordinator unavailable")?;
            if requested.as_deref() != Some(id) {
                *requested = Some(id.to_owned());
                run.cancelling = true;
                run.sequence += 1;
                run.revision += 1;
            }
        } else if run.status != Phase::Cancelled {
            return Err("This sync run cannot be cancelled".into());
        }
        Ok(run.clone())
    }
    fn is_cancelled(&self, id: &str) -> bool {
        self.cancel_requested
            .lock()
            .is_ok_and(|requested| requested.as_deref() == Some(id))
    }
}

fn attach_source_ref(item: &mut Value, run_id: &str) {
    if let Some(filename) = item
        .get("filename")
        .and_then(Value::as_str)
        .filter(|n| crate::managed_import::safe_name(n))
    {
        item["source_ref"] = json!(source_ref(run_id, filename));
    }
}

#[tauri::command]
pub fn reveal_sync_source(
    source_ref: String,
    run_id: Option<String>,
    item_id: Option<String>,
    operation_id: Option<String>,
) -> Result<(), String> {
    let c = coordinator();
    if let Some(operation_id) = operation_id {
        if run_id.is_some() || item_id.is_some() {
            return Err("Durable source is unavailable".into());
        }
        if source_ref != format!("src:{}", operation_id.to_lowercase()) {
            return Err("Durable source is unavailable".into());
        }
        let bound = c
            .import_config
            .lock()
            .map_err(|_| "Managed configuration unavailable")?
            .clone();
        let cfg = crate::managed_import::bound_or_load_config(
            bound,
            crate::managed_import::runtime_config,
        )?;
        let path = crate::managed_import::source_for_operation(&operation_id, &cfg)?;
        return reveal_path(path);
    }
    let run = c
        .current
        .lock()
        .map_err(|_| "Sync coordinator unavailable")?;
    if run_id.as_deref() != run.run_id.as_deref() {
        return Err("Source is no longer available".into());
    }
    let filename = if let Some(item_id) = item_id.as_deref() {
        run.matches
            .iter()
            .find(|m| {
                m["match_id"] == item_id
                    && item_id.starts_with(&format!("{}:", run_id.as_deref().unwrap_or("")))
            })
            .and_then(|m| m["filename"].as_str())
    } else {
        run.unmatched
            .iter()
            .chain(run.errors.iter())
            .find(|m| m["source_ref"] == source_ref)
            .and_then(|m| m["filename"].as_str())
    }
    .ok_or("Source is no longer available")?
    .to_owned();
    if source_ref != source_ref_fn(run_id.as_deref().unwrap_or(""), &filename)
        || operation_id.is_some()
    {
        return Err("Source is no longer available".into());
    }
    drop(run);
    let cfg = c
        .import_config
        .lock()
        .map_err(|_| "Managed configuration unavailable")?
        .clone()
        .ok_or("Managed configuration unavailable")?;
    if !crate::managed_import::safe_name(&filename) {
        return Err("Source file is unavailable".into());
    }
    let original_path = cfg.inbox.join(&filename);
    crate::managed_import::reject_reparse_ancestry(&original_path)
        .map_err(|_| "Source file is unavailable".to_string())?;
    let path = std::fs::canonicalize(&original_path).map_err(|_| "Source file is unavailable")?;
    if !path.starts_with(&cfg.inbox) || !path.is_file() {
        return Err("Source file is unavailable".into());
    }
    if let Some(item_id) = item_id.as_deref() {
        let plans = c.plans.lock().map_err(|_| "Managed plans unavailable")?;
        let plan = plans.get(item_id).ok_or("Source is no longer available")?;
        let approved =
            std::fs::canonicalize(plan.source_path()).map_err(|_| "Source file is unavailable")?;
        if approved != path {
            return Err("Source is no longer available".into());
        }
    }
    #[cfg(windows)]
    crate::managed_import::reject_reparse_ancestry(&path)
        .map_err(|_| "Source file is unavailable".to_string())?;
    reveal_path(path)
}

fn reveal_path(path: std::path::PathBuf) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        reveal_path_with(path, |canonical| {
            std::process::Command::new("explorer.exe")
                .arg(format!("/select,{}", canonical.display()))
                .creation_flags(0x08000000)
                .spawn()
                .map(|_| ())
                .map_err(|_| "Unable to open source location".to_string())
        })
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Source reveal is supported only on Windows".into())
    }
}

fn reveal_path_with<F>(canonical: std::path::PathBuf, opener: F) -> Result<(), String>
where
    F: FnOnce(std::path::PathBuf) -> Result<(), String>,
{
    opener(canonical)
}

fn source_ref_fn(run_id: &str, filename: &str) -> String {
    source_ref(run_id, filename)
}

fn strip_worker_identity(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("run_id");
        object.remove("revision");
        object.remove("sequence");
        object.remove("match_id");
        object.remove("source_ref");
    }
}

fn source_ref(run_id: &str, filename: &str) -> String {
    format!(
        "src:{}",
        hex(&sha2::Sha256::digest(
            format!("{run_id}\0{filename}").as_bytes()
        ))
    )
}

fn valid_match(value: &Value) -> bool {
    value.is_object()
        && value
            .get("filename")
            .and_then(Value::as_str)
            .is_some_and(crate::managed_import::safe_name)
        && value
            .get("new_name")
            .and_then(Value::as_str)
            .is_some_and(crate::managed_import::safe_name)
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
    let mut counts = std::collections::HashMap::<String, usize>::new();
    let mut destination_counts = std::collections::HashMap::<String, usize>::new();
    for item in &run.matches {
        if let Some(filename) = item.get("filename").and_then(Value::as_str) {
            *counts.entry(filename.to_lowercase()).or_default() += 1;
        }
        if let Some(name) = item.get("new_name").and_then(Value::as_str) {
            *destination_counts.entry(name.to_lowercase()).or_default() += 1;
        }
    }
    run.eligible_match_ids = run
        .matches
        .iter()
        .filter_map(|item| {
            let filename = item.get("filename")?.as_str()?;
            let destination = item.get("new_name")?.as_str()?;
            let id = item.get("match_id")?.as_str()?;
            (!blocked.contains(filename)
                && counts.get(&filename.to_lowercase()) == Some(&1)
                && destination_counts.get(&destination.to_lowercase()) == Some(&1))
            .then(|| id.to_owned())
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
    let phase_key = format!("{id}:{expected:?}");
    {
        let run = c.current.lock().unwrap();
        if run.run_id.as_deref() != Some(id) || run.status != expected {
            return;
        }
        let mut finalizing = c.finalizing.lock().unwrap();
        if !finalizing.insert(phase_key.clone()) {
            return;
        }
    }
    // Build and publish the immutable redacted record without holding the
    // coordinator lock. Worker identity and every untrusted source identifier
    // are hashed before serialization.
    let (receipt_id, receipt_error, elapsed) = {
        let snapshot = c.snapshot();
        let operation_kind = if matches!(expected, Phase::Analyzing | Phase::Review) {
            "analysis"
        } else if snapshot.operation_kind.as_deref() == Some("managed_retry") {
            "retry"
        } else {
            "commit"
        };
        let elapsed = c
            .phase_started
            .lock()
            .unwrap()
            .get(id)
            .map(|t| t.elapsed().as_millis().min(u64::MAX as u128) as u64)
            .unwrap_or(0);
        let started = snapshot
            .started_at_ms
            .unwrap_or_else(crate::sync_receipts::now_ms);
        let ended = crate::sync_receipts::now_ms().max(started);
        let terminal_state = match status {
            Phase::Review => "review",
            Phase::Complete => "complete",
            Phase::Failed => "failed",
            Phase::Cancelled => "cancelled",
            _ => "failed",
        };
        let mut items = Vec::new();
        for (i, item) in snapshot.matches.iter().enumerate() {
            items.push(crate::sync_receipts::ReceiptItem {
                item_id: crate::sync_receipts::opaque_id(&format!(
                    "{id}:match:{i}:{}",
                    item.get("match_id").and_then(Value::as_str).unwrap_or("")
                )),
                operation_id: None,
                state: "matched".into(),
                retry_safe: false,
            });
        }
        for (group, rows, state) in [
            ("unmatched", &snapshot.unmatched, "unmatched"),
            ("error", &snapshot.errors, "error"),
        ] {
            for (i, item) in rows.iter().enumerate() {
                items.push(crate::sync_receipts::ReceiptItem {
                    item_id: crate::sync_receipts::opaque_id(&format!(
                        "{id}:{group}:{i}:{}",
                        item.get("filename").and_then(Value::as_str).unwrap_or("")
                    )),
                    operation_id: None,
                    state: state.into(),
                    retry_safe: false,
                });
            }
        }
        for (i, item) in snapshot.managed_outcomes.iter().enumerate() {
            let state = match item.state.as_str() {
                "succeeded" => "succeeded",
                "uncertain" => "uncertain",
                _ => "failed",
            };
            items.push(crate::sync_receipts::ReceiptItem {
                item_id: crate::sync_receipts::opaque_id(&format!(
                    "{id}:operation:{i}:{}",
                    item.item_id
                )),
                operation_id: item
                    .operation_id
                    .as_deref()
                    .map(crate::sync_receipts::opaque_id),
                state: state.into(),
                retry_safe: item.retry_safe,
            });
        }
        let count = |state: &str| {
            items
                .iter()
                .filter(|v| v.state == state)
                .count()
                .min(u32::MAX as usize) as u32
        };
        let counts = crate::sync_receipts::Counts {
            matched: count("matched"),
            unmatched: count("unmatched"),
            error: count("error"),
            succeeded: count("succeeded"),
            failed: count("failed"),
            uncertain: count("uncertain"),
        };
        let items_truncated = items.len() > 1000;
        if items_truncated {
            items.truncate(1000);
        }
        let mut diagnostics = Vec::new();
        for code in [
            "timeout",
            "cancelled",
            "spawn_failed",
            "output_limit",
            "protocol_error",
            "runner_failed",
            "process_termination_uncertain",
        ] {
            if payload.get("diagnostic_code").and_then(Value::as_str) == Some(code)
                || snapshot
                    .errors
                    .iter()
                    .any(|e| e.get("diagnostic_code").and_then(Value::as_str) == Some(code))
            {
                diagnostics.push(code.to_owned());
            }
        }
        if status == Phase::Cancelled && !diagnostics.iter().any(|v| v == "cancelled") {
            diagnostics.push("cancelled".into());
        }
        if snapshot
            .managed_outcomes
            .iter()
            .any(|item| item.state == "failed")
        {
            diagnostics.push("item_failed".into());
        }
        if snapshot
            .managed_outcomes
            .iter()
            .any(|item| item.state == "uncertain")
        {
            diagnostics.push("item_uncertain".into());
        }
        let receipt_id = crate::sync_receipts::opaque_id(&format!(
            "{id}:{expected:?}:{}:{}",
            snapshot.sequence, operation_kind
        ));
        let receipt = crate::sync_receipts::RunReceipt {
            version: 1,
            receipt_id: receipt_id.clone(),
            run_id: crate::sync_receipts::opaque_id(id),
            operation_kind: operation_kind.into(),
            started_at_ms: started,
            ended_at_ms: ended,
            duration_ms: elapsed,
            terminal_state: terminal_state.into(),
            timed_out: diagnostics.iter().any(|v| v == "timeout"),
            counts,
            items,
            items_truncated,
            diagnostic_codes: diagnostics,
        };
        let store = c.receipt_store.lock().unwrap().clone();
        let error = match store {
            Some(store) => store.write(&receipt).err().map(str::to_owned),
            None => Some("store_unavailable".into()),
        };
        (
            if error.is_none() {
                Some(receipt_id)
            } else {
                None
            },
            error,
            elapsed,
        )
    };
    let mut run = c.current.lock().unwrap();
    if run.run_id.as_deref() != Some(id) || run.status != expected {
        c.finalizing.lock().unwrap().remove(&phase_key);
        return;
    }
    run.status = status.clone();
    run.cancelling = false;
    run.elapsed_ms = Some(elapsed);
    run.receipt_id = receipt_id;
    run.receipt_error = receipt_error;
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
    c.finalizing.lock().unwrap().remove(&phase_key);
    if let Ok(mut requested) = c.cancel_requested.lock() {
        if requested.as_deref() == Some(id) {
            *requested = None;
        }
    }
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
            json!({"message":"Malformed match metadata was rejected", "protocol_error":true}),
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
    let timeout = match crate::sync_process::timeout_seconds(crate::config::get(
        "sync_analysis_timeout_seconds",
    )) {
        Ok(seconds) => std::time::Duration::from_secs(seconds),
        Err(error) => {
            publish(
                c,
                sink,
                id,
                "error",
                json!({"message":error,"diagnostic_code":"invalid_timeout"}),
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
    let mut fatal = false;
    let report = crate::sync_process::run(
        program,
        args,
        timeout,
        || c.is_cancelled(id),
        |is_stderr, line| {
            if is_stderr {
                publish(
                    c,
                    sink,
                    id,
                    "log",
                    json!({"message":line,"level":"warning","source":"stderr"}),
                );
            } else {
                fatal |= parse_line(c, sink, id, line);
            }
        },
    );
    if !report.ended {
        let message = report
            .error
            .unwrap_or_else(|| "Analyzer termination could not be confirmed".into());
        if let Ok(mut run) = c.current.lock() {
            if run.run_id.as_deref() == Some(id) {
                run.cancelling = true;
                run.errors.push(
                    json!({"message":message,"diagnostic_code":"process_termination_uncertain"}),
                );
                run.sequence += 1;
                run.revision += 1;
            }
        }
        return;
    }
    if report.cancelled {
        terminal(
            c,
            sink,
            id,
            Phase::Analyzing,
            Phase::Cancelled,
            json!({"status":"cancelled","analysis_outcome":"failed","diagnostic_code":"cancelled"}),
        );
        return;
    }
    if report.timed_out || report.output_limited || report.error.is_some() {
        let error_detail = report.error.clone();
        let message = report.error.unwrap_or_else(|| {
            if report.timed_out {
                "Analyzer exceeded its configured time limit".into()
            } else {
                "Analyzer output exceeded its configured limit".into()
            }
        });
        let diagnostic_code = if report.timed_out {
            "timeout"
        } else if report.output_limited {
            "output_limit"
        } else if error_detail
            .as_deref()
            .is_some_and(|message| message.contains("invalid UTF-8"))
        {
            "protocol_error"
        } else if error_detail
            .as_deref()
            .is_some_and(|message| message.contains("CreateProcessW"))
        {
            "spawn_failed"
        } else {
            "runner_failed"
        };
        publish(
            c,
            sink,
            id,
            "error",
            json!({"message":message,"diagnostic_code":diagnostic_code,"protocol_error":true}),
        );
        terminal(
            c,
            sink,
            id,
            Phase::Analyzing,
            Phase::Failed,
            json!({"status":"error","analysis_outcome":"failed"}),
        );
        return;
    }
    if report.exit_code != Some(0) || fatal {
        if !fatal {
            publish(
                c,
                sink,
                id,
                "error",
                json!({"message":format!("Harvester exited with code {:?}", report.exit_code)}),
            );
        }
        terminal(
            c,
            sink,
            id,
            Phase::Analyzing,
            Phase::Failed,
            json!({"status":"error","analysis_outcome":"failed"}),
        );
        return;
    }
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
    let outcome = if global_error || (diagnostics && snapshot.eligible_match_ids.is_empty()) {
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
#[cfg(test)]
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
pub fn get_sync_receipts() -> crate::sync_receipts::ReceiptList {
    crate::sync_receipts::list_production()
}

#[tauri::command]
pub fn get_sync_outcomes() -> Vec<crate::managed_import::ItemOutcome> {
    crate::managed_import::outcomes(crate::config::get("ledger_path"))
}

/// Reconcile durable native imports against their journal, archive, source,
/// recovery copy, and the current ledger. The response uses ItemOutcome fields.
#[tauri::command]
pub fn reconcile_ledger_sync() -> Vec<crate::managed_import::ItemOutcome> {
    crate::managed_import::outcomes(crate::config::get("ledger_path"))
}

/// Retry only durable operation IDs returned as retry_safe by reconciliation.
#[tauri::command]
pub fn retry_ledger_sync(app: AppHandle, operation_ids: Vec<String>) -> Result<SyncRun, String> {
    if operation_ids.is_empty() {
        return Err("Select at least one safe managed operation".into());
    }
    if operation_ids
        .iter()
        .any(|id| id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()))
        || operation_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != operation_ids.len()
    {
        return Err("Retry selection contains an invalid operation ID".into());
    }
    let c = coordinator();
    c.configure_receipts();
    let run = c.reserve_retry()?;
    let id = run.run_id.clone().unwrap();
    let expected_count = operation_ids.len();
    publish_state(&app, &run);
    tauri::async_runtime::spawn_blocking(move || {
        let outcomes = crate::managed_import::retry_operations_cancellable(operation_ids, || {
            c.is_cancelled(&id)
        });
        let cancelled = c.is_cancelled(&id);
        let confirmed = outcomes.iter().any(|item| item.state == "succeeded");
        let success = outcomes.len() == expected_count
            && !outcomes.is_empty()
            && outcomes.iter().all(|item| item.state == "succeeded");
        {
            let mut state = c.current.lock().unwrap();
            if state.run_id.as_deref() == Some(&id) && state.status == Phase::Committing {
                state.managed_outcomes = outcomes.clone();
            }
        }
        terminal(
            &c,
            &app,
            &id,
            Phase::Committing,
            if cancelled {
                Phase::Cancelled
            } else if success {
                Phase::Complete
            } else {
                Phase::Failed
            },
            json!({"status":if success {"success"} else {"error"},"managed_outcomes":outcomes,"operation_kind":"managed_retry"}),
        );
        if confirmed {
            let _ = Emitter::emit(
                &app,
                "sync:resources_changed",
                json!({"run_id":id,"run_revision":c.snapshot().revision,"confirmed_effect_ids":outcomes.iter().filter(|item| item.state == "succeeded").filter_map(|item| item.operation_id.clone()).collect::<Vec<_>>()}),
            );
        }
    });
    Ok(run)
}

#[tauri::command]
pub fn start_ledger_sync(app: AppHandle) -> Result<SyncRun, String> {
    let c = coordinator();
    c.configure_receipts();
    crate::sync_process::timeout_seconds(crate::config::get("sync_analysis_timeout_seconds"))?;
    let cfg = crate::managed_import::runtime_config()?;
    let run = c.reserve_analysis(cfg)?;
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
    c.configure_receipts();
    let (run, plans, cfg) = c.begin_managed_commit(&run_id, &payload)?;
    let id = run_id;
    publish_state(&app, &run);
    tauri::async_runtime::spawn_blocking(move || {
        let outcomes = crate::managed_import::commit_prepared_cancellable(plans, &id, cfg, || {
            c.is_cancelled(&id)
        });
        let cancelled = c.is_cancelled(&id);
        let confirmed = outcomes.iter().any(|item| item.state == "succeeded");
        let success = outcomes.len() == payload.len()
            && !outcomes.is_empty()
            && outcomes.iter().all(|o| o.state == "succeeded");
        {
            let mut state = c.current.lock().unwrap();
            if state.run_id.as_deref() == Some(&id) && state.status == Phase::Committing {
                state.managed_outcomes = outcomes.clone();
            }
        }
        terminal(
            &c,
            &app,
            &id,
            Phase::Committing,
            if cancelled {
                Phase::Cancelled
            } else if success {
                Phase::Complete
            } else {
                Phase::Failed
            },
            json!({"status":if success {"success"} else {"error"},"managed_outcomes":outcomes}),
        );
        if confirmed {
            let _ = Emitter::emit(
                &app,
                "sync:resources_changed",
                json!({"run_id":id,"run_revision":c.snapshot().revision,"confirmed_effect_ids":outcomes.iter().filter(|item| item.state == "succeeded").filter_map(|item| item.operation_id.clone()).collect::<Vec<_>>()}),
            );
        }
    });
    Ok(run)
}

#[tauri::command]
pub fn cancel_sync_run(app: AppHandle, run_id: String) -> Result<SyncRun, String> {
    let c = coordinator();
    c.configure_receipts();
    if c.snapshot().run_id.as_deref() == Some(run_id.as_str())
        && c.snapshot().status == Phase::Review
    {
        terminal(
            &c,
            &app,
            &run_id,
            Phase::Review,
            Phase::Cancelled,
            json!({"status":"cancelled","diagnostic_code":"cancelled"}),
        );
        return Ok(c.snapshot());
    }
    let run = c.request_cancel(&run_id)?;
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
    #[test]
    fn reveal_opener_receives_only_canonical_path_without_launching_explorer() {
        let temp = TempDir::new();
        let inbox = temp.0.join("inbox");
        fs::create_dir_all(&inbox).unwrap();
        let file = inbox.join("statement.pdf");
        fs::write(&file, b"synthetic").unwrap();
        let canonical = fs::canonicalize(&file).unwrap();
        let observed = Mutex::new(None);
        reveal_path_with(canonical.clone(), |path| {
            *observed.lock().unwrap() = Some(path);
            Ok(())
        })
        .unwrap();
        assert_eq!(observed.into_inner().unwrap(), Some(canonical));
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

    fn test_import_config(
        root: &std::path::Path,
        label: &str,
    ) -> (
        crate::managed_import::ImportConfig,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        let inbox = root.join(format!("inbox-{label}"));
        let archive = root.join(format!("archive-{label}"));
        let store = root.join(format!("store-{label}"));
        let ledger = root.join(format!("ledger-{label}.beancount"));
        fs::create_dir_all(&inbox).unwrap();
        fs::create_dir_all(&archive).unwrap();
        fs::create_dir_all(&store).unwrap();
        fs::write(&ledger, "; test ledger\n").unwrap();
        let cfg = crate::managed_import::ImportConfig::injected(
            fs::canonicalize(inbox.clone()).unwrap(),
            fs::canonicalize(archive).unwrap(),
            fs::canonicalize(ledger.clone()).unwrap(),
            fs::canonicalize(store).unwrap(),
            "USD".into(),
            "Assets:Bank:Checking".into(),
        );
        (cfg, inbox, ledger)
    }

    #[test]
    fn configured_analysis_reservation_is_atomic_across_active_phases() {
        let temp = TempDir::new();
        let (first_cfg, first_inbox, first_ledger) = test_import_config(&temp.0, "first");
        let (other_cfg, _, _) = test_import_config(&temp.0, "other");
        fs::write(first_inbox.join("statement.pdf"), b"statement").unwrap();
        let c = Coordinator::default();
        let first = c.reserve_analysis(first_cfg.clone()).unwrap();
        let id = first.run_id.clone().unwrap();
        let match_data = json!({"filename":"statement.pdf","new_name":"Institution 2026-10-23.pdf","date":"2026-10-23","rule":"Groceries","category":"Expenses:Food","amount":"12.34","was_encrypted":false,"is_cleared":true});
        let (stored, _) = c.event(&id, "match", match_data).unwrap();
        let before = stored.clone();
        let plan_keys = c.plans.lock().unwrap().keys().cloned().collect::<Vec<_>>();
        assert_eq!(plan_keys.len(), 1);
        assert!(c.reserve_analysis(other_cfg.clone()).is_err());
        assert_eq!(c.snapshot().revision, before.revision);
        assert_eq!(c.snapshot().status, Phase::Analyzing);
        assert_eq!(
            c.approved_config().unwrap().test_ledger().to_path_buf(),
            fs::canonicalize(first_ledger.clone()).unwrap()
        );
        assert_eq!(
            c.plans.lock().unwrap().keys().cloned().collect::<Vec<_>>(),
            plan_keys
        );
        c.transition(&id, Phase::Review).unwrap();
        assert!(c.reserve_analysis(other_cfg.clone()).is_err());
        assert_eq!(
            c.approved_config().unwrap().test_ledger().to_path_buf(),
            fs::canonicalize(first_ledger.clone()).unwrap()
        );
        {
            c.current.lock().unwrap().status = Phase::Committing;
        }
        assert!(c.reserve_analysis(other_cfg).is_err());
        assert_eq!(
            c.approved_config().unwrap().test_ledger().to_path_buf(),
            fs::canonicalize(first_ledger.clone()).unwrap()
        );
        assert_eq!(c.plans.lock().unwrap().len(), 1);
    }

    #[test]
    fn retry_reservation_uses_run_ownership_and_rejects_review_or_active_runs() {
        let c = Coordinator::default();
        let retry = c.reserve_retry().unwrap();
        assert_eq!(retry.status, Phase::Committing);
        assert_eq!(retry.operation_kind.as_deref(), Some("managed_retry"));
        assert!(c.reserve_retry().is_err());
        c.transition(retry.run_id.as_deref().unwrap(), Phase::Failed)
            .unwrap();
        let analysis = c.reserve(Phase::Analyzing).unwrap();
        c.transition(analysis.run_id.as_deref().unwrap(), Phase::Review)
            .unwrap();
        assert!(c.reserve_retry().is_err());
        assert_eq!(c.snapshot().status, Phase::Review);
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
            vec![
                json!({"filename":"synthetic.pdf","new_name":"renamed.pdf","_managed_item_id":backend_id})
            ]
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
    fn analyzer_cancel_is_run_bound_and_keeps_reservation_until_process_ends() {
        let temp = TempDir::new();
        let script = temp.script("import time\ntime.sleep(30)\n");
        let coordinator = Arc::new(Coordinator::default());
        let id = start(&coordinator).run_id.unwrap();
        assert!(coordinator.request_cancel("stale-run-id").is_err());
        let requested = coordinator.request_cancel(&id).unwrap();
        assert_eq!(requested.status, Phase::Analyzing);
        assert!(requested.cancelling);
        assert!(coordinator.reserve(Phase::Analyzing).is_err());

        analyze_worker(
            coordinator.clone(),
            &FakeSink::default(),
            &id,
            &python(),
            &[script],
        );
        let final_state = coordinator.snapshot();
        assert_eq!(final_state.status, Phase::Cancelled);
        assert_eq!(final_state.analysis_outcome, Some(AnalysisOutcome::Failed));
        assert!(!final_state.cancelling);
        assert!(coordinator.reserve(Phase::Analyzing).is_ok());
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

    #[cfg(windows)]
    #[test]
    fn fake_analyzer_captures_private_prepared_plan_and_source_change_is_rejected() {
        let temp = TempDir::new();
        let inbox = temp.0.join("inbox");
        let archive = temp.0.join("archive");
        let store = temp.0.join("private");
        fs::create_dir_all(&inbox).unwrap();
        fs::create_dir_all(&archive).unwrap();
        fs::create_dir_all(&store).unwrap();
        fs::write(inbox.join("synthetic.pdf"), b"synthetic statement").unwrap();
        let ledger = temp.0.join("synthetic.beancount");
        fs::write(&ledger, "; synthetic ledger\n").unwrap();
        let cfg = crate::managed_import::ImportConfig::injected(
            fs::canonicalize(inbox.clone()).unwrap(),
            fs::canonicalize(archive.clone()).unwrap(),
            fs::canonicalize(ledger.clone()).unwrap(),
            fs::canonicalize(store).unwrap(),
            "USD".into(),
            "Assets:Bank:Checking".into(),
        );
        let script = temp.script("import json\nprint(json.dumps({'type':'match','filename':'synthetic.pdf','new_name':'Bank 2026-10-23.pdf','date':'2026-10-23','rule':'Groceries','category':'Expenses:Food','amount':'12.34','was_encrypted':False,'is_cleared':True}))\n");
        let coordinator = Arc::new(Coordinator::default());
        coordinator.configure_import(cfg.clone());
        let id = start(&coordinator).run_id.unwrap();
        analyze_worker(
            coordinator.clone(),
            &FakeSink::default(),
            &id,
            &python(),
            &[script],
        );
        let review = coordinator.snapshot();
        assert_eq!(review.status, Phase::Review);
        assert_eq!(review.eligible_match_ids.len(), 1);
        let match_id = review.eligible_match_ids[0].clone();
        fs::write(inbox.join("synthetic.pdf"), b"substituted after review").unwrap();
        let (_, plans, cfg) = coordinator.begin_managed_commit(&id, &[match_id]).unwrap();
        let before = fs::read(&ledger).unwrap();
        let outcomes = crate::managed_import::commit_prepared(plans, &id, cfg);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].state, "failed");
        assert_eq!(fs::read(&ledger).unwrap(), before);
        assert!(!archive.join("Statements 2026/Bank 2026-10-23.pdf").exists());
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

    struct FailedReceiptStore;
    impl crate::sync_receipts::ReceiptStore for FailedReceiptStore {
        fn write(&self, _: &crate::sync_receipts::RunReceipt) -> Result<(), &'static str> {
            Err("publication_failed")
        }
    }

    #[test]
    fn terminal_finalizer_publishes_distinct_analysis_and_commit_receipts_first() {
        let dir = TempDir::new();
        let c = Arc::new(Coordinator::default());
        *c.receipt_store.lock().unwrap() =
            Some(Arc::new(crate::sync_receipts::FileStore::at(dir.0.clone())));
        let run = start(&c);
        let id = run.run_id.clone().unwrap();
        let (item_id, _) = seed_review(&c, &id, json!({}));
        c.current.lock().unwrap().status = Phase::Analyzing;
        let sink = FakeSink::default();
        terminal(
            &c,
            &sink,
            &id,
            Phase::Analyzing,
            Phase::Review,
            json!({"status":"success","analysis_outcome":"success"}),
        );
        let analysis = c.snapshot();
        assert!(
            analysis.receipt_id.is_some(),
            "receipt error {:?}",
            analysis.receipt_error
        );
        let analysis_receipt = crate::sync_receipts::list_at(&dir.0).receipts.remove(0);
        assert_eq!(analysis_receipt.operation_kind, "analysis");
        assert_eq!(analysis_receipt.terminal_state, "review");
        assert_eq!(analysis_receipt.counts.matched, 1);
        let (_, selected) = c.begin_commit(&id, &[item_id]).unwrap();
        assert_eq!(selected.len(), 1);
        terminal(
            &c,
            &sink,
            &id,
            Phase::Committing,
            Phase::Complete,
            json!({"status":"success"}),
        );
        let final_run = c.snapshot();
        assert!(final_run.receipt_error.is_none());
        assert_ne!(
            final_run.receipt_id.as_deref(),
            Some(analysis_receipt.receipt_id.as_str())
        );
        let listing = crate::sync_receipts::list_at(&dir.0);
        assert_eq!(listing.receipts.len(), 2);
        assert!(listing
            .receipts
            .iter()
            .any(|r| r.operation_kind == "commit" && r.terminal_state == "complete"));
        let done = sink
            .0
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(event, _)| event == "sync:done")
            .unwrap()
            .1
            .clone();
        assert_eq!(done["snapshot"]["receipt_id"], json!(final_run.receipt_id));
        assert!(done["snapshot"]["receipt_error"].is_null());
    }

    #[test]
    fn terminal_finalizer_surfaces_fixed_store_error_without_losing_terminal_result() {
        let c = Coordinator::default();
        *c.receipt_store.lock().unwrap() = Some(Arc::new(FailedReceiptStore));
        let run = start(&c);
        let id = run.run_id.unwrap();
        let sink = FakeSink::default();
        terminal(
            &c,
            &sink,
            &id,
            Phase::Analyzing,
            Phase::Failed,
            json!({"status":"error","diagnostic_code":"spawn_failed"}),
        );
        let current = c.snapshot();
        assert_eq!(current.status, Phase::Failed);
        assert_eq!(current.receipt_error.as_deref(), Some("publication_failed"));
        assert!(current.receipt_id.is_none());
        let done = sink.0.lock().unwrap().last().unwrap().1.clone();
        assert_eq!(done["snapshot"]["receipt_error"], "publication_failed");
    }

    #[test]
    fn abandoning_review_writes_a_second_cancelled_receipt() {
        let dir = TempDir::new();
        let c = Coordinator::default();
        *c.receipt_store.lock().unwrap() =
            Some(Arc::new(crate::sync_receipts::FileStore::at(dir.0.clone())));
        let run = start(&c);
        let id = run.run_id.unwrap();
        seed_review(&c, &id, json!({}));
        c.current.lock().unwrap().status = Phase::Analyzing;
        let sink = FakeSink::default();
        terminal(
            &c,
            &sink,
            &id,
            Phase::Analyzing,
            Phase::Review,
            json!({"status":"success","analysis_outcome":"success"}),
        );
        let first = c.snapshot().receipt_id.clone();
        terminal(
            &c,
            &sink,
            &id,
            Phase::Review,
            Phase::Cancelled,
            json!({"status":"cancelled"}),
        );
        let listing = crate::sync_receipts::list_at(&dir.0);
        assert_eq!(listing.receipts.len(), 2);
        assert!(listing
            .receipts
            .iter()
            .any(|r| r.terminal_state == "cancelled" && r.operation_kind == "analysis"));
        assert!(c.snapshot().receipt_id.is_some());
        assert_ne!(first, c.snapshot().receipt_id);
    }
}
