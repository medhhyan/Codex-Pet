import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { IslandControls } from './components/IslandControls';
import { PetArtwork } from './components/PetArtwork';
import { UsagePanel } from './components/UsagePanel';
import { TaskOverview } from './components/TaskOverview';
import { acknowledgeCompletion, dismissCompletedTask, getSettings, getSnapshot, hideToTray, onSettings, onSnapshot, setMotionEnabled } from './lib/pet-api';
import type { PetSnapshot, Status } from './lib/types';
import './styles.css';

const fallbackSnapshot: PetSnapshot = {
  state: { status: 'resting', completedUntilMs: null },
  usage: { todayTokens: null, weeklyUsagePercent: null, weeklyResetAtMs: null, syncedAtMs: null },
  syncState: 'unavailable',
  tasks: [],
};

const statusContent: Record<Status, { label: string }> = {
  working: { label: '搬砖中' },
  completed: { label: '任务完成' },
  resting: { label: '休息中' },
};

export default function App({ snapshot: suppliedSnapshot }: { snapshot?: PetSnapshot }) {
  const [snapshot, setSnapshot] = useState(suppliedSnapshot ?? fallbackSnapshot);
  const [motionEnabled, setMotionEnabledState] = useState(false);
  const [completionSeconds, setCompletionSeconds] = useState<number | null>(null);
  const [mouseOverIsland, setMouseOverIsland] = useState(false);
  const status = snapshot.state.status;
  const content = statusContent[status];

  useEffect(() => {
    if (suppliedSnapshot) {
      setSnapshot(suppliedSnapshot);
      return;
    }
    void getSnapshot().then(setSnapshot).catch(() => setSnapshot(fallbackSnapshot));
    void getSettings().then((settings) => {
      setMotionEnabledState(settings.motionEnabled);
    }).catch(() => undefined);
    let stop: (() => void) | undefined;
    let stopSettings: (() => void) | undefined;
    void onSnapshot(setSnapshot).then((unlisten) => { stop = unlisten; });
    void onSettings((settings) => {
      setMotionEnabledState(settings.motionEnabled);
    }).then((unlisten) => { stopSettings = unlisten; });
    return () => { stop?.(); stopSettings?.(); };
  }, [suppliedSnapshot]);

  useEffect(() => {
    if (status !== 'completed' || !snapshot.state.completedUntilMs) {
      setCompletionSeconds(null);
      return;
    }
    const update = () => setCompletionSeconds(Math.max(0, Math.ceil((snapshot.state.completedUntilMs! - Date.now()) / 1000)));
    update();
    const timer = window.setInterval(update, 250);
    return () => window.clearInterval(timer);
  }, [status, snapshot.state.completedUntilMs]);

  const changeMotion = (next: boolean) => {
    setMotionEnabledState(next);
    void setMotionEnabled(next).catch(() => setMotionEnabledState(!next));
  };
  const confirmCompletion = () => {
    if (status === 'completed') void acknowledgeCompletion().catch(() => undefined);
  };
  const dismissTask = (turnId: string) => {
    setSnapshot((current) => ({ ...current, tasks: (current.tasks ?? []).filter((task) => task.turnId !== turnId) }));
    void dismissCompletedTask(turnId).catch(() => { void getSnapshot().then(setSnapshot).catch(() => undefined); });
  };
  const startWindowDrag = (event: React.MouseEvent<HTMLDivElement>) => {
    if (event.button !== 0 || (event.target instanceof Element && event.target.closest('button'))) return;
    void getCurrentWindow().startDragging().catch(() => undefined);
  };

  return (
    <main
      className={`island island--${status} ${mouseOverIsland ? 'island--mouse-over' : 'island--mouse-away'} ${motionEnabled ? 'island--motion' : ''}`}
      aria-label="Codex 桌宠灵动岛"
      onMouseEnter={() => setMouseOverIsland(true)}
      onMouseLeave={() => setMouseOverIsland(false)}
      onFocusCapture={() => setMouseOverIsland(true)}
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setMouseOverIsland(false);
      }}
    >
      <div className="island-dragbar" data-tauri-drag-region aria-label="拖动桌宠" onMouseDown={startWindowDrag}>
        <span className="drag-handle" data-tauri-drag-region>Codex</span>
        <IslandControls motionEnabled={motionEnabled} onToggleMotion={changeMotion} onHide={() => void hideToTray()} />
      </div>
      <div className="island-body">
        <PetArtwork status={status} motionEnabled={motionEnabled} />
        <div className="island-status-area">
          <button type="button" className="status-card" onClick={confirmCompletion} aria-label={status === 'completed' ? '确认任务完成' : undefined}>
            <span className="status-dot" />
            <span className="status-copy"><strong>{content.label}</strong></span>
            {completionSeconds !== null && <time>{completionSeconds}s</time>}
          </button>
          <TaskOverview tasks={snapshot.tasks} onDismissCompleted={dismissTask} />
        </div>
        <UsagePanel usage={snapshot.usage} />
      </div>
    </main>
  );
}
