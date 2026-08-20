# Codex 桌宠灵动岛 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Windows Tauri + React floating Codex pet that reports verified Codex work state and local usage.

**Architecture:** A Rust `CodexDataAdapter` reads only local Codex session/state sources and emits a conservative `PetSnapshot`; a pure state machine makes all status transitions testable. React renders the transparent draggable island and invokes focused Tauri commands for tray, visibility and settings.

**Tech Stack:** Tauri 2.10, Rust, React 19, TypeScript, Vite, Vitest, `tauri-plugin-single-instance`, `tauri-plugin-autostart`, `tauri-plugin-positioner`, `tauri-plugin-store`.

## Global Constraints

- Target Windows only; create a transparent, undecorated, always-on-top webview with `noRedirectionBitmap` to prevent white flashes.
- Do not use a Codex process/window, keyboard/mouse activity, or idle time as evidence of working.
- Missing, stale, or unknown Codex local data yields `resting` and a visible “暂未同步” value; never estimate usage.
- `completed` remains visible at most eight seconds and returns to `resting` after acknowledgement or timeout.
- Initial island 360×160; collapsed island remains at least 240×92.
- Keyboard/mouse visual effects default to disabled.

---

## File Structure

- `package.json`, `vite.config.ts`, `src/main.tsx`: React/Vite project and test entry points.
- `src/App.tsx`, `src/styles.css`: island state rendering, expand/collapse and visual treatment.
- `src/lib/pet-api.ts`, `src/lib/types.ts`: typed command/event bridge.
- `src/assets/pets/{working,completed,resting}.png`: transparent panda source assets.
- `src-tauri/src/model.rs`: shared snapshot, usage, state and settings types.
- `src-tauri/src/state_machine.rs`: pure conservative state transition logic.
- `src-tauri/src/codex_adapter.rs`: local Codex source detection, parsing and polling.
- `src-tauri/src/app.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/main.rs`: commands, tray and application bootstrap.
- `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`: Windows window configuration and least-privilege commands.
- `src-tauri/tests/{state_machine,adapter}.rs`, `src/**/*.test.tsx`: automated checks.

### Task 1: Scaffold the Tauri desktop shell

**Files:**
- Create: `package.json`, `vite.config.ts`, `index.html`, `tsconfig.json`, `src/main.tsx`, `src/App.tsx`, `src/styles.css`
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`
- Test: `src/App.test.tsx`

**Interfaces:**
- Produces: `npm run test`, `npm run tauri dev`, and a React root with `<App />`.

- [ ] **Step 1: Write the failing UI mount test**

```tsx
import { render, screen } from '@testing-library/react';
import App from './App';

