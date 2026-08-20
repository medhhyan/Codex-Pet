export type Status = 'working' | 'completed' | 'resting';
export type TaskStatus = 'working' | 'completed' | 'waiting';
export type SyncState = 'ready' | 'stale' | 'unavailable';

export type UsageSnapshot = {
  todayTokens: number | null;
  weeklyUsagePercent: number | null;
  weeklyResetAtMs: number | null;
  syncedAtMs: number | null;
};

export type PetSnapshot = {
  state: {
    status: Status;
    completedUntilMs: number | null;
  };
  usage: UsageSnapshot;
  syncState: SyncState;
  tasks?: TaskSnapshot[];
};

export type TaskSnapshot = {
  turnId: string;
  title: string;
  status: TaskStatus;
  observedAtMs: number;
};

export type PetSettings = { motionEnabled: boolean; autostartEnabled: boolean };

export type Settings = {
  motionEnabled: boolean;
  autostartEnabled: boolean;
};
