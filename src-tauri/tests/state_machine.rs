use codex_pet_island::app::Settings;
use codex_pet_island::model::{Evidence, EvidenceKind, PetState, Status};
use codex_pet_island::state_machine::advance;

fn state(status: Status) -> PetState {
    PetState {
        status,
        completed_until_ms: None,
    }
}

#[test]
fn settings_default_disables_motion() {
    assert!(!Settings::default().motion_enabled);
}

#[test]
fn state_machine_waiting_for_input_does_not_enter_or_maintain_working() {
    let next = advance(
        state(Status::Working),
        Some(Evidence {
            kind: EvidenceKind::WaitingForInput,
            observed_at_ms: 10,
        }),
        10,
    );

    assert_eq!(next, state(Status::Resting));
}

#[test]
fn state_machine_process_seen_does_not_enter_or_maintain_working() {
    let next = advance(
        state(Status::Working),
        Some(Evidence {
            kind: EvidenceKind::ProcessSeen,
            observed_at_ms: 10,
        }),
        10,
    );

    assert_eq!(next, state(Status::Resting));
}

#[test]
fn state_machine_execution_started_enters_working() {
    let next = advance(
        state(Status::Resting),
        Some(Evidence {
            kind: EvidenceKind::ExecutionStarted,
            observed_at_ms: 10,
        }),
        10,
    );

    assert_eq!(next, state(Status::Working));
}

#[test]
fn state_machine_execution_heartbeat_maintains_working() {
    let next = advance(
        state(Status::Working),
        Some(Evidence {
            kind: EvidenceKind::ExecutionHeartbeat,
            observed_at_ms: 10,
        }),
        10,
    );

    assert_eq!(next, state(Status::Working));
}

#[test]
fn state_machine_execution_completed_at_zero_enters_completed_for_eight_seconds() {
    let next = advance(
        state(Status::Working),
        Some(Evidence {
            kind: EvidenceKind::ExecutionCompleted,
            observed_at_ms: 0,
        }),
        0,
    );

    assert_eq!(
        next,
        PetState {
            status: Status::Completed,
            completed_until_ms: Some(8_000),
        }
    );
}

#[test]
fn state_machine_stale_execution_completed_evidence_returns_resting_at_eight_seconds() {
    let next = advance(
        state(Status::Working),
        Some(Evidence {
            kind: EvidenceKind::ExecutionCompleted,
            observed_at_ms: 0,
        }),
        8_000,
    );

    assert_eq!(next, state(Status::Resting));
}

#[test]
fn state_machine_completed_state_remains_completed_at_seven_seconds_without_new_evidence() {
    let next = advance(
        PetState {
            status: Status::Completed,
            completed_until_ms: Some(8_000),
        },
        None,
        7_000,
    );

    assert_eq!(
        next,
        PetState {
            status: Status::Completed,
            completed_until_ms: Some(8_000),
        }
    );
}

#[test]
fn state_machine_completed_state_returns_to_resting_at_eight_seconds_without_new_evidence() {
    let next = advance(
        PetState {
            status: Status::Completed,
            completed_until_ms: Some(8_000),
        },
        None,
        8_000,
    );

    assert_eq!(next, state(Status::Resting));
}

#[test]
fn state_machine_waiting_for_input_acknowledges_completion_immediately() {
    let next = advance(
        PetState {
            status: Status::Completed,
            completed_until_ms: Some(8_000),
        },
        Some(Evidence {
            kind: EvidenceKind::WaitingForInput,
            observed_at_ms: 1,
        }),
        1,
    );

    assert_eq!(next, state(Status::Resting));
}

#[test]
fn state_machine_missing_evidence_does_not_maintain_working() {
    let next = advance(state(Status::Working), None, 10);

    assert_eq!(next, state(Status::Resting));
}
