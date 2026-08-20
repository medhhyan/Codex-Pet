#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Working,
    Completed,
    Resting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    ExecutionStarted,
    ExecutionHeartbeat,
    ExecutionCompleted,
    WaitingForInput,
    ProcessSeen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetState {
    pub status: Status,
    pub completed_until_ms: Option<u64>,
}
