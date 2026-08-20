import type { TaskSnapshot, TaskStatus } from '../lib/types';

const labels: Record<TaskStatus, string> = { working: '进行中', completed: '已完成', waiting: '待处理' };

function count(tasks: TaskSnapshot[], status: TaskStatus) {
  return tasks.filter((task) => task.status === status).length;
}

export function TaskOverview({ tasks = [], onDismissCompleted }: { tasks?: TaskSnapshot[]; onDismissCompleted?: (turnId: string) => void }) {
  if (tasks.length === 0) return null;
  const working = count(tasks, 'working');
  const completed = count(tasks, 'completed');
  const waiting = count(tasks, 'waiting');
  const total = tasks.length;
  const workingEnd = (working / total) * 100;
  const completedEnd = workingEnd + (completed / total) * 100;
  return (
    <section className="task-overview" aria-label="Codex 任务概览">
      <div className="task-summary">
        <span className="task-ring" aria-label={`${working} 项进行中，${completed} 项已完成，${waiting} 项待处理`} style={{ background: `conic-gradient(#ff9d3d 0 ${workingEnd}%, #44d58e ${workingEnd}% ${completedEnd}%, #3e9bff ${completedEnd}% 100%)` }} />
        <div className="task-counts">{working > 0 && <span>{working} 项进行中</span>}{completed > 0 && <span>{completed} 项已完成</span>}{waiting > 0 && <span>{waiting} 项待处理</span>}</div>
      </div>
      <div className="task-list">{tasks.slice(0, 4).map((task) => task.status === 'completed' && onDismissCompleted ? (
        <button type="button" className="task-row task-row--completed" key={task.turnId} aria-label={`移除已完成项目 ${task.title}`} title="点击移除" onClick={() => onDismissCompleted(task.turnId)}><span className="task-row__dot" /><span className="task-row__title" title={task.title}>{task.title}</span><small>{labels[task.status]}<span className="task-row__remove task-row__remove--glass" aria-hidden="true">×</span></small></button>
      ) : <div className={`task-row task-row--${task.status}`} key={task.turnId}><span className="task-row__dot" /><span className="task-row__title" title={task.title}>{task.title}</span><small>{labels[task.status]}</small></div>)}</div>
    </section>
  );
}
