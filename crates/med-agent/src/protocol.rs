// Derived from OpenAI Codex CLI.
//
// Original source:
// - https://github.com/openai/codex
// - codex-rs/protocol/src/protocol.rs
// - upstream snapshot commit: bf72be59278e23002a352a53207182985cabb9d0
//
// Upstream license: Apache-2.0. See this repository's NOTICE and
// docs/CODEX_EXTRACTION_LOG.md for attribution and modification notes.
//
// Modifications by Flekks EMR TUI contributors:
// - replaced Codex coding-agent operations with local medical workflow operations
// - removed shell, patch, MCP, realtime, telemetry, and coding-workspace payloads
// - added patient/encounter/note context, BAA state, PHI policy, and loop limits
// - kept the Codex SQ/EQ protocol shape: submissions in, structured events out

use med_core::{EncounterId, NoteId, PatientId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::MedicalToolName;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalSubmission {
    pub id: String,
    pub op: MedicalOp,
    pub submitted_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MedicalOp {
    StartTurn(MedicalTurnRequest),
    SaveNoteDraft {
        patient_id: PatientId,
        encounter_id: EncounterId,
        note_id: Option<NoteId>,
    },
    SignNote {
        patient_id: PatientId,
        encounter_id: EncounterId,
        note_id: NoteId,
    },
    RunLocalAudit {
        patient_id: PatientId,
        encounter_id: Option<EncounterId>,
        note_id: Option<NoteId>,
    },
    ApproveAction(MedicalApprovalResponse),
    DenyAction(MedicalApprovalResponse),
    CancelTurn {
        turn_id: String,
        reason: String,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalTurnRequest {
    pub instruction: String,
    pub patient_id: Option<PatientId>,
    pub encounter_id: Option<EncounterId>,
    pub note_id: Option<NoteId>,
    pub contains_phi: bool,
    pub requested_tools: Vec<MedicalToolName>,
    pub provider: Option<MedicalProviderUse>,
    pub loop_limits: MedicalLoopLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalProviderUse {
    pub provider_id: String,
    pub service_name: String,
    pub baa_status: ProviderBaaStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderBaaStatus {
    NotRequested,
    Missing,
    ExecutedAndApproved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalLoopLimits {
    pub max_steps: u32,
    pub max_wall_clock_seconds: u64,
}

impl Default for MedicalLoopLimits {
    fn default() -> Self {
        Self {
            max_steps: 8,
            max_wall_clock_seconds: 120,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalEvent {
    pub id: String,
    pub submission_id: Option<String>,
    pub occurred_at: OffsetDateTime,
    pub msg: MedicalEventMsg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MedicalEventMsg {
    SessionConfigured(MedicalSessionConfiguredEvent),
    TurnStarted(MedicalTurnStartedEvent),
    TurnStepStarted(MedicalTurnStepEvent),
    ToolStarted(MedicalToolLifecycleEvent),
    ToolFinished(MedicalToolLifecycleEvent),
    ApprovalRequested(MedicalApprovalRequest),
    ApprovalResolved(MedicalApprovalResolvedEvent),
    TurnComplete(MedicalTurnCompleteEvent),
    TurnAborted(MedicalTurnAbortedEvent),
    PolicyBlocked(MedicalPolicyBlockedEvent),
    Error(MedicalAgentErrorEvent),
    ShutdownComplete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalSessionConfiguredEvent {
    pub thread_id: String,
    pub local_storage_only: bool,
    pub default_loop_limits: MedicalLoopLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalTurnStartedEvent {
    pub turn_id: String,
    pub patient_id: Option<PatientId>,
    pub encounter_id: Option<EncounterId>,
    pub note_id: Option<NoteId>,
    pub contains_phi: bool,
    pub loop_limits: MedicalLoopLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalTurnStepEvent {
    pub turn_id: String,
    pub step_index: u32,
    pub redacted_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalToolLifecycleEvent {
    pub turn_id: String,
    pub call_id: String,
    pub tool_name: MedicalToolName,
    pub redacted_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalApprovalRequest {
    pub approval_id: String,
    pub turn_id: String,
    pub class: MedicalApprovalClass,
    pub redacted_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalApprovalResponse {
    pub approval_id: String,
    pub turn_id: String,
    pub decided_by: String,
    pub decision: MedicalReviewDecision,
    pub redacted_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalApprovalResolvedEvent {
    pub approval_id: String,
    pub turn_id: String,
    pub decision: MedicalReviewDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MedicalApprovalClass {
    OutboundPhi,
    SignedClinicalChange,
    BillingSupportExport,
    DestructiveLocalWrite,
    DesktopAutomation,
    BulkImport,
    BulkExport,
    PluginInstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MedicalReviewDecision {
    Approved,
    ApprovedForTurn,
    Denied,
    AbortTurn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalTurnCompleteEvent {
    pub turn_id: String,
    pub steps_completed: u32,
    pub redacted_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalTurnAbortedEvent {
    pub turn_id: String,
    pub reason: MedicalTurnAbortReason,
    pub redacted_summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MedicalTurnAbortReason {
    UserCancelled,
    ReplacedByNewTurn,
    PolicyBlocked,
    LoopLimitExceeded,
    RuntimeError,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalPolicyBlockedEvent {
    pub turn_id: Option<String>,
    pub policy: MedicalPolicyBlock,
    pub redacted_summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MedicalPolicyBlock {
    MissingBaaForPhi,
    HumanConfirmationRequired,
    ToolNotAllowed,
    LoopLimitInvalid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalAgentErrorEvent {
    pub redacted_message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MedicalAgentStatus {
    Idle,
    Running,
    WaitingForApproval,
    Cancelling,
    ShuttingDown,
    Stopped,
}
