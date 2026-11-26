use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkState {
    Preparing,
    Active,
    RecoveryRequired,
    Finalizing,
    Completed,
    Blocked,
    Failed,
    Cancelled,
}
impl WorkState {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Blocked | Self::Failed | Self::Cancelled
        )
    }
}
pub fn can_transition(from: WorkState, to: WorkState) -> bool {
    use WorkState::*;
    matches!(
        (from, to),
        (Preparing, Active | Finalizing)
            | (Active, RecoveryRequired | Finalizing)
            | (RecoveryRequired, Active | Finalizing)
            | (Finalizing, Completed | Blocked | Failed | Cancelled)
    )
}

pub fn issue_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("backlog", "ready" | "blocked")
            | ("ready", "claimed" | "blocked" | "backlog")
            | ("claimed", "running" | "blocked" | "ready")
            | ("running", "review" | "blocked" | "ready" | "done")
            | ("review", "done" | "rework" | "blocked")
            | ("rework", "ready" | "blocked")
            | ("blocked", "ready" | "backlog")
            | ("done", "rework" | "backlog")
    )
}

pub fn session_terminal(state: &str) -> bool {
    matches!(state, "ended" | "failed" | "cancelled")
}
