use std::fs;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::model::{Evidence, EvidenceKind, PetState, Status};
use crate::state_machine::advance;

const STALE_AFTER_MS: u64 = 5 * 60 * 1_000;
// Codex subagents often emit `task_started` once and then work silently while
// commands or tests run.  Keep such tasks visible for a bounded longer window.
const TASK_STALE_AFTER_MS: u64 = 2 * 60 * 60 * 1_000;
// A parent project can have more than a dozen concurrent subagent sessions.
// Keep enough recent files to retain the parent's own task events as well.
const MAX_SESSION_FILES: usize = 48;
// Large active sessions can place their `task_started` event far before the
// tail window. Recover task events from the most recently written sessions.
const RECENT_TASK_EVENT_SCAN_FILES: usize = 8;
// The original user request normally appears near the beginning of a session.
// Keep it so later follow-up questions cannot become a second project name.
const PREFIX_BYTES: u64 = 128 * 1_024;
const TAIL_BYTES: u64 = 512 * 1_024;

#[derive(Debug, Clone)]
pub struct CodexPaths {
    pub home: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageSnapshot {
    pub today_tokens: Option<u64>,
    pub weekly_usage_percent: Option<f64>,
    pub weekly_reset_at_ms: Option<u64>,
    pub synced_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    Ready,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Working,
    Completed,
    Waiting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSnapshot {
    pub turn_id: String,
    pub title: String,
    pub status: TaskStatus,
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PetSnapshot {
    pub state: PetState,
    pub usage: UsageSnapshot,
    pub sync_state: SyncState,
    pub tasks: Vec<TaskSnapshot>,
}

pub struct CodexDataAdapter {
    paths: CodexPaths,
    state: PetState,
}

impl CodexDataAdapter {
    pub fn new(paths: CodexPaths) -> Self {
        Self {
            paths,
            state: resting(),
        }
    }

    pub fn poll(&mut self, now_ms: u64) -> PetSnapshot {
        let Ok(scan) = scan_sources(&self.paths.home) else {
            self.state = resting();
            return unavailable_snapshot();
        };

        let has_task_records = !scan.tasks.is_empty();
        let tasks = task_snapshots(&scan.tasks, scan.no_turn_waiting_at_ms, now_ms);
        let synced_at_ms = scan
            .usage
            .as_ref()
            .map(|usage| usage.synced_at_ms)
            .into_iter()
            .chain(tasks.iter().map(|task| task.observed_at_ms))
            .max();
        let Some(synced_at_ms) = synced_at_ms else {
            self.state = resting();
            return unavailable_snapshot();
        };

        if now_ms.saturating_sub(synced_at_ms) > STALE_AFTER_MS {
            self.state = resting();
            return PetSnapshot {
                state: self.state,
                usage: scan.usage.map(ValidatedUsage::into_snapshot).unwrap_or_else(empty_usage),
                sync_state: SyncState::Stale,
                tasks,
            };
        }

        self.state = if has_task_records {
            aggregate_task_state(&tasks, now_ms)
        } else {
            advance(self.state, scan.evidence, now_ms)
        };
        PetSnapshot {
            state: self.state,
            usage: scan.usage.map(ValidatedUsage::into_snapshot).unwrap_or_else(empty_usage),
            sync_state: SyncState::Ready,
            tasks,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ValidatedUsage {
    today_tokens: Option<u64>,
    weekly_usage_percent: f64,
    weekly_reset_at_ms: Option<u64>,
    synced_at_ms: u64,
}

impl ValidatedUsage {
    fn into_snapshot(self) -> UsageSnapshot {
        UsageSnapshot {
            today_tokens: self.today_tokens,
            weekly_usage_percent: Some(self.weekly_usage_percent),
            weekly_reset_at_ms: self.weekly_reset_at_ms,
            synced_at_ms: Some(self.synced_at_ms),
        }
    }
}

struct Scan {
    evidence: Option<Evidence>,
    usage: Option<ValidatedUsage>,
    latest_observed_at_ms: Option<u64>,
    no_turn_waiting_at_ms: Option<u64>,
    tasks: BTreeMap<String, TaskSnapshot>,
}

#[derive(Debug, Default)]
struct TitleCatalog {
    titles: BTreeMap<String, String>,
    parents: BTreeMap<String, String>,
}

impl TitleCatalog {
    fn project_title(&self, session_id: &str) -> Option<String> {
        let mut owner = session_id;
        for _ in 0..64 {
            let Some(parent) = self.parents.get(owner) else { break; };
            if parent == owner { break; }
            owner = parent;
        }
        self.titles.get(owner).cloned()
    }
}

fn scan_sources(home: &Path) -> Result<Scan, ()> {
    let mut paths = Vec::new();
    let index = home.join("session_index.jsonl");
    if index.is_file() {
        paths.push(index);
    }

    let sessions = home.join("sessions");
    if sessions.is_dir() {
        let mut session_paths = Vec::new();
        collect_jsonl_files(&sessions, &mut session_paths)?;
        session_paths.sort_by_key(|path| {
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        });
        session_paths.reverse();
        session_paths.truncate(MAX_SESSION_FILES);
        let recent_task_event_paths: BTreeSet<_> = session_paths
            .iter()
            .take(RECENT_TASK_EVENT_SCAN_FILES)
            .cloned()
            .collect();
        paths.extend(session_paths);
        paths.sort();

        let catalog = load_title_catalog(home, &paths);
        let mut scan = Scan {
            evidence: None,
            usage: None,
            latest_observed_at_ms: None,
            no_turn_waiting_at_ms: None,
            tasks: BTreeMap::new(),
        };
        let mut scanned_any = false;
        for path in paths {
            let result = if recent_task_event_paths.contains(&path) {
                scan_jsonl_with_task_recovery(&path, &mut scan, &catalog)
            } else {
                scan_jsonl(&path, &mut scan, &catalog)
            };
            if result.is_ok() {
                scanned_any = true;
            }
        }
        return scanned_any.then_some(scan).ok_or(());
    }
    if paths.is_empty() {
        return Err(());
    }
    let catalog = load_title_catalog(home, &paths);
    paths.sort();

    let mut scan = Scan {
        evidence: None,
        usage: None,
        latest_observed_at_ms: None,
        no_turn_waiting_at_ms: None,
        tasks: BTreeMap::new(),
    };
    let mut scanned_any = false;
    for path in paths {
        if scan_jsonl(&path, &mut scan, &catalog).is_ok() {
            scanned_any = true;
        }
    }
    scanned_any.then_some(scan).ok_or(())
}

fn collect_jsonl_files(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), ()> {
    for entry in fs::read_dir(directory).map_err(|_| ())? {
        let path = entry.map_err(|_| ())?.path();
        if path.is_dir() {
            collect_jsonl_files(&path, paths)?;
        } else if path.extension().is_some_and(|extension| extension == "jsonl") {
            paths.push(path);
        }
    }
    Ok(())
}

fn scan_jsonl(path: &Path, scan: &mut Scan, catalog: &TitleCatalog) -> Result<(), ()> {
    scan_jsonl_source(read_jsonl_window(path)?, scan, catalog)
}

fn scan_jsonl_with_task_recovery(path: &Path, scan: &mut Scan, catalog: &TitleCatalog) -> Result<(), ()> {
    scan_jsonl_source(read_jsonl_with_task_recovery(path)?, scan, catalog)
}

fn scan_jsonl_source(source: String, scan: &mut Scan, catalog: &TitleCatalog) -> Result<(), ()> {
    // A filename is only a storage detail, never a project name.  It must not
    // replace the title carried by task events when session metadata is absent.
    let mut source_title = None;
    let mut has_user_title = false;
    let mut source_id = None;
    for line in source.lines().filter(|line| !line.trim().is_empty()) {
        let record = JsonParser::parse(line)?;
        let JsonValue::Object(_) = record else {
            return Err(());
        };

        if string(field(&record, "type")) == Some("session_meta") {
            let payload = field(&record, "payload");
            let session_id = string(field(payload.unwrap_or(&JsonValue::Null), "session_id"))
                .or_else(|| string(field(payload.unwrap_or(&JsonValue::Null), "id")))
                .map(str::to_owned);
            source_title = session_id.as_deref().and_then(|id| catalog.project_title(id));
            has_user_title = source_title.is_some();
            source_id = session_id;
            continue;
        }

        if !has_user_title {
            if let Some(title) = user_task_title(&record) {
                source_title = Some(title);
                has_user_title = true;
            }
        }

        if string(field(&record, "type")) == Some("event_msg") {
            scan_desktop_event(
                field(&record, "payload"),
                string(field(&record, "timestamp")).and_then(rfc3339_ms),
                source_title.as_deref(),
                source_id.as_deref(),
                scan,
            )?;
            continue;
        }

        if let Some(usage) = field(&record, "usage") {
            let usage = parse_usage(usage)?;
            if scan
                .usage
                .is_none_or(|previous| usage.synced_at_ms >= previous.synced_at_ms)
            {
                scan.usage = Some(usage);
            }
        }

        let Some(event_type) = field(&record, "event_type") else {
            continue;
        };
        let JsonValue::String(event_type) = event_type else {
            return Err(());
        };
        let kind = match event_type.as_str() {
            "execution_started" => EvidenceKind::ExecutionStarted,
            "execution_heartbeat" => EvidenceKind::ExecutionHeartbeat,
            "execution_completed" => EvidenceKind::ExecutionCompleted,
            "waiting_for_input" => EvidenceKind::WaitingForInput,
            _ => return Err(()),
        };
        let observed_at_ms = integer(field(&record, "observed_at_ms").ok_or(())?)?;
        let evidence = Evidence {
            kind,
            observed_at_ms,
        };
        if evidence.kind == EvidenceKind::WaitingForInput {
            scan.no_turn_waiting_at_ms = Some(
                scan.no_turn_waiting_at_ms
                    .unwrap_or(0)
                    .max(evidence.observed_at_ms),
            );
        }
        if scan
            .evidence
            .is_none_or(|previous| evidence.observed_at_ms >= previous.observed_at_ms)
        {
            scan.evidence = Some(evidence);
        }
    }
    Ok(())
}

fn load_title_catalog(home: &Path, paths: &[PathBuf]) -> TitleCatalog {
    let mut catalog = TitleCatalog::default();
    if let Ok(source) = fs::read_to_string(home.join(".codex-global-state.json")) {
        if let Ok(value) = JsonParser::parse(&source) {
            if let Some(JsonValue::Object(entries)) = field(field(&value, "electron-persisted-atom-state").unwrap_or(&JsonValue::Null), "thread-descriptions-v1") {
                for (id, title) in entries {
                    if let JsonValue::String(title) = title {
                        if !title.trim().is_empty() { catalog.titles.insert(id.clone(), title.clone()); }
                    }
                }
            }
        }
    }

    if let Ok(source) = fs::read_to_string(home.join("session_index.jsonl")) {
        for line in source.lines().filter(|line| !line.trim().is_empty()) {
            let Ok(record) = JsonParser::parse(line) else { continue; };
            let Some(id) = string(field(&record, "id")) else { continue; };
            let Some(title) = string(field(&record, "thread_name")) else { continue; };
            if !title.trim().is_empty() { catalog.titles.insert(id.to_owned(), title.to_owned()); }
        }
    }

    for path in paths {
        let Ok(file) = fs::File::open(path) else { continue; };
        let mut line = String::new();
        if BufReader::new(file).read_line(&mut line).is_err() { continue; }
        let Ok(record) = JsonParser::parse(&line) else { continue; };
        if string(field(&record, "type")) != Some("session_meta") { continue; }
        let payload = field(&record, "payload").unwrap_or(&JsonValue::Null);
        let Some(id) = string(field(payload, "session_id")).or_else(|| string(field(payload, "id"))) else { continue; };
        if let Some(parent) = string(field(payload, "parent_thread_id")) {
            if !parent.trim().is_empty() { catalog.parents.insert(id.to_owned(), parent.to_owned()); }
        }
    }
    catalog
}

fn read_jsonl_window(path: &Path) -> Result<String, ()> {
    let length = fs::metadata(path).map_err(|_| ())?.len();
    if length <= PREFIX_BYTES.saturating_add(TAIL_BYTES) {
        return fs::read_to_string(path).map_err(|_| ());
    }

    let mut file = fs::File::open(path).map_err(|_| ())?;
    let mut prefix_bytes = vec![0; PREFIX_BYTES as usize];
    let prefix_length = file.read(&mut prefix_bytes).map_err(|_| ())?;
    let prefix = String::from_utf8_lossy(&prefix_bytes[..prefix_length]);
    let prefix = prefix.rsplit_once('\n')
        .map(|(complete_lines, _)| format!("{complete_lines}\n"))
        .unwrap_or_default();
    file.seek(SeekFrom::End(-(TAIL_BYTES as i64))).map_err(|_| ())?;
    let mut tail = String::new();
    file.read_to_string(&mut tail).map_err(|_| ())?;
    let tail = tail.split_once('\n').map(|(_, complete_lines)| complete_lines).unwrap_or("");
    Ok(format!("{prefix}{tail}"))
}

fn read_jsonl_with_task_recovery(path: &Path) -> Result<String, ()> {
    let mut source = read_jsonl_window(path)?;
    if fs::metadata(path).map_err(|_| ())?.len() <= PREFIX_BYTES.saturating_add(TAIL_BYTES) {
        return Ok(source);
    }

    let file = fs::File::open(path).map_err(|_| ())?;
    for line in BufReader::new(file).split(b'\n') {
        let line = line.map_err(|_| ())?;
        let Ok(line) = String::from_utf8(line) else { continue; };
        if is_task_event_line(&line) {
            source.push_str(&line);
            source.push('\n');
        }
    }
    Ok(source)
}

fn is_task_event_line(line: &str) -> bool {
    line.contains("\"type\":\"event_msg\"")
        && [
            "\"type\":\"task_started\"",
            "\"type\":\"task_complete\"",
            "\"type\":\"task_waiting\"",
            "\"type\":\"task_waiting_for_input\"",
            "\"type\":\"waiting_for_input\"",
            "\"type\":\"sub_agent_activity\"",
        ]
        .iter()
        .any(|event| line.contains(event))
}

fn scan_desktop_event(
    payload: Option<&JsonValue>,
    record_timestamp_ms: Option<u64>,
    source_title: Option<&str>,
    source_id: Option<&str>,
    scan: &mut Scan,
) -> Result<(), ()> {
    let Some(payload) = payload else { return Ok(()); };
    let Some(kind) = string(field(payload, "type")) else { return Ok(()); };
    match kind {
        "task_started" => {
            let observed_at_ms = integer(field(payload, "started_at").ok_or(() )?)?.saturating_mul(1_000);
            update_desktop_evidence(scan, Evidence { kind: EvidenceKind::ExecutionStarted, observed_at_ms });
            update_task(scan, payload, source_title, TaskStatus::Working, observed_at_ms);
        }
        "task_complete" => {
            let observed_at_ms = integer(field(payload, "completed_at").ok_or(() )?)?.saturating_mul(1_000);
            update_desktop_evidence(scan, Evidence { kind: EvidenceKind::ExecutionCompleted, observed_at_ms });
            update_task(scan, payload, source_title, TaskStatus::Completed, observed_at_ms);
        }
        "task_waiting" | "task_waiting_for_input" | "waiting_for_input" => {
            let Some(observed_at_ms) = record_timestamp_ms else { return Ok(()); };
            update_desktop_evidence(scan, Evidence { kind: EvidenceKind::WaitingForInput, observed_at_ms });
            update_task(scan, payload, source_title, TaskStatus::Waiting, observed_at_ms);
        }
        "token_count" => {
            let parsed = parse_desktop_usage(payload, record_timestamp_ms);
            if let Some(usage) = parsed.filter(|usage| scan.usage.is_none_or(|previous| usage.synced_at_ms >= previous.synced_at_ms)) {
                scan.usage = Some(usage);
            }
        }
        "sub_agent_activity" => {
            let Some(observed_at_ms) = record_timestamp_ms else { return Ok(()); };
            update_desktop_evidence(scan, Evidence { kind: EvidenceKind::ExecutionHeartbeat, observed_at_ms });
            update_source_task(scan, source_id, source_title, observed_at_ms);
        }
        _ => {}
    }
    Ok(())
}

fn update_source_task(scan: &mut Scan, source_id: Option<&str>, source_title: Option<&str>, observed_at_ms: u64) {
    let Some(source_id) = source_id else { return; };
    let task = TaskSnapshot {
        turn_id: format!("session:{source_id}"),
        title: source_title.unwrap_or("Codex 会话").to_owned(),
        status: TaskStatus::Working,
        observed_at_ms,
    };
    let entry = scan.tasks.entry(task.turn_id.clone()).or_insert(task.clone());
    if task.observed_at_ms >= entry.observed_at_ms { *entry = task; }
}

fn parse_desktop_usage(
    payload: &JsonValue,
    synced_at_ms: Option<u64>,
) -> Option<ValidatedUsage> {
    let weekly_usage_percent = number(field(field(field(payload, "rate_limits")?, "primary")?, "used_percent")?).ok()?;
    if !(0.0..=100.0).contains(&weekly_usage_percent) {
        return None;
    }

    let today_tokens = field(payload, "info")
        .and_then(|info| field(info, "last_token_usage"))
        .and_then(|usage| field(usage, "total_tokens"))
        .and_then(|tokens| integer(tokens).ok());
    let weekly_reset_at_ms = field(field(field(payload, "rate_limits")?, "primary")?, "resets_at")
        .and_then(|reset_at| integer(reset_at).ok())
        .map(|reset_at| reset_at.saturating_mul(1_000));
    Some(ValidatedUsage {
        today_tokens,
        weekly_usage_percent,
        weekly_reset_at_ms,
        synced_at_ms: synced_at_ms?,
    })
}

fn update_task(
    scan: &mut Scan,
    payload: &JsonValue,
    source_title: Option<&str>,
    status: TaskStatus,
    observed_at_ms: u64,
) {
    let Some(turn_id) = string(field(payload, "turn_id")) else { return; };
    let explicit_title = string(field(payload, "title"))
        .or_else(|| string(field(payload, "task_title")))
        .or_else(|| string(field(payload, "name")));
    let title = source_title
        .or(explicit_title)
        .unwrap_or_else(|| task_fallback_title(turn_id));
    let task = TaskSnapshot {
        turn_id: turn_id.to_owned(),
        title: shorten_title(title),
        status,
        observed_at_ms,
    };
    let entry = scan.tasks.entry(task.turn_id.clone()).or_insert(task.clone());
    if task.observed_at_ms >= entry.observed_at_ms {
        *entry = TaskSnapshot {
            title: source_title
                .or(explicit_title)
                .map(shorten_title)
                .unwrap_or_else(|| entry.title.clone()),
            ..task
        };
    }
}

fn task_snapshots(
    tasks: &BTreeMap<String, TaskSnapshot>,
    no_turn_waiting_at_ms: Option<u64>,
    now_ms: u64,
) -> Vec<TaskSnapshot> {
    let tasks: Vec<_> = tasks
        .values()
        .filter_map(|task| {
            let mut task = task.clone();
            if task.status == TaskStatus::Working
                && no_turn_waiting_at_ms.is_some_and(|waiting_at_ms| waiting_at_ms >= task.observed_at_ms)
            {
                task.status = TaskStatus::Waiting;
                task.observed_at_ms = no_turn_waiting_at_ms.unwrap_or(task.observed_at_ms);
            }
            (now_ms.saturating_sub(task.observed_at_ms) <= TASK_STALE_AFTER_MS).then_some(task)
        })
        .collect();
    // The island presents projects, not individual parent/subagent sessions.
    // A child task and the parent's activity heartbeat therefore become one row.
    let mut projects: BTreeMap<String, TaskSnapshot> = BTreeMap::new();
    for task in tasks {
        let entry = projects.entry(task.title.clone()).or_insert(task.clone());
        if task_status_rank(task.status) > task_status_rank(entry.status)
            || (task_status_rank(task.status) == task_status_rank(entry.status)
                && task.observed_at_ms > entry.observed_at_ms)
        {
            *entry = task;
        }
    }
    let mut tasks: Vec<_> = projects.into_values().collect();
    tasks.sort_by(|left, right| {
        right
            .observed_at_ms
            .cmp(&left.observed_at_ms)
            .then_with(|| left.turn_id.cmp(&right.turn_id))
    });
    tasks
}

fn task_status_rank(status: TaskStatus) -> u8 {
    match status {
        TaskStatus::Working => 3,
        TaskStatus::Completed => 2,
        TaskStatus::Waiting => 1,
    }
}

fn user_task_title(record: &JsonValue) -> Option<String> {
    if string(field(record, "type")) != Some("response_item") {
        return None;
    }
    let payload = field(record, "payload")?;
    if string(field(payload, "type")) != Some("message") || string(field(payload, "role")) != Some("user") {
        return None;
    }
    let JsonValue::Array(content) = field(payload, "content")? else {
        return None;
    };
    content.iter().find_map(|item| {
        let text = string(field(item, "text"))?;
        let first_line = text.lines().map(str::trim).find(|line| is_user_task_line(line))?;
        Some(shorten_title(first_line))
    })
}

fn is_user_task_line(line: &&str) -> bool {
    let line = line.trim();
    !line.is_empty()
        && !line.starts_with('<')
        && !line.starts_with('#')
        && !line.starts_with('-')
        && !line.starts_with("Here is a list of plugins")
        && !matches!(line, "filesystem" | "environment_context" | "permissions instructions")
}

fn shorten_title(title: &str) -> String {
    let compact = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let lowercase = compact.to_ascii_lowercase();
    if compact.contains("Codex") && compact.contains("桌宠") {
        return "Codex桌宠灵动岛".to_owned();
    }
    if lowercase.contains("translate") || compact.contains("翻译") {
        return "翻译大师".to_owned();
    }
    if let Some((_, quoted)) = compact.split_once('“') {
        if let Some((project, _)) = quoted.split_once('”') {
            if !project.trim().is_empty() {
                return project.trim().to_owned();
            }
        }
    }
    let mut shortened: String = compact.chars().take(22).collect();
    if compact.chars().count() > 22 { shortened.push('…'); }
    shortened
}

fn task_fallback_title(_: &str) -> &str {
    "Codex 任务"
}

fn aggregate_task_state(tasks: &[TaskSnapshot], now_ms: u64) -> PetState {
    if tasks.iter().any(|task| task.status == TaskStatus::Working) {
        return working();
    }

    tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Completed)
        .filter_map(|task| task.observed_at_ms.checked_add(8_000))
        .filter(|&completed_until_ms| now_ms < completed_until_ms)
        .max()
        .map(completed)
        .unwrap_or_else(resting)
}

fn rfc3339_ms(value: &str) -> Option<u64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20 || bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') || bytes.get(10) != Some(&b'T') {
        return None;
    }
    let parse = |start: usize, end: usize| std::str::from_utf8(bytes.get(start..end)?).ok()?.parse::<u64>().ok();
    let year = parse(0, 4)?;
    let month = parse(5, 7)?;
    let day = parse(8, 10)?;
    let hour = parse(11, 13)?;
    let minute = parse(14, 16)?;
    let second = parse(17, 19)?;
    if !(1..=12).contains(&month) || day == 0 || hour > 23 || minute > 59 || second > 59 { return None; }
    let leap = |year: u64| year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_before_year = |year: u64| 365 * year + year / 4 - year / 100 + year / 400;
    let month_days = [31_u64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut days = days_before_year(year).checked_sub(days_before_year(1970))?;
    for index in 0..(month - 1) as usize { days += month_days[index] + u64::from(index == 1 && leap(year)); }
    if day > month_days[(month - 1) as usize] + u64::from(month == 2 && leap(year)) { return None; }
    let millis = if bytes.get(19) == Some(&b'.') { parse(20, 23).unwrap_or(0) } else { 0 };
    days.checked_add(day - 1)?.checked_mul(86_400)?.checked_add(hour * 3_600 + minute * 60 + second)?.checked_mul(1_000)?.checked_add(millis)
}

fn update_desktop_evidence(scan: &mut Scan, evidence: Evidence) {
    scan.latest_observed_at_ms = Some(scan.latest_observed_at_ms.unwrap_or(0).max(evidence.observed_at_ms));
    if scan.evidence.is_none_or(|previous| evidence.observed_at_ms >= previous.observed_at_ms) {
        scan.evidence = Some(evidence);
    }
}

fn parse_usage(value: &JsonValue) -> Result<ValidatedUsage, ()> {
    let JsonValue::Object(_) = value else {
        return Err(());
    };
    let today_tokens = integer(field(value, "today_tokens").ok_or(())?)?;
    let weekly_usage_percent = number(field(value, "weekly_usage_percent").ok_or(())?)?;
    if !(0.0..=100.0).contains(&weekly_usage_percent) {
        return Err(());
    }
    let synced_at_ms = integer(field(value, "synced_at_ms").ok_or(())?)?;
    Ok(ValidatedUsage {
        today_tokens: Some(today_tokens),
        weekly_usage_percent,
        weekly_reset_at_ms: None,
        synced_at_ms,
    })
}

fn unavailable_snapshot() -> PetSnapshot {
    PetSnapshot {
        state: resting(),
        usage: UsageSnapshot {
            today_tokens: None,
            weekly_usage_percent: None,
            weekly_reset_at_ms: None,
            synced_at_ms: None,
        },
        sync_state: SyncState::Unavailable,
        tasks: Vec::new(),
    }
}

fn empty_usage() -> UsageSnapshot {
    UsageSnapshot { today_tokens: None, weekly_usage_percent: None, weekly_reset_at_ms: None, synced_at_ms: None }
}

fn resting() -> PetState {
    PetState {
        status: Status::Resting,
        completed_until_ms: None,
    }
}

fn working() -> PetState {
    PetState {
        status: Status::Working,
        completed_until_ms: None,
    }
}

fn completed(completed_until_ms: u64) -> PetState {
    PetState {
        status: Status::Completed,
        completed_until_ms: Some(completed_until_ms),
    }
}

fn field<'a>(value: &'a JsonValue, name: &str) -> Option<&'a JsonValue> {
    let JsonValue::Object(fields) = value else {
        return None;
    };
    fields
        .iter()
        .find_map(|(key, value)| (key == name).then_some(value))
}

fn integer(value: &JsonValue) -> Result<u64, ()> {
    let JsonValue::Number(value) = value else {
        return Err(());
    };
    value.parse().map_err(|_| ())
}

fn number(value: &JsonValue) -> Result<f64, ()> {
    let JsonValue::Number(value) = value else {
        return Err(());
    };
    let number = value.parse::<f64>().map_err(|_| ())?;
    number.is_finite().then_some(number).ok_or(())
}

fn string(value: Option<&JsonValue>) -> Option<&str> {
    match value? { JsonValue::String(value) => Some(value), _ => None }
}

#[derive(Debug)]
enum JsonValue {
    Null,
    Bool,
    Number(String),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

struct JsonParser<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> JsonParser<'a> {
    fn parse(input: &'a str) -> Result<JsonValue, ()> {
        let mut parser = Self {
            input: input.as_bytes(),
            offset: 0,
        };
        let value = parser.value()?;
        parser.whitespace();
        (parser.offset == parser.input.len()).then_some(value).ok_or(())
    }

    fn value(&mut self) -> Result<JsonValue, ()> {
        self.whitespace();
        match self.byte()? {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => self.string().map(JsonValue::String),
            b't' => self.literal(b"true", JsonValue::Bool),
            b'f' => self.literal(b"false", JsonValue::Bool),
            b'n' => self.literal(b"null", JsonValue::Null),
            b'-' | b'0'..=b'9' => self.number().map(JsonValue::Number),
            _ => Err(()),
        }
    }

    fn object(&mut self) -> Result<JsonValue, ()> {
        self.consume(b'{')?;
        self.whitespace();
        let mut fields = Vec::new();
        if self.try_consume(b'}') {
            return Ok(JsonValue::Object(fields));
        }
        loop {
            self.whitespace();
            let key = self.string()?;
            if fields.iter().any(|(existing, _)| existing == &key) {
                return Err(());
            }
            self.whitespace();
            self.consume(b':')?;
            let value = self.value()?;
            fields.push((key, value));
            self.whitespace();
            if self.try_consume(b'}') {
                return Ok(JsonValue::Object(fields));
            }
            self.consume(b',')?;
        }
    }

    fn array(&mut self) -> Result<JsonValue, ()> {
        self.consume(b'[')?;
        self.whitespace();
        if self.try_consume(b']') {
            return Ok(JsonValue::Array(Vec::new()));
        }
        let mut values = Vec::new();
        loop {
            values.push(self.value()?);
            self.whitespace();
            if self.try_consume(b']') {
                return Ok(JsonValue::Array(values));
            }
            self.consume(b',')?;
        }
    }

    fn string(&mut self) -> Result<String, ()> {
        self.consume(b'"')?;
        let mut value = String::new();
        loop {
            let byte = self.next()?;
            match byte {
                b'"' => return Ok(value),
                b'\\' => match self.next()? {
                    b'"' => value.push('"'),
                    b'\\' => value.push('\\'),
                    b'/' => value.push('/'),
                    b'b' => value.push('\u{0008}'),
                    b'f' => value.push('\u{000C}'),
                    b'n' => value.push('\n'),
                    b'r' => value.push('\r'),
                    b't' => value.push('\t'),
                    b'u' => {
                        let mut codepoint = 0_u32;
                        for _ in 0..4 {
                            let digit = (self.next()? as char).to_digit(16).ok_or(())?;
                            codepoint = codepoint.checked_mul(16).ok_or(())? + digit;
                        }
                        value.push(char::from_u32(codepoint).unwrap_or('\u{FFFD}'));
                    }
                    _ => return Err(()),
                },
                0..=31 => return Err(()),
                byte if byte.is_ascii() => value.push(byte as char),
                _ => {
                    self.offset -= 1;
                    let remaining = std::str::from_utf8(&self.input[self.offset..]).map_err(|_| ())?;
                    let character = remaining.chars().next().ok_or(())?;
                    value.push(character);
                    self.offset += character.len_utf8();
                }
            }
        }
    }

    fn number(&mut self) -> Result<String, ()> {
        let start = self.offset;
        self.try_consume(b'-');
        if self.try_consume(b'0') {
            if self.byte().is_ok_and(|byte| byte.is_ascii_digit()) {
                return Err(());
            }
        } else {
            self.digits()?;
        }
        if self.try_consume(b'.') {
            self.digits()?;
        }
        if self.try_consume(b'e') || self.try_consume(b'E') {
            let _ = self.try_consume(b'+') || self.try_consume(b'-');
            self.digits()?;
        }
        std::str::from_utf8(&self.input[start..self.offset])
            .map(str::to_owned)
            .map_err(|_| ())
    }

    fn digits(&mut self) -> Result<(), ()> {
        let start = self.offset;
        while self.byte().is_ok_and(|byte| byte.is_ascii_digit()) {
            self.offset += 1;
        }
        (self.offset > start).then_some(()).ok_or(())
    }

    fn literal(&mut self, literal: &[u8], value: JsonValue) -> Result<JsonValue, ()> {
        let end = self.offset.checked_add(literal.len()).ok_or(())?;
        (self.input.get(self.offset..end) == Some(literal)).then_some(()).ok_or(())?;
        self.offset = end;
        Ok(value)
    }

    fn whitespace(&mut self) {
        while self.byte().is_ok_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t')) {
            self.offset += 1;
        }
    }

    fn consume(&mut self, expected: u8) -> Result<(), ()> {
        (self.next()? == expected).then_some(()).ok_or(())
    }

    fn try_consume(&mut self, expected: u8) -> bool {
        if self.byte() == Ok(expected) {
            self.offset += 1;
            true
        } else {
            false
        }
    }

    fn byte(&self) -> Result<u8, ()> {
        self.input.get(self.offset).copied().ok_or(())
    }

    fn next(&mut self) -> Result<u8, ()> {
        let byte = self.byte()?;
        self.offset += 1;
        Ok(byte)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_jsonl_reads_the_initial_project_title_and_latest_tail() {
        let directory = std::env::temp_dir().join(format!("codex-pet-jsonl-window-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("rollout.jsonl");
        let content = format!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"agent_nickname\":\"最新任务\"}}}}\n{}\n{{\"type\":\"event_msg\",\"timestamp\":\"2026-08-10T00:00:02Z\",\"payload\":{{\"type\":\"task_complete\",\"turn_id\":\"latest\",\"completed_at\":2}}}}\n",
            "{\"type\":\"ignored\"}\n".repeat(50_000),
        );
        fs::write(&path, content).unwrap();
        let mut scan = Scan {
            evidence: None,
            usage: None,
            latest_observed_at_ms: None,
            no_turn_waiting_at_ms: None,
            tasks: BTreeMap::new(),
        };

        scan_jsonl(&path, &mut scan, &TitleCatalog::default()).unwrap();

        let task = scan.tasks.get("latest").unwrap();
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(task.title, "Codex 任务");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn scan_sources_keeps_a_running_task_when_its_start_event_is_in_a_large_log_middle() {
        let directory = std::env::temp_dir().join(format!("codex-pet-middle-task-{}", std::process::id()));
        let sessions = directory.join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        let path = sessions.join("active.jsonl");
        let mut content = concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"thread_source\":\"user\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"制作 Codex 桌宠灵动岛\"}]}}\n",
        ).to_owned();
        content.push_str(&"{\"type\":\"ignored\"}\n".repeat(12_000));
        content.push_str("{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"still-working\",\"started_at\":100}}\n");
        content.push_str(&"{\"type\":\"ignored\"}\n".repeat(40_000));
        fs::write(&path, content).unwrap();

        let scan = scan_sources(&directory).unwrap();

        assert_eq!(scan.tasks.get("still-working").map(|task| task.status), Some(TaskStatus::Working));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn scan_sources_ignores_invalid_middle_bytes_while_recovering_running_tasks() {
        let directory = std::env::temp_dir().join(format!("codex-pet-invalid-middle-{}", std::process::id()));
        let sessions = directory.join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        let path = sessions.join("active.jsonl");
        let mut content = concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"thread_source\":\"user\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"翻译大师\"}]}}\n",
        ).as_bytes().to_vec();
        content.extend_from_slice("{\"type\":\"ignored\"}\n".repeat(12_000).as_bytes());
        content.extend_from_slice(b"\xff\n");
        content.extend_from_slice(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"still-working\",\"started_at\":100}}\n");
        content.extend_from_slice("{\"type\":\"ignored\"}\n".repeat(40_000).as_bytes());
        fs::write(&path, content).unwrap();

        let scan = scan_sources(&directory).unwrap();

        assert_eq!(scan.tasks.get("still-working").map(|task| task.status), Some(TaskStatus::Working));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn scan_sources_keeps_valid_project_data_when_another_session_is_malformed() {
        let directory = std::env::temp_dir().join(format!("codex-pet-malformed-session-{}", std::process::id()));
        let sessions = directory.join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        fs::write(sessions.join("broken.jsonl"), "{not valid json}\n").unwrap();
        fs::write(sessions.join("valid.jsonl"), concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"thread_source\":\"user\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"翻译大师\"}]}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"working\",\"started_at\":100}}\n",
        )).unwrap();

        let scan = scan_sources(&directory).unwrap();

        assert_eq!(scan.tasks.get("working").map(|task| task.status), Some(TaskStatus::Working));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn scan_jsonl_does_not_turn_a_later_follow_up_into_a_duplicate_project() {
        let directory = std::env::temp_dir().join(format!("codex-pet-project-title-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("rollout.jsonl");
        let mut content = concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"thread_source\":\"user\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"请帮我制作 Codex 桌宠灵动岛\"}]}}\n",
        ).to_owned();
        content.push_str(&"{\"type\":\"ignored\"}\n".repeat(50_000));
        content.push_str(concat!(
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"这个工具只涵盖codex，还是也涵盖ChatGPT\"}]}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"same-project\",\"completed_at\":2}}\n",
        ));
        fs::write(&path, content).unwrap();
        let mut scan = Scan { evidence: None, usage: None, latest_observed_at_ms: None, no_turn_waiting_at_ms: None, tasks: BTreeMap::new() };

        scan_jsonl(&path, &mut scan, &TitleCatalog::default()).unwrap();

        assert_eq!(scan.tasks["same-project"].title, "Codex桌宠灵动岛");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn scan_jsonl_uses_the_first_real_user_request_as_the_task_title() {
        let directory = std::env::temp_dir().join(format!("codex-pet-user-title-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("rollout.jsonl");
        fs::write(&path, concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"thread_source\":\"user\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"<environment>ignored</environment>\"},{\"type\":\"input_text\",\"text\":\"制作 Codex 桌宠灵动岛\"}]}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"title\",\"started_at\":1}}\n",
        )).unwrap();
        let mut scan = Scan { evidence: None, usage: None, latest_observed_at_ms: None, no_turn_waiting_at_ms: None, tasks: BTreeMap::new() };

        scan_jsonl(&path, &mut scan, &TitleCatalog::default()).unwrap();

        assert_eq!(scan.tasks["title"].title, "Codex桌宠灵动岛");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn scan_jsonl_skips_internal_heading_before_the_real_user_request() {
        let directory = std::env::temp_dir().join(format!("codex-pet-internal-title-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("rollout.jsonl");
        fs::write(&path, concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"thread_source\":\"user\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"# filesystem\\n<environment>ignored</environment>\\n修复会议专家管理系统\"}]}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"title\",\"started_at\":1}}\n",
        )).unwrap();
        let mut scan = Scan { evidence: None, usage: None, latest_observed_at_ms: None, no_turn_waiting_at_ms: None, tasks: BTreeMap::new() };

        scan_jsonl(&path, &mut scan, &TitleCatalog::default()).unwrap();

        assert_eq!(scan.tasks["title"].title, "修复会议专家管理系统");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn user_project_title_skips_injected_setup_and_uses_the_quoted_project_name() {
        let record = JsonParser::parse(r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":"<recommended_plugins>\nHere is a list of plugins that are available but not installed.\n- Plugin A\n</recommended_plugins>"},{"text":"请帮我在 Windows 电脑上制作一个桌面悬浮的“Codex 桌宠灵动岛”。"}]}}"#).unwrap();

        assert_eq!(user_task_title(&record).as_deref(), Some("Codex桌宠灵动岛"));
    }

    #[test]
    fn stable_project_names_replace_dynamic_codex_and_translate_labels() {
        assert_eq!(shorten_title("修复 Codex 桌宠灵动岛仍显示休息中"), "Codex桌宠灵动岛");
        assert_eq!(shorten_title("#translate-medical-papers"), "翻译大师");
    }

    #[test]
    fn nested_session_uses_its_top_level_sidebar_title() {
        let catalog = TitleCatalog {
            titles: BTreeMap::from([("project".to_owned(), "翻译大师".to_owned())]),
            parents: BTreeMap::from([("child".to_owned(), "project".to_owned())]),
        };

        assert_eq!(catalog.project_title("child").as_deref(), Some("翻译大师"));
    }

    #[test]
    fn unknown_task_never_uses_a_turn_id_as_its_title() {
        assert_eq!(task_fallback_title("01a7eb5b-0000-0000-0000-000000000000"), "Codex 任务");
    }

    #[test]
    fn task_rows_prefer_the_project_title_over_the_execution_label() {
        let payload = JsonParser::parse(
            r#"{"turn_id":"repair","title":"修复 Codex 桌宠无法拖动"}"#,
        )
        .unwrap();
        let mut scan = Scan {
            evidence: None,
            usage: None,
            latest_observed_at_ms: None,
            no_turn_waiting_at_ms: None,
            tasks: BTreeMap::new(),
        };

        update_task(
            &mut scan,
            &payload,
            Some("制作 Codex 桌宠灵动岛"),
            TaskStatus::Working,
            1_000,
        );

        assert_eq!(scan.tasks["repair"].title, "Codex桌宠灵动岛");
    }

    #[test]
    fn desktop_usage_exposes_the_real_weekly_reset_timestamp() {
        let payload = JsonParser::parse(r#"{"rate_limits":{"primary":{"used_percent":63,"resets_at":1787145600}},"info":{"last_token_usage":{"total_tokens":123}}}"#).unwrap();

        let usage = parse_desktop_usage(&payload, Some(1_000)).unwrap();

        assert_eq!(usage.weekly_reset_at_ms, Some(1_787_145_600_000));
    }

    #[test]
    fn scan_jsonl_keeps_subagent_sessions_for_project_activity() {
        let directory = std::env::temp_dir().join(format!("codex-pet-subagent-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("rollout.jsonl");
        fs::write(&path, concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"thread_source\":\"subagent\",\"agent_nickname\":\"Carson\"}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"internal\",\"started_at\":1}}\n",
        )).unwrap();
        let mut scan = Scan { evidence: None, usage: None, latest_observed_at_ms: None, no_turn_waiting_at_ms: None, tasks: BTreeMap::new() };

        scan_jsonl(&path, &mut scan, &TitleCatalog::default()).unwrap();

        assert_eq!(scan.tasks["internal"].title, "Codex 任务");
        assert_eq!(scan.tasks["internal"].status, TaskStatus::Working);
        let _ = fs::remove_dir_all(directory);
    }
}
