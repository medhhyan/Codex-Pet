# Multi-task Pet Overview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show the correct overall state and a compact per-task list from local Codex session records.

**Architecture:** The Rust adapter will aggregate desktop event records by `turn_id`, keep the latest event per task, and separately keep the newest timestamped weekly limit. The React UI will render a task summary and task rows when expanded; the panda continues to represent the aggregate state.

**Tech Stack:** Rust, Tauri 2.11, React 19, Vitest, Rust integration tests.

## Global Constraints

- Windows transparent, topmost Tauri window remains unchanged.
- Working has priority over completed; completed lasts 8 seconds only when no task is working.
- Do not display a today-token metric.
- Weekly usage displays the newest locally written value and its timestamp; never display a waiting-sync placeholder.

---

### Task 1: Aggregate desktop task records by turn

**Files:**
- Modify: `src-tauri/src/codex_adapter.rs`
- Modify: `src-tauri/tests/adapter.rs`

**Interfaces:**
- Produces: `TaskSnapshot { turn_id: String, title: String, status: TaskStatus, observed_at_ms: u64 }` and `PetSnapshot.tasks: Vec<TaskSnapshot>`.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn adapter_keeps_a_working_task_when_another_task_completed_later() {
    let snapshot = poll_fixture("multiple_tasks", 2_001);
    assert_eq!(snapshot.state.status, Status::Working);
    assert_eq!(snapshot.tasks.len(), 2);
}
```

- [ ] **Step 2: Run the targeted test and verify it fails**

Run: `cargo test --offline --test adapter multiple_tasks`
Expected: FAIL because `tasks` does not exist.

- [ ] **Step 3: Implement the minimal aggregation**

```rust
let entry = tasks.entry(turn_id).or_insert(task);
if task.observed_at_ms >= entry.observed_at_ms { *entry = task; }
```

Map `task_started` to working, `task_complete` to completed, and preserve a waiting event as waiting. Derive the aggregate state by checking for working tasks first, then completed tasks within 8 seconds.

- [ ] **Step 4: Run targeted and existing adapter tests**

Run: `cargo test --offline --test adapter`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/codex_adapter.rs src-tauri/tests/adapter.rs
git commit -m "feat: aggregate Codex task states"
```

### Task 2: Choose weekly usage by record timestamp

**Files:**
- Modify: `src-tauri/src/codex_adapter.rs`
- Modify: `src-tauri/tests/adapter.rs`

**Interfaces:**
- Produces: `UsageSnapshot.weekly_usage_percent` and `synced_at_ms` from the newest valid `token_count` timestamp.

- [ ] **Step 1: Write failing test for out-of-order files**

```rust
#[test]
fn adapter_uses_newest_token_count_timestamp_not_file_order() {
    let snapshot = poll_fixture("newest_usage", 2_001);
    assert_eq!(snapshot.usage.weekly_usage_percent, Some(95.0));
}
```

- [ ] **Step 2: Run it to verify failure**

Run: `cargo test --offline --test adapter newest_usage`
Expected: FAIL with an older percentage.

- [ ] **Step 3: Store parsed RFC3339 time with each usage record**

```rust
if scan.usage.is_none_or(|previous| usage.synced_at_ms > previous.synced_at_ms) {
    scan.usage = Some(usage);
}
```

- [ ] **Step 4: Run adapter tests**

Run: `cargo test --offline --test adapter`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/codex_adapter.rs src-tauri/tests/adapter.rs
git commit -m "fix: choose newest locally synced weekly usage"
```

### Task 3: Render the task overview

**Files:**
- Modify: `src/lib/types.ts`
- Create: `src/components/TaskOverview.tsx`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`
- Modify: `src/App.test.tsx`

**Interfaces:**
- Consumes: `PetSnapshot.tasks` with `id`, `title`, `status`, and `observedAtMs`.
- Produces: an expanded task list and summary counts; compact mode remains readable.

- [ ] **Step 1: Write failing UI tests**

```tsx
expect(screen.getByText('作图大师')).toBeVisible();
expect(screen.getByText('2 项进行中')).toBeVisible();
expect(screen.queryByText('今日 Token')).not.toBeInTheDocument();
```

- [ ] **Step 2: Run UI test and verify failure**

Run: `node node_modules/vitest/vitest.mjs run src/App.test.tsx`
Expected: FAIL because task overview is absent.

- [ ] **Step 3: Implement `TaskOverview`**

Render aggregate counts, a compact CSS ring, and up to four task rows with Chinese status labels. Show `最近同步 HH:mm:ss` beneath weekly usage, with the last known percentage retained for stale records.

- [ ] **Step 4: Run frontend tests and build**

Run: `npm test -- --run && npm run build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lib/types.ts src/components/TaskOverview.tsx src/App.tsx src/styles.css src/App.test.tsx
git commit -m "feat: show Codex task overview"
```
