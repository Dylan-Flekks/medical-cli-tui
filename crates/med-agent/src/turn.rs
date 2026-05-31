// Derived from OpenAI Codex CLI.
//
// Original source:
// - https://github.com/openai/codex
// - codex-rs/core/src/state/turn.rs
// - codex-rs/core/src/tasks/mod.rs
// - upstream snapshot commit: bf72be59278e23002a352a53207182985cabb9d0
//
// Upstream license: Apache-2.0. See this repository's NOTICE and
// docs/CODEX_EXTRACTION_LOG.md for attribution and modification notes.
//
// Modifications by Flekks EMR CLI contributors:
// - replaced Codex coding-agent task state with medical turn state
// - replaced shell/sandbox permission waiters with medical approval waiters
// - added patient/encounter/note context, bounded loop checks, and PHI-safe
//   redacted event summaries
// - kept the Codex active-turn pattern: one running turn owns pending approvals,
//   pending tools, cancellation state, and lifecycle counters

use std::collections::HashMap;

use med_core::{new_id, EncounterId, NoteId, PatientId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

use crate::protocol::{
    MedicalApprovalClass, MedicalApprovalRequest, MedicalApprovalResolvedEvent,
    MedicalApprovalResponse, MedicalLoopLimits, MedicalToolLifecycleEvent, MedicalTurnAbortReason,
    MedicalTurnAbortedEvent, MedicalTurnCompleteEvent, MedicalTurnRequest, MedicalTurnStartedEvent,
    MedicalTurnStepEvent,
};
use crate::MedicalToolName;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveMedicalTurn {
    pub turn_id: String,
    pub patient_id: Option<PatientId>,
    pub encounter_id: Option<EncounterId>,
    pub note_id: Option<NoteId>,
    pub contains_phi: bool,
    pub status: MedicalActiveTurnStatus,
    pub loop_limits: MedicalLoopLimits,
    pub started_at: OffsetDateTime,
    pub steps_completed: u32,
    pub cancellation_requested: bool,
    pending_approvals: HashMap<String, MedicalPendingApproval>,
    pending_tools: HashMap<String, MedicalPendingToolCall>,
}

impl ActiveMedicalTurn {
    pub fn new(
        turn_id: impl Into<String>,
        request: &MedicalTurnRequest,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            turn_id: turn_id.into(),
            patient_id: request.patient_id,
            encounter_id: request.encounter_id,
            note_id: request.note_id,
            contains_phi: request.contains_phi,
            status: MedicalActiveTurnStatus::Running,
            loop_limits: request.loop_limits,
            started_at: now,
            steps_completed: 0,
            cancellation_requested: false,
            pending_approvals: HashMap::new(),
            pending_tools: HashMap::new(),
        }
    }

    pub fn started_event(&self) -> MedicalTurnStartedEvent {
        MedicalTurnStartedEvent {
            turn_id: self.turn_id.clone(),
            patient_id: self.patient_id,
            encounter_id: self.encounter_id,
            note_id: self.note_id,
            contains_phi: self.contains_phi,
            loop_limits: self.loop_limits,
        }
    }

    pub fn record_step(
        &mut self,
        now: OffsetDateTime,
        redacted_summary: impl Into<String>,
    ) -> Result<MedicalTurnStepEvent, ActiveMedicalTurnError> {
        self.ensure_not_cancelled()?;
        self.ensure_within_wall_clock_limit(now)?;

        if self.steps_completed >= self.loop_limits.max_steps {
            self.status = MedicalActiveTurnStatus::Aborted;
            return Err(ActiveMedicalTurnError::LoopStepLimitExceeded {
                turn_id: self.turn_id.clone(),
                max_steps: self.loop_limits.max_steps,
            });
        }

        self.steps_completed += 1;
        Ok(MedicalTurnStepEvent {
            turn_id: self.turn_id.clone(),
            step_index: self.steps_completed,
            redacted_summary: redacted_summary.into(),
        })
    }

    pub fn request_approval(
        &mut self,
        class: MedicalApprovalClass,
        redacted_reason: impl Into<String>,
        now: OffsetDateTime,
    ) -> MedicalApprovalRequest {
        self.status = MedicalActiveTurnStatus::WaitingForApproval;
        let request = MedicalApprovalRequest {
            approval_id: new_id().to_string(),
            turn_id: self.turn_id.clone(),
            class,
            redacted_reason: redacted_reason.into(),
        };
        self.pending_approvals.insert(
            request.approval_id.clone(),
            MedicalPendingApproval {
                request: request.clone(),
                requested_at: now,
            },
        );
        request
    }

    pub fn resolve_approval(
        &mut self,
        response: &MedicalApprovalResponse,
    ) -> Result<MedicalApprovalResolvedEvent, ActiveMedicalTurnError> {
        if response.turn_id != self.turn_id {
            return Err(ActiveMedicalTurnError::TurnMismatch {
                expected_turn_id: self.turn_id.clone(),
                actual_turn_id: response.turn_id.clone(),
            });
        }

        self.pending_approvals
            .remove(&response.approval_id)
            .ok_or_else(|| ActiveMedicalTurnError::ApprovalNotPending {
                approval_id: response.approval_id.clone(),
            })?;

        if self.pending_approvals.is_empty()
            && self.status == MedicalActiveTurnStatus::WaitingForApproval
        {
            self.status = MedicalActiveTurnStatus::Running;
        }

        Ok(MedicalApprovalResolvedEvent {
            approval_id: response.approval_id.clone(),
            turn_id: self.turn_id.clone(),
            decision: response.decision,
        })
    }

    pub fn tool_started(
        &mut self,
        call_id: impl Into<String>,
        tool_name: MedicalToolName,
        redacted_summary: impl Into<String>,
    ) -> MedicalToolLifecycleEvent {
        let call_id = call_id.into();
        let event = MedicalToolLifecycleEvent {
            turn_id: self.turn_id.clone(),
            call_id: call_id.clone(),
            tool_name: tool_name.clone(),
            redacted_summary: redacted_summary.into(),
        };
        self.pending_tools.insert(
            call_id,
            MedicalPendingToolCall {
                tool_name,
                started_at: OffsetDateTime::now_utc(),
            },
        );
        event
    }

    pub fn tool_finished(
        &mut self,
        call_id: &str,
        redacted_summary: impl Into<String>,
    ) -> Result<MedicalToolLifecycleEvent, ActiveMedicalTurnError> {
        let pending = self.pending_tools.remove(call_id).ok_or_else(|| {
            ActiveMedicalTurnError::ToolCallNotPending {
                call_id: call_id.to_owned(),
            }
        })?;

        Ok(MedicalToolLifecycleEvent {
            turn_id: self.turn_id.clone(),
            call_id: call_id.to_owned(),
            tool_name: pending.tool_name,
            redacted_summary: redacted_summary.into(),
        })
    }

    pub fn cancel(&mut self, redacted_summary: impl Into<String>) -> MedicalTurnAbortedEvent {
        self.cancellation_requested = true;
        self.status = MedicalActiveTurnStatus::Cancelled;
        self.pending_approvals.clear();
        self.pending_tools.clear();
        MedicalTurnAbortedEvent {
            turn_id: self.turn_id.clone(),
            reason: MedicalTurnAbortReason::UserCancelled,
            redacted_summary: redacted_summary.into(),
        }
    }

    pub fn abort(
        &mut self,
        reason: MedicalTurnAbortReason,
        redacted_summary: impl Into<String>,
    ) -> MedicalTurnAbortedEvent {
        self.status = MedicalActiveTurnStatus::Aborted;
        self.pending_approvals.clear();
        self.pending_tools.clear();
        MedicalTurnAbortedEvent {
            turn_id: self.turn_id.clone(),
            reason,
            redacted_summary: redacted_summary.into(),
        }
    }

    pub fn complete(&mut self, redacted_summary: impl Into<String>) -> MedicalTurnCompleteEvent {
        self.status = MedicalActiveTurnStatus::Complete;
        self.pending_approvals.clear();
        self.pending_tools.clear();
        MedicalTurnCompleteEvent {
            turn_id: self.turn_id.clone(),
            steps_completed: self.steps_completed,
            redacted_summary: redacted_summary.into(),
        }
    }

    pub fn pending_approval_count(&self) -> usize {
        self.pending_approvals.len()
    }

    pub fn pending_tool_count(&self) -> usize {
        self.pending_tools.len()
    }

    fn ensure_not_cancelled(&self) -> Result<(), ActiveMedicalTurnError> {
        if self.cancellation_requested {
            return Err(ActiveMedicalTurnError::TurnCancelled {
                turn_id: self.turn_id.clone(),
            });
        }

        Ok(())
    }

    fn ensure_within_wall_clock_limit(
        &mut self,
        now: OffsetDateTime,
    ) -> Result<(), ActiveMedicalTurnError> {
        let elapsed = now - self.started_at;
        if elapsed.whole_seconds() > self.loop_limits.max_wall_clock_seconds as i64 {
            self.status = MedicalActiveTurnStatus::Aborted;
            return Err(ActiveMedicalTurnError::WallClockLimitExceeded {
                turn_id: self.turn_id.clone(),
                max_wall_clock_seconds: self.loop_limits.max_wall_clock_seconds,
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MedicalActiveTurnStatus {
    Running,
    WaitingForApproval,
    Cancelling,
    Cancelled,
    Complete,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalPendingApproval {
    pub request: MedicalApprovalRequest,
    pub requested_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalPendingToolCall {
    pub tool_name: MedicalToolName,
    pub started_at: OffsetDateTime,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActiveMedicalTurnError {
    #[error("turn {turn_id} exceeded maximum step count of {max_steps}")]
    LoopStepLimitExceeded { turn_id: String, max_steps: u32 },

    #[error("turn {turn_id} exceeded maximum wall-clock seconds of {max_wall_clock_seconds}")]
    WallClockLimitExceeded {
        turn_id: String,
        max_wall_clock_seconds: u64,
    },

    #[error("turn has already been cancelled: {turn_id}")]
    TurnCancelled { turn_id: String },

    #[error("approval is not pending: {approval_id}")]
    ApprovalNotPending { approval_id: String },

    #[error("approval response turn mismatch; expected {expected_turn_id}, got {actual_turn_id}")]
    TurnMismatch {
        expected_turn_id: String,
        actual_turn_id: String,
    },

    #[error("tool call is not pending: {call_id}")]
    ToolCallNotPending { call_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MedicalReviewDecision, ProviderBaaStatus};

    fn request() -> MedicalTurnRequest {
        MedicalTurnRequest {
            instruction: "Synthetic local documentation loop".to_owned(),
            patient_id: None,
            encounter_id: None,
            note_id: None,
            contains_phi: false,
            requested_tools: vec![MedicalToolName::ReadPatientSummary],
            provider: Some(crate::MedicalProviderUse {
                provider_id: "openai".to_owned(),
                service_name: "responses".to_owned(),
                baa_status: ProviderBaaStatus::NotRequested,
            }),
            loop_limits: MedicalLoopLimits {
                max_steps: 2,
                max_wall_clock_seconds: 60,
            },
        }
    }

    #[test]
    fn records_steps_until_limit() {
        let now = OffsetDateTime::now_utc();
        let mut turn = ActiveMedicalTurn::new("turn-1", &request(), now);

        let first = turn.record_step(now, "First synthetic step").unwrap();
        let second = turn.record_step(now, "Second synthetic step").unwrap();
        let result = turn.record_step(now, "Third synthetic step");

        assert_eq!(first.step_index, 1);
        assert_eq!(second.step_index, 2);
        assert!(matches!(
            result,
            Err(ActiveMedicalTurnError::LoopStepLimitExceeded { .. })
        ));
    }

    #[test]
    fn tracks_pending_approval_resolution() {
        let now = OffsetDateTime::now_utc();
        let mut turn = ActiveMedicalTurn::new("turn-1", &request(), now);

        let approval = turn.request_approval(
            MedicalApprovalClass::DesktopAutomation,
            "Synthetic approval",
            now,
        );
        assert_eq!(turn.status, MedicalActiveTurnStatus::WaitingForApproval);
        assert_eq!(turn.pending_approval_count(), 1);

        let resolved = turn
            .resolve_approval(&MedicalApprovalResponse {
                approval_id: approval.approval_id.clone(),
                turn_id: approval.turn_id.clone(),
                decided_by: "local-user".to_owned(),
                decision: MedicalReviewDecision::Approved,
                redacted_reason: None,
            })
            .unwrap();

        assert_eq!(resolved.approval_id, approval.approval_id);
        assert_eq!(turn.status, MedicalActiveTurnStatus::Running);
        assert_eq!(turn.pending_approval_count(), 0);
    }

    #[test]
    fn clears_pending_state_on_cancel() {
        let now = OffsetDateTime::now_utc();
        let mut turn = ActiveMedicalTurn::new("turn-1", &request(), now);

        turn.request_approval(
            MedicalApprovalClass::DesktopAutomation,
            "Synthetic approval",
            now,
        );
        turn.tool_started(
            "call-1",
            MedicalToolName::ReadPatientSummary,
            "Synthetic tool start",
        );
        let aborted = turn.cancel("Synthetic cancellation");

        assert_eq!(aborted.reason, MedicalTurnAbortReason::UserCancelled);
        assert_eq!(turn.status, MedicalActiveTurnStatus::Cancelled);
        assert_eq!(turn.pending_approval_count(), 0);
        assert_eq!(turn.pending_tool_count(), 0);
    }
}
