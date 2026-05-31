// Derived from OpenAI Codex CLI.
//
// Original source:
// - https://github.com/openai/codex
// - codex-rs/core/src/codex_thread.rs
// - codex-rs/core/src/session/mod.rs
// - codex-rs/core/src/session/handlers.rs
// - upstream snapshot commit: bf72be59278e23002a352a53207182985cabb9d0
//
// Upstream license: Apache-2.0. See this repository's NOTICE and
// docs/CODEX_EXTRACTION_LOG.md for attribution and modification notes.
//
// Modifications by Flekks EMR TUI contributors:
// - converted the async Codex thread conduit into a small std-channel medical
//   thread runtime that can be wired into the Ratatui dashboard later
// - removed coding-agent config, shell execution, MCP, telemetry, and cloud task
//   behavior
// - added medical loop limits, PHI/BAA policy blocking, local-only session
//   configured events, cancellation, and shutdown events

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use med_core::new_id;
use thiserror::Error;
use time::OffsetDateTime;

use crate::protocol::{
    MedicalAgentErrorEvent, MedicalAgentStatus, MedicalApprovalClass, MedicalEvent,
    MedicalEventMsg, MedicalLoopLimits, MedicalOp, MedicalPolicyBlock, MedicalPolicyBlockedEvent,
    MedicalProviderUse, MedicalReviewDecision, MedicalSessionConfiguredEvent, MedicalSubmission,
    MedicalTurnAbortReason, MedicalTurnAbortedEvent, MedicalTurnRequest, ProviderBaaStatus,
};
use crate::turn::{ActiveMedicalTurn, ActiveMedicalTurnError};
use crate::MedicalToolName;

#[derive(Debug, Clone)]
pub struct MedicalAgentThreadConfig {
    pub thread_id: String,
    pub default_loop_limits: MedicalLoopLimits,
    pub submission_queue_bound: usize,
    pub event_queue_bound: usize,
}