it('renders the pet island', () => {
  render(<App />);
  expect(screen.getByLabelText('Codex 桌宠灵动岛')).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm run test -- --run src/App.test.tsx`

Expected: FAIL because the Vite project and `App` do not exist.

- [ ] **Step 3: Create the minimal Tauri + React setup**

Configure a Tauri 2.10 application with an `island` webview: `transparent: true`, `decorations: false`, `alwaysOnTop: true`, `width: 360`, `height: 160`, `resizable: false`, and `windowEffects` omitted. Configure the Windows `noRedirectionBitmap` attribute. Put `aria-label="Codex 桌宠灵动岛"` on the root app landmark.

```tsx
export default function App() {
  return <main aria-label="Codex 桌宠灵动岛" />;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npm run test -- --run src/App.test.tsx`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add package.json vite.config.ts index.html tsconfig.json src src-tauri
git commit -m "feat: scaffold Tauri pet island"
```

### Task 2: Implement verified-state model and state machine

**Files:**
- Create: `src-tauri/src/model.rs`, `src-tauri/src/state_machine.rs`, `src-tauri/tests/state_machine.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `Evidence { kind: EvidenceKind, observed_at_ms: u64 }`.
- Produces: `advance(previous: PetState, evidence: Option<Evidence>, now_ms: u64) -> PetState`.

- [ ] **Step 1: Write failing Rust tests**

```rust
#[test]
fn waiting_or_process_evidence_never_marks_working() {
    assert_eq!(advance(PetState::Resting, Some(Evidence::waiting(10)), 10).status, Status::Resting);
}

#[test]
fn completion_expires_after_eight_seconds() {
    let complete = advance(PetState::Working, Some(Evidence::completed(100)), 100);
    assert_eq!(advance(complete, None, 8_100).status, Status::Resting);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml state_machine`

Expected: FAIL because `PetState`, `Evidence`, and `advance` are absent.

- [ ] **Step 3: Implement the pure transition function**

Define `Status::{Working, Completed, Resting}` and evidence kinds `ExecutionStarted`, `ExecutionHeartbeat`, `ExecutionCompleted`, `WaitingForInput`, and `ProcessSeen`. Only the first two enter/maintain working. Completion stores `completed_until_ms = observed_at_ms + 8_000`; acknowledgement resets to resting. Every other unknown/missing evidence preserves resting or lets completion expire.

```rust
pub fn advance(previous: PetState, evidence: Option<Evidence>, now_ms: u64) -> PetState;
```

- [ ] **Step 4: Run state-machine tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml state_machine`

Expected: PASS, including waiting, process-only, execution start, completion, acknowledgement, and 8-second expiry.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/model.rs src-tauri/src/state_machine.rs src-tauri/tests/state_machine.rs src-tauri/src/lib.rs
git commit -m "feat: add conservative Codex work state machine"
```

### Task 3: Add a version-tolerant local Codex adapter

**Files:**
- Create: `src-tauri/src/codex_adapter.rs`, `src-tauri/tests/adapter.rs`, `src-tauri/tests/fixtures/{idle,working,completed}/`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `CodexPaths { home: PathBuf }` and local JSONL/SQLite event records.
- Produces: `CodexDataAdapter::poll(&mut self, now_ms: u64) -> PetSnapshot`.

- [ ] **Step 1: Write adapter fixture tests**

```rust
#[test]
fn inactive_session_is_resting_not_working() {
    assert_eq!(fixture_snapshot("idle").status, Status::Resting);
}

#[test]
fn missing_usage_is_not_invented() {
    assert_eq!(fixture_snapshot("idle").usage.today_tokens, None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml adapter`

Expected: FAIL because the adapter and fixtures do not exist.

- [ ] **Step 3: Implement source probing and parsing**

Probe `$USERPROFILE/.codex/session_index.jsonl`, current `sessions/` records, and versioned SQLite state/log databases read-only. Map only explicit execution start/heartbeat/completion markers into `Evidence`; map waiting markers into `WaitingForInput`; ignore process metadata. Extract today token and weekly quota only from named, validated fields. Attach `synced_at_ms`; emit `None` usage and `sync_state: Unavailable` whenever a field or source cannot be validated.

```rust
pub struct PetSnapshot { pub state: PetState, pub usage: UsageSnapshot, pub sync_state: SyncState }
```

- [ ] **Step 4: Run adapter tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml adapter`

Expected: PASS for idle, working, completion, unavailable source, malformed input, and absent usage.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/codex_adapter.rs src-tauri/tests/adapter.rs src-tauri/tests/fixtures src-tauri/src/lib.rs
git commit -m "feat: read verified local Codex activity"
```

### Task 4: Expose snapshots, settings, tray, autostart and single instance

**Files:**
- Create: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/main.rs`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`
- Test: `src-tauri/tests/state_machine.rs`

**Interfaces:**
- Produces: commands `get_snapshot() -> PetSnapshot`, `set_collapsed(bool)`, `set_motion_enabled(bool)`, `set_autostart(bool)`, `hide_to_tray()`, and event `pet://snapshot`.

- [ ] **Step 1: Write failing command-contract tests**

```rust
#[test]
fn default_settings_disable_motion() {
    assert!(!Settings::default().motion_enabled);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml default_settings_disable_motion`

Expected: FAIL because `Settings` is absent.

- [ ] **Step 3: Implement app integration**

Register Tauri single-instance, autostart, store, and positioner plugins. A second launch restores and focuses `island`. Configure a tray menu with restore, collapse toggle, motion toggle, autostart toggle, and quit. Intercept close to hide the window. Poll the adapter once per second and emit `pet://snapshot`; preserve user settings in the plugin store.

```rust
#[tauri::command]
fn set_motion_enabled(enabled: bool, state: State<AppState>) -> Result<(), String>;
```

- [ ] **Step 4: Run Rust tests and check permissions**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS. Confirm capabilities include only required core window actions and plugin permissions.

- [ ] **Step 5: Commit**

```bash
git add src-tauri
git commit -m "feat: add tray and desktop controls"
```

### Task 5: Generate and validate the three panda assets

**Files:**
- Create: `src/assets/pets/working.png`, `src/assets/pets/completed.png`, `src/assets/pets/resting.png`
- Create: `src/assets/pets/README.md`

**Interfaces:**
- Produces: three alpha-enabled PNG assets with a consistent black hoodie panda-programmer character.

- [ ] **Step 1: Generate working asset**

Use the supplied panda poster as a style reference. Create a cold-faced panda in black hoodie typing at a dark laptop, high-contrast poster ink texture, orange UI accent, isolated on a single green chroma-key background, without any text, border, shadow or watermark.

- [ ] **Step 2: Generate completed and resting assets**

Create the same panda happy and holding a checked coffee cup for completed, then happily resting next to a closed laptop for resting. Keep pose, outfit, canvas, line texture and character identity consistent; use green and blue accents respectively.

- [ ] **Step 3: Remove chroma key and validate alpha**

Run the image-generation skill’s `remove_chroma_key.py` helper for each asset. Verify alpha is present, corners are transparent, and no green fringe remains. Save final assets to the paths above.

- [ ] **Step 4: Record exact prompts and source reference**

Document the three prompts, reference image role, chroma-key conversion, final resolution, and the final asset paths in `src/assets/pets/README.md`.

- [ ] **Step 5: Commit**

```bash
git add src/assets/pets
git commit -m "feat: add panda status assets"
```

### Task 6: Build the island experience

**Files:**
- Create: `src/lib/types.ts`, `src/lib/pet-api.ts`, `src/components/{PetArtwork,UsagePanel,IslandControls}.tsx`
- Modify: `src/App.tsx`, `src/styles.css`
- Test: `src/App.test.tsx`, `src/components/IslandControls.test.tsx`

**Interfaces:**
- Consumes: `PetSnapshot` and `Settings` from `pet-api.ts`.
- Produces: `App` controls that invoke the commands from Task 4.

- [ ] **Step 1: Write failing UI behavior tests**

```tsx
it('shows no fake usage when local sync is unavailable', () => {
  render(<App snapshot={unavailableSnapshot} />);
  expect(screen.getByText('暂未同步')).toBeVisible();
});

it('collapses to the readable compact island', async () => {
  render(<App snapshot={restingSnapshot} />);
  await userEvent.click(screen.getByRole('button', { name: '收起' }));
  expect(screen.getByLabelText('紧凑桌宠')).toBeVisible();
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run test -- --run src/App.test.tsx src/components/IslandControls.test.tsx`

Expected: FAIL because typed snapshots and controls are absent.

- [ ] **Step 3: Implement visual states and controls**

Render the matching panda asset and Chinese label for each status: orange “搬砖中”, green “任务完成”, blue “休息中”. Show a weekly percentage progress bar and today tokens only when supplied. Start a countdown from `completedUntilMs`; invoke acknowledgement on click. Make the non-control header draggable through `data-tauri-drag-region`. Implement visible expand, collapse, hide-to-tray, and motion buttons; collapsed CSS must have `min-width: 240px; min-height: 92px`.

```ts
export type PetSnapshot = { status: 'working' | 'completed' | 'resting'; completedUntilMs?: number; usage: UsageSnapshot; syncState: 'ready' | 'stale' | 'unavailable' };
```

- [ ] **Step 4: Run frontend tests and production build**

Run: `npm run test -- --run`

Expected: PASS.

Run: `npm run build`

Expected: PASS with a Vite production bundle.

- [ ] **Step 5: Commit**

```bash
git add src
git commit -m "feat: render Codex pet island"
```

### Task 7: Package and perform Windows acceptance verification

**Files:**
- Create: `README.md`, `docs/verification/windows-acceptance.md`
- Modify: `package.json`, `src-tauri/tauri.conf.json`

**Interfaces:**
- Produces: signed-ready Windows installer configuration and reproducible acceptance instructions.

- [ ] **Step 1: Write the acceptance matrix**

Create a table with scenarios: Codex opened but waiting, verified task executes, completion at 0/7/8 seconds, missing data, expand/collapse, drag, hide/restore tray, second launch focus, autostart toggle, and motion default/enable. Each row contains action and expected visible result.

- [ ] **Step 2: Build the Windows installer**

Run: `npm run tauri build`

Expected: an `.msi` or NSIS installer in `src-tauri/target/release/bundle/`.

- [ ] **Step 3: Run acceptance checks**

Install locally and execute every acceptance-matrix row. Record actual version, installer path, and failures in `docs/verification/windows-acceptance.md`. If Codex source format is unavailable, record `resting` + “暂未同步” as the expected safe fallback.

- [ ] **Step 4: Commit**

```bash
git add README.md docs/verification package.json src-tauri/tauri.conf.json
git commit -m "docs: add Windows packaging verification"
```
