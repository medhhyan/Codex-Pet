# Project Title Resolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve every Codex task row to the sidebar project title, including nested subagent tasks.

**Architecture:** Build a title catalog from `session_index.jsonl` and persisted titles, then resolve each session to its top-level parent before task events are recorded. The event scanner receives the resolved project title and uses `Codex 任务` as its only fallback.

**Tech Stack:** Rust, Tauri, existing JSONL parser and unit-test module.

## Global Constraints

- The sidebar/session-index title is canonical.
- Subagent task rows use the top-level parent title.
- UUIDs, agent nicknames, file names, and execution labels must not be shown as project titles.
- A missing canonical title displays `Codex 任务`.

---

### Task 1: Build Canonical Session Title Catalog

**Files:**
- Modify: `src-tauri/src/codex_adapter.rs`
- Test: `src-tauri/src/codex_adapter.rs`

**Interfaces:**
- Produces: `TitleCatalog`, mapping session IDs to canonical sidebar titles and top-level owner IDs.
- Consumes: `session_index.jsonl`, `.codex-global-state.json`, and session metadata.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn nested_session_uses_its_top_level_sidebar_title() {
    let title = resolve_project_title(
        "child",
        &titles([( "project", "翻译大师" )]),
        &parents([( "child", "project" )]),
    );
    assert_eq!(title, "翻译大师");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib nested_session_uses_its_top_level_sidebar_title`

Expected: FAIL because the resolution function does not exist.

- [ ] **Step 3: Write minimal implementation**

```rust
fn resolve_project_title(session_id: &str, titles: &BTreeMap<String, String>, parents: &BTreeMap<String, String>) -> String {
    let owner = top_level_owner(session_id, parents);
    titles.get(owner).cloned().unwrap_or_else(|| "Codex 任务".to_owned())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib nested_session_uses_its_top_level_sidebar_title`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/codex_adapter.rs
git commit -m "fix: resolve subagent tasks to project title"
```

### Task 2: Use the Catalog for Task Rows

**Files:**
- Modify: `src-tauri/src/codex_adapter.rs`
- Test: `src-tauri/src/codex_adapter.rs`

**Interfaces:**
- Consumes: `TitleCatalog` produced by Task 1.
- Produces: task rows whose title is canonical or `Codex 任务`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn unresolved_task_never_displays_its_turn_id_as_a_title() {
    let task = task_from_event("01a7eb5b-0000-0000-0000-000000000000", None);
    assert_eq!(task.title, "Codex 任务");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib unresolved_task_never_displays_its_turn_id_as_a_title`

Expected: FAIL because the existing fallback returns the turn ID.

- [ ] **Step 3: Write minimal implementation**

```rust
fn task_fallback_title(_: &str) -> &'static str {
    "Codex 任务"
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib unresolved_task_never_displays_its_turn_id_as_a_title`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/codex_adapter.rs
git commit -m "fix: hide task identifiers from island rows"
```

### Task 3: Verify and Package

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Run focused title-resolution tests**

Run: `cargo test --lib project_title`

Expected: PASS with all project-title tests green.

- [ ] **Step 2: Run release compilation check**

Run: `cargo check --release`

Expected: PASS.

- [ ] **Step 3: Build Windows installer**

Run: `tauri build`

Expected: a new NSIS installer in `src-tauri/target/release/bundle/nsis`.

- [ ] **Step 4: Commit version bump**

```bash
git add src-tauri/tauri.conf.json
git commit -m "build: package project title resolution fix"
```
