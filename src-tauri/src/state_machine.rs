use crate::model::{Evidence, EvidenceKind, PetState, Status};

const COMPLETED_DURATION_MS: u64 = 8_000;

pub fn advance(previous: PetState, evidence: Option<Evidence>, now_ms: u64) -> PetState {
    match evidence {
        Some(Evidence {
            kind: EvidenceKind::ExecutionStarted | EvidenceKind::ExecutionHeartbeat,
            ..
        }) => working(),
        Some(Evidence {
            kind: EvidenceKind::ExecutionCompleted,
            observed_at_ms,
        }) => observed_at_ms
            .checked_add(COMPLETED_DURATION_MS)
            .filter(|&completed_until_ms| now_ms < completed_until_ms)
            .map(completed)
            .unwrap_or_else(resting),
        Some(Evidence {
            kind: EvidenceKind::WaitingForInput,
            ..
        }) if previous.status == Status::Completed => resting(),
        _ if completion_is_current(previous, now_ms) => previous,
        _ => resting(),
    }
}

fn completion_is_current(state: PetState, now_ms: u64) -> bool {
    state.status == Status::Completed
        && state
            .completed_until_ms
            .is_some_and(|completed_until_ms| now_ms < completed_until_ms)
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

fn resting() -> PetState {
    PetState {
        status: Status::Resting,
        completed_until_ms: None,
    }
}