impl Default for MedicalAgentThreadConfig {
    fn default() -> Self {
        Self {
            thread_id: new_id().to_string(),
            default_loop_limits: MedicalLoopLimits::default(),
            submission_queue_bound: 64,
            event_queue_bound: 256,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MedicalAgentConfigSnapshot {
    pub thread_id: String,
    pub local_storage_only: bool,
    pub default_loop_limits: MedicalLoopLimits,
}

pub struct MedicalAgentThread {
    config: MedicalAgentThreadConfig,
    tx_submission: SyncSender<MedicalSubmission>,
    rx_event: Arc<Mutex<Receiver<MedicalEvent>>>,
    status: Arc<Mutex<MedicalAgentStatus>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl MedicalAgentThread {
    pub fn spawn(config: MedicalAgentThreadConfig) -> Self {
        let (tx_submission, rx_submission) = mpsc::sync_channel(config.submission_queue_bound);
        let (tx_event, rx_event) = mpsc::sync_channel(config.event_queue_bound);
        let status = Arc::new(Mutex::new(MedicalAgentStatus::Idle));
        let worker_status = Arc::clone(&status);
        let worker_config = config.clone();
        let worker = thread::spawn(move || {
            MedicalSessionLoop::new(worker_config, rx_submission, tx_event, worker_status).run();
        });

        Self {
            config,
            tx_submission,
            rx_event: Arc::new(Mutex::new(rx_event)),
            status,
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn submit(&self, op: MedicalOp) -> Result<String, MedicalAgentThreadError> {
        let submission = MedicalSubmission {
            id: new_id().to_string(),
            op,
            submitted_at: OffsetDateTime::now_utc(),
        };
        let submission_id = submission.id.clone();
        self.tx_submission
            .send(submission)
            .map_err(|_| MedicalAgentThreadError::SubmissionChannelClosed)?;
        Ok(submission_id)
    }

    pub fn next_event(&self) -> Result<MedicalEvent, MedicalAgentThreadError> {
        self.rx_event
            .lock()
            .map_err(|_| MedicalAgentThreadError::EventLockPoisoned)?
            .recv()
            .map_err(|_| MedicalAgentThreadError::EventChannelClosed)
    }

    pub fn next_event_timeout(
        &self,
        timeout: Duration,
    ) -> Result<Option<MedicalEvent>, MedicalAgentThreadError> {
        match self
            .rx_event
            .lock()
            .map_err(|_| MedicalAgentThreadError::EventLockPoisoned)?
            .recv_timeout(timeout)
        {
            Ok(event) => Ok(Some(event)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(MedicalAgentThreadError::EventChannelClosed),
        }
    }

    pub fn status(&self) -> MedicalAgentStatus {
        self.status
            .lock()
            .map(|status| *status)
            .unwrap_or(MedicalAgentStatus::Stopped)
    }

    pub fn config_snapshot(&self) -> MedicalAgentConfigSnapshot {
        MedicalAgentConfigSnapshot {
            thread_id: self.config.thread_id.clone(),
            local_storage_only: true,
            default_loop_limits: self.config.default_loop_limits,
        }
    }

    pub fn shutdown_and_wait(&self) -> Result<(), MedicalAgentThreadError> {
        let _ = self.submit(MedicalOp::Shutdown);
        let Some(worker) = self
            .worker
            .lock()
            .map_err(|_| MedicalAgentThreadError::WorkerLockPoisoned)?
            .take()
        else {
            return Ok(());
        };

        worker
            .join()
            .map_err(|_| MedicalAgentThreadError::WorkerPanicked)
    }
}

impl Drop for MedicalAgentThread {
    fn drop(&mut self) {
        let _ = self.tx_submission.send(MedicalSubmission {
            id: new_id().to_string(),
            op: MedicalOp::Shutdown,
            submitted_at: OffsetDateTime::now_utc(),
        });

        if let Ok(mut worker) = self.worker.lock() {
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }
    }
}

struct MedicalSessionLoop {
    config: MedicalAgentThreadConfig,
    rx_submission: Receiver<MedicalSubmission>,
    tx_event: SyncSender<MedicalEvent>,
    status: Arc<Mutex<MedicalAgentStatus>>,
    active_turn: Option<ActiveMedicalTurn>,
}

impl MedicalSessionLoop {
    fn new(
        config: MedicalAgentThreadConfig,
        rx_submission: Receiver<MedicalSubmission>,
        tx_event: SyncSender<MedicalEvent>,
        status: Arc<Mutex<MedicalAgentStatus>>,
    ) -> Self {
        Self {
            config,
            rx_submission,
            tx_event,
            status,
            active_turn: None,
        }
    }

    fn run(mut self) {
        self.emit(
            None,
            MedicalEventMsg::SessionConfigured(MedicalSessionConfiguredEvent {
                thread_id: self.config.thread_id.clone(),
                local_storage_only: true,
                default_loop_limits: self.config.default_loop_limits,
            }),
        );

        while let Ok(submission) = self.rx_submission.recv() {
            let should_stop = matches!(submission.op, MedicalOp::Shutdown);
            self.handle_submission(submission);
            if should_stop {
                break;
            }
        }

        self.set_status(MedicalAgentStatus::Stopped);
    }

    fn handle_submission(&mut self, submission: MedicalSubmission) {
        match submission.op {
            MedicalOp::StartTurn(request) => self.start_turn(submission.id, request),
            MedicalOp::CancelTurn { turn_id, reason } => {
                self.cancel_turn(submission.id, turn_id, reason);
            }
            MedicalOp::Shutdown => {
                self.set_status(MedicalAgentStatus::ShuttingDown);
                if let Some(mut turn) = self.active_turn.take() {
                    self.emit(
                        Some(submission.id.clone()),
                        MedicalEventMsg::TurnAborted(turn.abort(
                            MedicalTurnAbortReason::Shutdown,
                            "Turn stopped during local agent shutdown",
                        )),
                    );
                }
                self.emit(Some(submission.id), MedicalEventMsg::ShutdownComplete);
            }
            MedicalOp::ApproveAction(response) | MedicalOp::DenyAction(response) => {
                self.resolve_approval(submission.id, response);
            }
            MedicalOp::SaveNoteDraft { .. } | MedicalOp::RunLocalAudit { .. } => {
                self.emit(
                    Some(submission.id),
                    MedicalEventMsg::Error(MedicalAgentErrorEvent {
                        redacted_message:
                            "Direct medical operation dispatch is not wired to the session loop yet"
                                .to_owned(),
                    }),
                );
            }
        }
    }

    fn start_turn(&mut self, submission_id: String, request: MedicalTurnRequest) {
        let turn_id = new_id().to_string();

        if let Some(mut active_turn) = self.active_turn.take() {
            self.emit(
                Some(submission_id.clone()),
                MedicalEventMsg::TurnAborted(active_turn.abort(
                    MedicalTurnAbortReason::ReplacedByNewTurn,
                    "Turn replaced by a newer local agent turn",
                )),
            );
        }

        if let Err(policy) = validate_turn_request(&request) {
            self.set_status(MedicalAgentStatus::Idle);
            self.emit(
                Some(submission_id),
                MedicalEventMsg::PolicyBlocked(MedicalPolicyBlockedEvent {
                    turn_id: Some(turn_id),
                    policy,
                    redacted_summary: policy_block_summary(policy).to_owned(),
                }),
            );
            return;
        }

        let mut turn = ActiveMedicalTurn::new(turn_id, &request, OffsetDateTime::now_utc());
        self.set_status(MedicalAgentStatus::Running);
        self.emit(
            Some(submission_id.clone()),
            MedicalEventMsg::TurnStarted(turn.started_event()),
        );

        if let Some(approval_class) = turn_approval_class(&request) {
            let approval = turn.request_approval(
                approval_class,
                approval_reason(approval_class),
                OffsetDateTime::now_utc(),
            );
            self.active_turn = Some(turn);
            self.set_status(MedicalAgentStatus::WaitingForApproval);
            self.emit(
                Some(submission_id),
                MedicalEventMsg::ApprovalRequested(approval),
            );
            return;
        }

        self.active_turn = Some(turn);
        self.complete_active_turn(
            Some(submission_id),
            "Local medical agent turn accepted; model/tool execution is not attached yet",
        );
    }

    fn resolve_approval(
        &mut self,
        submission_id: String,
        response: crate::MedicalApprovalResponse,
    ) {
        let Some(turn) = self.active_turn.as_mut() else {
            self.emit(
                Some(submission_id),
                MedicalEventMsg::Error(MedicalAgentErrorEvent {
                    redacted_message: "No active medical turn is waiting for approval".to_owned(),
                }),
            );
            return;
        };

        let decision = response.decision;
        match turn.resolve_approval(&response) {
            Ok(event) => {
                self.emit(
                    Some(submission_id.clone()),
                    MedicalEventMsg::ApprovalResolved(event),
                );
            }
            Err(error) => {
                self.emit(
                    Some(submission_id),
                    MedicalEventMsg::Error(MedicalAgentErrorEvent {
                        redacted_message: active_turn_error_summary(&error).to_owned(),
                    }),
                );
                return;
            }
        }

        match decision {
            MedicalReviewDecision::Approved | MedicalReviewDecision::ApprovedForTurn => {
                self.set_status(MedicalAgentStatus::Running);
                self.complete_active_turn(
                    Some(submission_id),
                    "Approved medical agent action; model/tool execution is not attached yet",
                );
            }
            MedicalReviewDecision::Denied | MedicalReviewDecision::AbortTurn => {
                self.abort_active_turn(
                    Some(submission_id),
                    MedicalTurnAbortReason::PolicyBlocked,
                    "Human denied the pending medical agent action",
                );
            }
        }
    }

    fn complete_active_turn(&mut self, submission_id: Option<String>, redacted_summary: &str) {
        let Some(mut turn) = self.active_turn.take() else {
            return;
        };
        self.emit(
            submission_id,
            MedicalEventMsg::TurnComplete(turn.complete(redacted_summary)),
        );
        self.set_status(MedicalAgentStatus::Idle);
    }

    fn abort_active_turn(
        &mut self,
        submission_id: Option<String>,
        reason: MedicalTurnAbortReason,
        redacted_summary: &str,
    ) {
        let Some(mut turn) = self.active_turn.take() else {
            return;
        };
        self.emit(
            submission_id,
            MedicalEventMsg::TurnAborted(turn.abort(reason, redacted_summary)),
        );
        self.set_status(MedicalAgentStatus::Idle);
    }

    fn cancel_turn(&mut self, submission_id: String, turn_id: String, reason: String) {
        self.set_status(MedicalAgentStatus::Cancelling);
        if let Some(mut turn) = self.active_turn.take() {
            if turn.turn_id == turn_id {
                self.emit(
                    Some(submission_id),
                    MedicalEventMsg::TurnAborted(turn.cancel(reason)),
                );
                self.set_status(MedicalAgentStatus::Idle);
                return;
            }

            self.active_turn = Some(turn);
        }

        self.emit(
            Some(submission_id),
            MedicalEventMsg::TurnAborted(MedicalTurnAbortedEvent {
                turn_id,
                reason: MedicalTurnAbortReason::UserCancelled,
                redacted_summary: reason,
            }),
        );
        self.set_status(MedicalAgentStatus::Idle);
    }

    fn emit(&self, submission_id: Option<String>, msg: MedicalEventMsg) {
        let _ = self.tx_event.send(MedicalEvent {
            id: new_id().to_string(),
            submission_id,
            occurred_at: OffsetDateTime::now_utc(),
            msg,
        });
    }

    fn set_status(&self, status: MedicalAgentStatus) {
        if let Ok(mut current) = self.status.lock() {
            *current = status;
        }
    }
}

fn validate_turn_request(request: &MedicalTurnRequest) -> Result<(), MedicalPolicyBlock> {
    if request.loop_limits.max_steps == 0 || request.loop_limits.max_wall_clock_seconds == 0 {
        return Err(MedicalPolicyBlock::LoopLimitInvalid);
    }

    if request.contains_phi && !provider_has_baa(request.provider.as_ref()) {
        return Err(MedicalPolicyBlock::MissingBaaForPhi);
    }

    Ok(())
}

fn provider_has_baa(provider: Option<&MedicalProviderUse>) -> bool {
    match provider {
        Some(provider) => provider.baa_status == ProviderBaaStatus::ExecutedAndApproved,
        None => true,
    }
}

fn policy_block_summary(policy: MedicalPolicyBlock) -> &'static str {
    match policy {
        MedicalPolicyBlock::MissingBaaForPhi => {
            "PHI model request blocked because no executed, approved BAA is available"
        }
        MedicalPolicyBlock::HumanConfirmationRequired => {
            "Human confirmation is required before this action"
        }
        MedicalPolicyBlock::ToolNotAllowed => "Requested tool is not allowed by local policy",
        MedicalPolicyBlock::LoopLimitInvalid => "Agent loop limits must be nonzero",
    }
}

fn turn_approval_class(request: &MedicalTurnRequest) -> Option<MedicalApprovalClass> {
    if request.provider.is_some() && request.contains_phi {
        return Some(MedicalApprovalClass::OutboundPhi);
    }

    request.requested_tools.iter().find_map(|tool| match tool {
        MedicalToolName::ObserveDesktopTarget
        | MedicalToolName::ProposeDesktopAction
        | MedicalToolName::VerifyDesktopState => Some(MedicalApprovalClass::DesktopAutomation),
        MedicalToolName::PrepareSuperbillDraft => Some(MedicalApprovalClass::BillingSupportExport),
        _ => None,
    })
}

fn approval_reason(class: MedicalApprovalClass) -> &'static str {
    match class {
        MedicalApprovalClass::OutboundPhi => {
            "Outbound PHI requires an executed BAA and human approval"
        }
        MedicalApprovalClass::DesktopAutomation => {
            "Local desktop automation requires human approval"
        }
        MedicalApprovalClass::BillingSupportExport => {
            "Billing-support export or finalization requires human review"
        }
        MedicalApprovalClass::SignedClinicalChange => {
            "Signing or finalizing a clinical note requires human confirmation"
        }
        MedicalApprovalClass::DestructiveLocalWrite => {
            "Destructive local writes require human confirmation"
        }
        MedicalApprovalClass::BulkImport => "Bulk import requires human confirmation",
        MedicalApprovalClass::BulkExport => "Bulk export requires human confirmation",
        MedicalApprovalClass::PluginInstall => "Plugin installation requires human confirmation",
    }
}

fn active_turn_error_summary(error: &ActiveMedicalTurnError) -> &'static str {
    match error {
        ActiveMedicalTurnError::ApprovalNotPending { .. } => {
            "Approval response did not match a pending approval"
        }
        ActiveMedicalTurnError::TurnMismatch { .. } => {
            "Approval response did not match the active turn"
        }
        ActiveMedicalTurnError::LoopStepLimitExceeded { .. } => {
            "Medical agent turn exceeded its step limit"
        }
        ActiveMedicalTurnError::WallClockLimitExceeded { .. } => {
            "Medical agent turn exceeded its wall-clock limit"
        }
        ActiveMedicalTurnError::TurnCancelled { .. } => "Medical agent turn was already cancelled",
        ActiveMedicalTurnError::ToolCallNotPending { .. } => {
            "Tool result did not match a pending tool call"
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MedicalAgentThreadError {
    #[error("medical agent submission channel is closed")]
    SubmissionChannelClosed,

    #[error("medical agent event channel is closed")]
    EventChannelClosed,

    #[error("medical agent event lock is poisoned")]
    EventLockPoisoned,

    #[error("medical agent worker lock is poisoned")]
    WorkerLockPoisoned,

    #[error("medical agent worker panicked")]
    WorkerPanicked,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MedicalProviderUse, ProviderBaaStatus};

    fn start_turn(contains_phi: bool) -> MedicalOp {
        MedicalOp::StartTurn(MedicalTurnRequest {
            instruction: "Draft a local documentation plan".to_owned(),
            patient_id: None,
            encounter_id: None,
            note_id: None,
            contains_phi,
            requested_tools: Vec::new(),
            provider: None,
            loop_limits: MedicalLoopLimits::default(),
        })
    }

    fn desktop_turn() -> MedicalOp {
        MedicalOp::StartTurn(MedicalTurnRequest {
            instruction: "Propose the next local desktop action".to_owned(),
            patient_id: None,
            encounter_id: None,
            note_id: None,
            contains_phi: false,
            requested_tools: vec![MedicalToolName::ProposeDesktopAction],
            provider: None,
            loop_limits: MedicalLoopLimits::default(),
        })
    }

    #[test]
    fn emits_configured_event_on_spawn() {
        let thread = MedicalAgentThread::spawn(MedicalAgentThreadConfig::default());

        let event = thread
            .next_event_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();

        assert!(matches!(
            event.msg,
            MedicalEventMsg::SessionConfigured(MedicalSessionConfiguredEvent {
                local_storage_only: true,
                ..
            })
        ));

        thread.shutdown_and_wait().unwrap();
    }

    #[test]
    fn accepts_non_phi_turn_and_emits_turn_lifecycle() {
        let thread = MedicalAgentThread::spawn(MedicalAgentThreadConfig::default());
        let _ = thread.next_event().unwrap();
        let submission_id = thread.submit(start_turn(false)).unwrap();

        let started = thread.next_event().unwrap();
        let complete = thread.next_event().unwrap();

        assert_eq!(
            started.submission_id.as_deref(),
            Some(submission_id.as_str())
        );
        assert!(matches!(started.msg, MedicalEventMsg::TurnStarted(_)));
        assert!(matches!(complete.msg, MedicalEventMsg::TurnComplete(_)));
        assert_eq!(thread.status(), MedicalAgentStatus::Idle);

        thread.shutdown_and_wait().unwrap();
    }

    #[test]
    fn blocks_phi_provider_turn_without_baa() {
        let thread = MedicalAgentThread::spawn(MedicalAgentThreadConfig::default());
        let _ = thread.next_event().unwrap();
        thread
            .submit(MedicalOp::StartTurn(MedicalTurnRequest {
                instruction: "Draft a local documentation plan".to_owned(),
                patient_id: None,
                encounter_id: None,
                note_id: None,
                contains_phi: true,
                requested_tools: Vec::new(),
                provider: Some(MedicalProviderUse {
                    provider_id: "openai".to_owned(),
                    service_name: "responses".to_owned(),
                    baa_status: ProviderBaaStatus::Missing,
                }),
                loop_limits: MedicalLoopLimits::default(),
            }))
            .unwrap();

        let blocked = thread.next_event().unwrap();

        assert!(matches!(
            blocked.msg,
            MedicalEventMsg::PolicyBlocked(MedicalPolicyBlockedEvent {
                policy: MedicalPolicyBlock::MissingBaaForPhi,
                ..
            })
        ));

        thread.shutdown_and_wait().unwrap();
    }

    #[test]
    fn permits_phi_turn_with_executed_baa_provider() {
        let thread = MedicalAgentThread::spawn(MedicalAgentThreadConfig::default());
        let _ = thread.next_event().unwrap();
        thread
            .submit(MedicalOp::StartTurn(MedicalTurnRequest {
                instruction: "Draft a local documentation plan".to_owned(),
                patient_id: None,
                encounter_id: None,
                note_id: None,
                contains_phi: true,
                requested_tools: Vec::new(),
                provider: Some(MedicalProviderUse {
                    provider_id: "openai".to_owned(),
                    service_name: "responses".to_owned(),
                    baa_status: ProviderBaaStatus::ExecutedAndApproved,
                }),
                loop_limits: MedicalLoopLimits::default(),
            }))
            .unwrap();

        let started = thread.next_event().unwrap();

        assert!(matches!(started.msg, MedicalEventMsg::TurnStarted(_)));

        thread.shutdown_and_wait().unwrap();
    }

    #[test]
    fn rejects_zero_loop_limits() {
        let thread = MedicalAgentThread::spawn(MedicalAgentThreadConfig::default());
        let _ = thread.next_event().unwrap();
        thread
            .submit(MedicalOp::StartTurn(MedicalTurnRequest {
                instruction: "Run without bounds".to_owned(),
                patient_id: None,
                encounter_id: None,
                note_id: None,
                contains_phi: false,
                requested_tools: Vec::new(),
                provider: None,
                loop_limits: MedicalLoopLimits {
                    max_steps: 0,
                    max_wall_clock_seconds: 120,
                },
            }))
            .unwrap();

        let blocked = thread.next_event().unwrap();

        assert!(matches!(
            blocked.msg,
            MedicalEventMsg::PolicyBlocked(MedicalPolicyBlockedEvent {
                policy: MedicalPolicyBlock::LoopLimitInvalid,
                ..
            })
        ));

        thread.shutdown_and_wait().unwrap();
    }

    #[test]
    fn desktop_turn_waits_for_approval_then_completes() {
        let thread = MedicalAgentThread::spawn(MedicalAgentThreadConfig::default());
        let _ = thread.next_event().unwrap();
        thread.submit(desktop_turn()).unwrap();

        let started = thread.next_event().unwrap();
        let approval = thread.next_event().unwrap();

        let MedicalEventMsg::TurnStarted(started) = started.msg else {
            panic!("expected turn started event");
        };
        let MedicalEventMsg::ApprovalRequested(approval) = approval.msg else {
            panic!("expected approval request event");
        };
        assert_eq!(approval.turn_id, started.turn_id);
        assert_eq!(approval.class, MedicalApprovalClass::DesktopAutomation);
        assert_eq!(thread.status(), MedicalAgentStatus::WaitingForApproval);

        thread
            .submit(MedicalOp::ApproveAction(crate::MedicalApprovalResponse {
                approval_id: approval.approval_id,
                turn_id: approval.turn_id,
                decided_by: "local-user".to_owned(),
                decision: MedicalReviewDecision::Approved,
                redacted_reason: None,
            }))
            .unwrap();

        let resolved = thread.next_event().unwrap();
        let complete = thread.next_event().unwrap();

        assert!(matches!(resolved.msg, MedicalEventMsg::ApprovalResolved(_)));
        assert!(matches!(complete.msg, MedicalEventMsg::TurnComplete(_)));
        assert_eq!(thread.status(), MedicalAgentStatus::Idle);

        thread.shutdown_and_wait().unwrap();
    }

    #[test]
    fn desktop_turn_denial_aborts_turn() {
        let thread = MedicalAgentThread::spawn(MedicalAgentThreadConfig::default());
        let _ = thread.next_event().unwrap();
        thread.submit(desktop_turn()).unwrap();
        let _ = thread.next_event().unwrap();
        let approval = thread.next_event().unwrap();

        let MedicalEventMsg::ApprovalRequested(approval) = approval.msg else {
            panic!("expected approval request event");
        };
        thread
            .submit(MedicalOp::DenyAction(crate::MedicalApprovalResponse {
                approval_id: approval.approval_id,
                turn_id: approval.turn_id,
                decided_by: "local-user".to_owned(),
                decision: MedicalReviewDecision::Denied,
                redacted_reason: Some("Synthetic denial".to_owned()),
            }))
            .unwrap();

        let resolved = thread.next_event().unwrap();
        let aborted = thread.next_event().unwrap();

        assert!(matches!(resolved.msg, MedicalEventMsg::ApprovalResolved(_)));
        assert!(matches!(
            aborted.msg,
            MedicalEventMsg::TurnAborted(MedicalTurnAbortedEvent {
                reason: MedicalTurnAbortReason::PolicyBlocked,
                ..
            })
        ));
        assert_eq!(thread.status(), MedicalAgentStatus::Idle);

        thread.shutdown_and_wait().unwrap();
    }
}
