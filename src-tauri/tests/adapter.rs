use std::path::PathBuf;

use codex_pet_island::codex_adapter::{
    CodexDataAdapter, CodexPaths, SyncState,
};
use codex_pet_island::model::Status;

fn fixture_home(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn poll_fixture(name: &str, now_ms: u64) -> codex_pet_island::codex_adapter::PetSnapshot {
    CodexDataAdapter::new(CodexPaths {
        home: fixture_home(name),
    })
    .poll(now_ms)
}

#[test]
fn adapter_waiting_for_input_session_is_resting_and_does_not_fabricate_usage() {
    let snapshot = poll_fixture("idle", 1_001);

    assert_eq!(snapshot.state.status, Status::Resting);
    assert_eq!(snapshot.sync_state, SyncState::Unavailable);
    assert_eq!(snapshot.usage.today_tokens, None);
    assert_eq!(snapshot.usage.weekly_usage_percent, None);
    assert_eq!(snapshot.usage.synced_at_ms, None);
}

#[test]
fn adapter_enters_working_only_for_explicit_execution_started_record() {
    let snapshot = poll_fixture("working", 1_001);

    assert_eq!(snapshot.state.status, Status::Working);
    assert_eq!(snapshot.sync_state, SyncState::Ready);
    assert_eq!(snapshot.usage.today_tokens, Some(42));
    assert_eq!(snapshot.usage.weekly_usage_percent, Some(12.5));
    assert_eq!(snapshot.usage.synced_at_ms, Some(1_000));
}

#[test]
fn adapter_enters_working_for_explicit_execution_heartbeat_record() {
    let snapshot = poll_fixture("heartbeat", 1_001);

    assert_eq!(snapshot.state.status, Status::Working);
    assert_eq!(snapshot.sync_state, SyncState::Ready);
}

#[test]
fn adapter_reads_desktop_codex_task_and_usage_events() {
    let snapshot = poll_fixture("desktop_codex", 1_000_001);

    assert_eq!(snapshot.state.status, Status::Working);
    assert_eq!(snapshot.sync_state, SyncState::Ready);
    assert_eq!(snapshot.usage.today_tokens, Some(42));
    assert_eq!(snapshot.usage.weekly_usage_percent, Some(37.5));
    assert_eq!(snapshot.usage.synced_at_ms, Some(1_002_000));
}

#[test]
fn adapter_uses_the_saved_project_title_and_marks_root_subagent_activity_as_working() {
    let snapshot = poll_fixture("root_subagent_activity", 2_001);

    assert_eq!(snapshot.state.status, Status::Working);
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(snapshot.tasks[0].title, "会议专家管理系统");
    assert_eq!(snapshot.tasks[0].status, codex_pet_island::codex_adapter::TaskStatus::Working);
}

#[test]
fn adapter_counts_a_running_subagent_under_its_parent_project() {
    let snapshot = poll_fixture("running_subagent", 2_001);

    assert_eq!(snapshot.state.status, Status::Working);
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(snapshot.tasks[0].title, "会议专家管理系统");
    assert_eq!(snapshot.tasks[0].status, codex_pet_island::codex_adapter::TaskStatus::Working);
}

#[test]
fn adapter_merges_parent_and_subagent_activity_into_one_project_row() {
    let snapshot = poll_fixture("duplicate_project", 2_001);

    assert_eq!(snapshot.state.status, Status::Working);
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(snapshot.tasks[0].title, "会议专家管理系统");
    assert_eq!(snapshot.tasks[0].status, codex_pet_island::codex_adapter::TaskStatus::Working);
}

#[test]
fn adapter_uses_newest_token_count_timestamp_not_file_order() {
    let snapshot = poll_fixture("newest_usage", 2_001);

    assert_eq!(snapshot.usage.weekly_usage_percent, Some(95.0));
    assert_eq!(snapshot.usage.synced_at_ms, Some(2_000));
}

#[test]
fn adapter_ignores_newer_invalid_weekly_usage_records() {
    let snapshot = poll_fixture("invalid_newest_usage", 3_001);

    assert_eq!(snapshot.usage.weekly_usage_percent, Some(95.0));
    assert_eq!(snapshot.usage.synced_at_ms, Some(2_000));
}

#[test]
fn adapter_retains_last_locally_synced_weekly_usage_when_stale() {
    let snapshot = poll_fixture("fresh_task_stale_usage", 300_001);

    assert_eq!(snapshot.usage.weekly_usage_percent, Some(12.5));
    assert_eq!(snapshot.usage.synced_at_ms, Some(0));
    assert_eq!(snapshot.sync_state, SyncState::Ready);
}

#[test]
fn adapter_keeps_a_working_task_when_another_task_completed_later() {
    let snapshot = poll_fixture("multiple_tasks", 2_001);

    assert_eq!(snapshot.state.status, Status::Working);
    assert_eq!(snapshot.tasks.len(), 2);
    assert!(snapshot.tasks.iter().any(|task| {
        task.turn_id == "watermark" && task.status == codex_pet_island::codex_adapter::TaskStatus::Working
    }));
    assert!(snapshot.tasks.iter().any(|task| {
        task.turn_id == "banner" && task.status == codex_pet_island::codex_adapter::TaskStatus::Completed
    }));
}

#[test]
fn adapter_keeps_a_silent_task_working_while_sync_records_are_fresh() {
    let snapshot = poll_fixture("silent_running_task", 2_700_001);

    assert_eq!(snapshot.state.status, Status::Working);
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(snapshot.tasks[0].status, codex_pet_island::codex_adapter::TaskStatus::Working);
}

#[test]
fn adapter_expires_an_unmatched_historical_desktop_start() {
    let snapshot = poll_fixture("stale_task", 7_201_001);

    assert_eq!(snapshot.state.status, Status::Resting);
    assert!(snapshot.tasks.is_empty());
}

#[test]
fn adapter_keeps_fresh_task_working_when_usage_is_stale() {
    let snapshot = poll_fixture("fresh_task_stale_usage", 300_001);

    assert_eq!(snapshot.state.status, Status::Working);
    assert_eq!(snapshot.sync_state, SyncState::Ready);
}

#[test]
fn adapter_retains_start_title_when_completion_has_no_title() {
    let snapshot = poll_fixture("retained_task_title", 2_001);

    assert_eq!(snapshot.tasks[0].title, "添加水印");
}

#[test]
fn adapter_uses_session_metadata_title_when_task_event_has_none() {
    let snapshot = poll_fixture("metadata_task_title", 1_001);

    assert_eq!(snapshot.tasks[0].title, "翻译专家");
}

#[test]
fn adapter_turns_active_tasks_to_waiting_after_no_turn_waiting_event() {
    let snapshot = poll_fixture("no_turn_waiting", 2_001);

    assert_eq!(snapshot.state.status, Status::Resting);
    assert_eq!(snapshot.tasks[0].status, codex_pet_island::codex_adapter::TaskStatus::Waiting);
}

#[test]
fn adapter_maps_explicit_completion_record_to_completed() {
    let snapshot = poll_fixture("completed", 2_000);

    assert_eq!(snapshot.state.status, Status::Completed);
    assert_eq!(snapshot.state.completed_until_ms, Some(9_000));
}

#[test]
fn adapter_malformed_jsonl_returns_safe_unavailable_snapshot() {
    let snapshot = poll_fixture("malformed", 1_001);

    assert_eq!(snapshot.state.status, Status::Resting);
    assert_eq!(snapshot.sync_state, SyncState::Unavailable);
    assert_eq!(snapshot.usage.today_tokens, None);
    assert_eq!(snapshot.usage.weekly_usage_percent, None);
    assert_eq!(snapshot.usage.synced_at_ms, None);
}

#[test]
fn adapter_unknown_event_kind_returns_safe_unavailable_snapshot() {
    let snapshot = poll_fixture("unknown_event", 1_001);

    assert_eq!(snapshot.state.status, Status::Resting);
    assert_eq!(snapshot.sync_state, SyncState::Unavailable);
    assert_eq!(snapshot.usage.today_tokens, None);
}

#[test]
fn adapter_missing_sources_return_no_fabricated_usage() {
    let missing_home = fixture_home("does-not-exist");
    let snapshot = CodexDataAdapter::new(CodexPaths { home: missing_home }).poll(1_001);

    assert_eq!(snapshot.state.status, Status::Resting);
    assert_eq!(snapshot.sync_state, SyncState::Unavailable);
    assert_eq!(snapshot.usage.today_tokens, None);
    assert_eq!(snapshot.usage.weekly_usage_percent, None);
    assert_eq!(snapshot.usage.synced_at_ms, None);
}
