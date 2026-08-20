import { act, fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { vi } from 'vitest';
import App from './App';

const { startDragging } = vi.hoisted(() => ({ startDragging: vi.fn().mockResolvedValue(undefined) }));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ startDragging }),
}));

const unavailableSnapshot = {
  state: { status: 'resting' as const, completedUntilMs: null },
  usage: { todayTokens: null, weeklyUsagePercent: null, weeklyResetAtMs: null, syncedAtMs: null },
  syncState: 'unavailable' as const,
};

const restingSnapshot = {
  state: { status: 'resting' as const, completedUntilMs: null },
  usage: { todayTokens: 2345, weeklyUsagePercent: 42, weeklyResetAtMs: Date.UTC(2026, 7, 20), syncedAtMs: 1 },
  syncState: 'ready' as const,
};

const activeTasksSnapshot = {
  state: { status: 'working' as const, completedUntilMs: null },
  usage: { todayTokens: 9876, weeklyUsagePercent: 95, weeklyResetAtMs: Date.UTC(2026, 7, 20), syncedAtMs: 1 },
  syncState: 'ready' as const,
  tasks: [
    { turnId: 'turn-illustration', title: '作图大师', status: 'working' as const, observedAtMs: 2_000 },
    { turnId: 'turn-report', title: '整理报告', status: 'working' as const, observedAtMs: 1_900 },
    { turnId: 'turn-review', title: '等待审核', status: 'waiting' as const, observedAtMs: 1_800 },
  ],
};

const completedAndWorkingSnapshot = {
  ...restingSnapshot,
  tasks: [
    { turnId: 'turn-translate', title: '翻译大师', status: 'completed' as const, observedAtMs: 2_000 },
    { turnId: 'turn-report', title: '整理报告', status: 'working' as const, observedAtMs: 1_900 },
  ],
};

it('renders the pet island', () => {
  render(<App />);

  expect(screen.getByLabelText('Codex 桌宠灵动岛')).toBeInTheDocument();
});

it('shows only the three status titles without explanatory copy', () => {
  const { rerender } = render(<App snapshot={activeTasksSnapshot} />);
  expect(screen.getByText('搬砖中')).toBeVisible();
  expect(screen.queryByText('正在执行 Codex 任务')).not.toBeInTheDocument();

  rerender(<App snapshot={{ ...restingSnapshot, state: { status: 'completed', completedUntilMs: Date.now() + 8_000 } }} />);
  expect(screen.getByText('任务完成')).toBeVisible();
  expect(screen.queryByText('点击确认')).not.toBeInTheDocument();

  rerender(<App snapshot={restingSnapshot} />);
  expect(screen.getByText('休息中')).toBeVisible();
  expect(screen.queryByText('等待新的 Codex 任务')).not.toBeInTheDocument();
});

it('uses the lighter mouse-away glass state until the pointer enters the island', () => {
  render(<App snapshot={restingSnapshot} />);

  const island = screen.getByLabelText('Codex 桌宠灵动岛');
  expect(island).toHaveClass('island--mouse-away');

  act(() => {
    fireEvent.mouseEnter(island);
  });
  expect(island).toHaveClass('island--mouse-over');

  act(() => {
    fireEvent.mouseLeave(island);
  });
  expect(island).toHaveClass('island--mouse-away');
});

it('shows no fake usage when local sync is unavailable', () => {
  render(<App snapshot={unavailableSnapshot} />);

  expect(screen.getAllByText('—')).toHaveLength(3);
});

it('renders local task activity and neutral usage labels', () => {
  render(<App snapshot={activeTasksSnapshot} />);

  expect(screen.getByText('作图大师')).toBeVisible();
  expect(screen.getByText('2 项进行中')).toBeVisible();
  expect(screen.getByText('1 项待处理')).toBeVisible();
  expect(screen.getByText('最近同步')).toBeVisible();
  expect(screen.getByText('下次重置')).toBeVisible();
  expect(screen.getByText('最近同步').compareDocumentPosition(screen.getByText('下次重置')) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.getByText('8/20')).toBeVisible();
  expect(screen.getByText('08:00:00')).toBeVisible();
  expect(screen.queryByText('今日 Token')).not.toBeInTheDocument();
  expect(screen.queryByText('暂未同步')).not.toBeInTheDocument();
});

  it('removes only the clicked completed project from the island list', () => {
    render(<App snapshot={completedAndWorkingSnapshot} />);

    const dismissButton = screen.getByRole('button', { name: '移除已完成项目 翻译大师' });
    expect(dismissButton).toHaveAttribute('title', '点击移除');
    expect(dismissButton.querySelector('.task-row__remove')).toHaveClass('task-row__remove--glass');

    fireEvent.click(dismissButton);

  expect(screen.queryByText('翻译大师')).not.toBeInTheDocument();
  expect(screen.getByText('整理报告')).toBeVisible();
  expect(screen.queryByText('1 项已完成')).not.toBeInTheDocument();
});

it('does not offer a collapsed island mode', () => {
  render(<App snapshot={restingSnapshot} />);

  expect(screen.queryByRole('button', { name: '收起' })).not.toBeInTheDocument();
  expect(screen.queryByLabelText('紧凑桌宠')).not.toBeInTheDocument();
});

it('starts a native window drag from the title area', () => {
  render(<App snapshot={restingSnapshot} />);

  fireEvent.mouseDown(screen.getByLabelText('拖动桌宠'), { button: 0 });

  expect(startDragging).toHaveBeenCalledOnce();
});
