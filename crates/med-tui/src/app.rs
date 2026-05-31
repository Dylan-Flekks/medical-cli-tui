use anyhow::{anyhow, Result};
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use med_agent::{
    MedicalAgentStatus, MedicalAgentThread, MedicalApprovalClass, MedicalApprovalPolicy,
    MedicalApprovalResponse, MedicalEvent, MedicalEventMsg, MedicalLoopLimits, MedicalOp,
    MedicalReviewDecision, MedicalToolContext, MedicalToolInvocation, MedicalToolName,
    MedicalToolPayload, MedicalToolRuntimeRegistry, MedicalTurnAbortReason, MedicalTurnRequest,
    PrepareSuperbillDraftRequest, SaveNoteDraftRequest, SignNoteRequest,
};
use med_core::{
    assess_claim_readiness, audit_documentation, new_id, ClaimDraft, ClaimReadinessFlag,
    ClaimReadinessSeverity, ClinicalNote, DiagnosisSystem, DocumentationAuditFlag,
    DocumentationAuditReport, DocumentationAuditSeverity, Encounter, EncounterId, EncounterStatus,
    EncounterType, NoteId, NoteSection, NoteStatus, NoteTemplate, Patient, PatientId,
    ProcedureSystem,
};
use med_store::LocalStore;
use time::{Date, OffsetDateTime};
use tui_textarea::{CursorMove, Input, Key, TextArea};

const SIGNING_ARMED_MESSAGE: &str = "Press S again to sign this note; any other key cancels";
const SIGNED_NOTE_LOCKED_MESSAGE: &str =
    "Signed note is immutable; future edits require an amendment flow";
const AGENT_EVENT_LIMIT: usize = 8;
const LOCAL_AGENT_USER: &str = "local-tui-user";

#[derive(Debug, Clone)]
pub struct App {
    pub focus: FocusArea,
    pub selected_patient: usize,
    pub selected_tab: WorkspaceTab,
    pub data: DashboardData,
    pub note_editor: TextArea<'static>,
    pub note_draft_id: Option<NoteId>,
    pub note_status: Option<String>,
    pub note_version: Option<u32>,
    pub note_updated_at: Option<String>,
    pub note_signed_at: Option<String>,
    pub note_signing_armed: bool,
    pub note_dirty: bool,
    pub agent: AgentPanelState,
    pub last_message: String,
    pub should_quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            focus: FocusArea::PatientQueue,
            selected_patient: 0,
            selected_tab: WorkspaceTab::Chart,
            data: DashboardData::empty(),
            note_editor: default_note_editor(),
            note_draft_id: None,
            note_status: None,
            note_version: None,
            note_updated_at: None,
            note_signed_at: None,
            note_signing_armed: false,
            note_dirty: false,
            agent: AgentPanelState::default(),
            last_message: "Local database not loaded".to_owned(),
            should_quit: false,
        }
    }
}

impl App {
    pub fn from_store(store: &LocalStore) -> Result<Self> {
        let mut app = Self::default();
        app.refresh_from_store(store)?;
        app.last_message = format!("Loaded {} local patients", app.data.patients.len());
        Ok(app)
    }

    #[cfg(test)]
    fn with_data(data: DashboardData) -> Self {
        Self {
            data,
            last_message: "Synthetic dashboard data loaded".to_owned(),
            ..Self::default()
        }
    }

    pub fn refresh_from_store(&mut self, store: &LocalStore) -> Result<()> {
        let preferred_patient_id = self.active_patient().map(|patient| patient.id);
        self.refresh_from_store_with_selection(store, preferred_patient_id)
    }

    #[cfg(test)]
    pub fn handle_key_with_store(&mut self, key: KeyEvent, store: &LocalStore) -> Result<()> {
        self.handle_key_with_store_and_agent(key, store, None)
    }

    pub fn handle_key_with_store_and_agent(
        &mut self,
        key: KeyEvent,
        store: &LocalStore,
        agent_thread: Option<&MedicalAgentThread>,
    ) -> Result<()> {
        if self.handle_agent_key(key, agent_thread)? {
            return Ok(());
        }

        if self.selected_tab == WorkspaceTab::Note && is_note_sign_key(key) {
            if self.note_signing_armed {
                self.sign_note(store)?;
            } else {
                self.arm_note_signing();
            }
            return Ok(());
        }

        self.cancel_note_signing_confirmation();

        if self.note_editor_active() && is_note_save_key(key) {
            self.save_note_draft(store)?;
            return Ok(());
        }

        if self.note_editor_active() && self.note_is_signed() && is_note_editor_input_key(key) {
            self.block_signed_note_edit();
            return Ok(());
        }

        if self.note_editor_active() && is_note_editor_input_key(key) {
            self.note_dirty |= self.note_editor.input(note_editor_input(key));
            return Ok(());
        }

        if self.selected_tab == WorkspaceTab::Billing && matches!(key.code, KeyCode::Char('b')) {
            self.prepare_superbill_draft(store)?;
            return Ok(());
        }

        match key.code {
            KeyCode::Char('r') => {
                self.refresh_from_store(store)?;
                self.last_message = "Refreshed local records".to_owned();
            }
            KeyCode::Char('n') => self.create_local_patient(store)?,
            KeyCode::Char('e') => self.create_encounter_for_selected_patient(store)?,
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('k') | KeyCode::Up => {
                self.handle_key(key);
                let selected_patient_id = self.active_patient().map(|patient| patient.id);
                self.refresh_from_store_with_selection(store, selected_patient_id)?;
                self.last_message = self.selection_message();
            }
            _ => self.handle_key(key),
        }

        Ok(())
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.note_editor_active() && is_note_editor_input_key(key) {
            self.note_dirty |= self.note_editor.input(note_editor_input(key));
            return;
        }

        if self.selected_tab == WorkspaceTab::Note && is_note_sign_key(key) {
            self.arm_note_signing();
            return;
        }

        self.cancel_note_signing_confirmation();

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab => self.focus_next(),
            KeyCode::BackTab => self.focus_previous(),
            KeyCode::Char('1') => self.selected_tab = WorkspaceTab::Chart,
            KeyCode::Char('2') => self.selected_tab = WorkspaceTab::Note,
            KeyCode::Char('3') => self.selected_tab = WorkspaceTab::Audit,
            KeyCode::Char('4') => self.selected_tab = WorkspaceTab::Billing,
            KeyCode::Char('j') | KeyCode::Down => self.select_next_patient(),
            KeyCode::Char('k') | KeyCode::Up => self.select_previous_patient(),
            _ => {}
        }
    }

    pub fn active_patient(&self) -> Option<&PatientQueueItem> {
        self.data.patients.get(self.selected_patient)
    }

    pub fn active_encounter(&self) -> Option<&EncounterItem> {
        self.data.encounters.first()
    }

    pub fn note_editor_active(&self) -> bool {
        self.selected_tab == WorkspaceTab::Note && self.focus == FocusArea::Workspace
    }

    pub fn note_is_signed(&self) -> bool {
        self.note_status.as_deref() == Some("Signed")
    }

    pub fn drain_agent_events(
        &mut self,
        agent_thread: &MedicalAgentThread,
        first_wait: Duration,
    ) -> Result<usize> {
        self.sync_agent_thread_status(agent_thread.status());

        let mut drained = 0;
        if let Some(event) = agent_thread.next_event_timeout(first_wait)? {
            self.apply_agent_event(event);
            drained += 1;
        }

        while let Some(event) = agent_thread.next_event_timeout(Duration::ZERO)? {
            self.apply_agent_event(event);
            drained += 1;
        }

        self.sync_agent_thread_status(agent_thread.status());
        Ok(drained)
    }

    fn handle_agent_key(
        &mut self,
        key: KeyEvent,
        agent_thread: Option<&MedicalAgentThread>,
    ) -> Result<bool> {
        match key.code {
            KeyCode::F(5) => {
                self.start_agent_turn(agent_thread)?;
                Ok(true)
            }
            KeyCode::F(6) => {
                self.approve_pending_agent_action(agent_thread)?;
                Ok(true)
            }
            KeyCode::F(7) => {
                self.deny_pending_agent_action(agent_thread)?;
                Ok(true)
            }
            KeyCode::F(8) => {
                self.cancel_agent_turn(agent_thread)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn start_agent_turn(&mut self, agent_thread: Option<&MedicalAgentThread>) -> Result<()> {
        let Some(agent_thread) = agent_thread else {
            self.last_message = "Agent thread is not available in this context".to_owned();
            return Ok(());
        };
        let Some(patient_id) = self.active_patient().map(|patient| patient.id) else {
            self.last_message = "Select a patient before starting an agent turn".to_owned();
            return Ok(());
        };

        let encounter_id = self.active_encounter().map(|encounter| encounter.id);
        let note_id = self.note_draft_id;
        let mut requested_tools = vec![
            MedicalToolName::ReadPatientSummary,
            MedicalToolName::RunDocumentationAudit,
        ];
        if self.selected_tab == WorkspaceTab::Note && note_id.is_some() && !self.note_is_signed() {
            requested_tools.push(MedicalToolName::SignNote);
        }

        agent_thread.submit(MedicalOp::StartTurn(MedicalTurnRequest {
            instruction: agent_turn_instruction(
                note_id,
                requested_tools.contains(&MedicalToolName::SignNote),
            ),
            patient_id: Some(patient_id),
            encounter_id,
            note_id,
            contains_phi: true,
            requested_tools,
            provider: None,
            loop_limits: MedicalLoopLimits::default(),
        }))?;

        self.agent.panel_status = AgentPanelStatus::Running;
        self.agent.record_event(
            "Submitted local bounded agent turn".to_owned(),
            Severity::Info,
        );
        self.last_message = "Submitted local bounded agent turn".to_owned();

        Ok(())
    }

    fn approve_pending_agent_action(
        &mut self,
        agent_thread: Option<&MedicalAgentThread>,
    ) -> Result<()> {
        let Some(agent_thread) = agent_thread else {
            self.last_message = "Agent thread is not available in this context".to_owned();
            return Ok(());
        };
        let Some(pending) = self.agent.pending_approval.clone() else {
            self.last_message = "No pending agent approval to approve".to_owned();
            return Ok(());
        };

        agent_thread.submit(MedicalOp::ApproveAction(MedicalApprovalResponse {
            approval_id: pending.approval_id,
            turn_id: pending.turn_id,
            decided_by: LOCAL_AGENT_USER.to_owned(),
            decision: MedicalReviewDecision::ApprovedForTurn,
            redacted_reason: Some("Approved in local TUI".to_owned()),
        }))?;
        self.last_message = "Approved pending agent action".to_owned();

        Ok(())
    }

    fn deny_pending_agent_action(
        &mut self,
        agent_thread: Option<&MedicalAgentThread>,
    ) -> Result<()> {
        let Some(agent_thread) = agent_thread else {
            self.last_message = "Agent thread is not available in this context".to_owned();
            return Ok(());
        };
        let Some(pending) = self.agent.pending_approval.clone() else {
            self.last_message = "No pending agent approval to deny".to_owned();
            return Ok(());
        };

        agent_thread.submit(MedicalOp::DenyAction(MedicalApprovalResponse {
            approval_id: pending.approval_id,
            turn_id: pending.turn_id,
            decided_by: LOCAL_AGENT_USER.to_owned(),
            decision: MedicalReviewDecision::Denied,
            redacted_reason: Some("Denied in local TUI".to_owned()),
        }))?;
        self.last_message = "Denied pending agent action".to_owned();

        Ok(())
    }

    fn cancel_agent_turn(&mut self, agent_thread: Option<&MedicalAgentThread>) -> Result<()> {
        let Some(agent_thread) = agent_thread else {
            self.last_message = "Agent thread is not available in this context".to_owned();
            return Ok(());
        };
        let turn_id = self.agent.active_turn_id.clone().or_else(|| {
            self.agent
                .pending_approval
                .as_ref()
                .map(|pending| pending.turn_id.clone())
        });
        let Some(turn_id) = turn_id else {
            self.last_message = "No active agent turn to cancel".to_owned();
            return Ok(());
        };

        agent_thread.submit(MedicalOp::CancelTurn {
            turn_id,
            reason: "Cancelled in local TUI".to_owned(),
        })?;
        self.agent.panel_status = AgentPanelStatus::Cancelling;
        self.last_message = "Cancelling active agent turn".to_owned();

        Ok(())
    }

    fn apply_agent_event(&mut self, event: MedicalEvent) {
        match event.msg {
            MedicalEventMsg::SessionConfigured(configured) => {
                self.agent.thread_status = MedicalAgentStatus::Idle;
                self.agent.loop_limits = configured.default_loop_limits;
                self.agent.record_event(
                    format!(
                        "Agent session configured: local-only, {} step limit",
                        configured.default_loop_limits.max_steps
                    ),
                    Severity::Info,
                );
            }
            MedicalEventMsg::TurnStarted(started) => {
                self.agent.panel_status = AgentPanelStatus::Running;
                self.agent.active_turn_id = Some(started.turn_id.clone());
                self.agent.patient_id = started.patient_id;
                self.agent.encounter_id = started.encounter_id;
                self.agent.note_id = started.note_id;
                self.agent.contains_phi = started.contains_phi;
                self.agent.loop_limits = started.loop_limits;
                self.agent.pending_approval = None;
                self.agent.last_error = None;
                self.agent.record_event(
                    format!("Turn {} started", short_text(&started.turn_id)),
                    Severity::Info,
                );
                self.last_message = "Agent turn started".to_owned();
            }
            MedicalEventMsg::TurnStepStarted(step) => {
                self.agent.record_event(
                    format!("Step {}: {}", step.step_index, step.redacted_summary),
                    Severity::Info,
                );
            }
            MedicalEventMsg::ToolStarted(tool) => {
                self.agent.record_event(
                    format!(
                        "Tool {} started: {}",
                        tool.tool_name.as_str(),
                        tool.redacted_summary
                    ),
                    Severity::Info,
                );
            }
            MedicalEventMsg::ToolFinished(tool) => {
                self.agent.record_event(
                    format!(
                        "Tool {} finished: {}",
                        tool.tool_name.as_str(),
                        tool.redacted_summary
                    ),
                    Severity::Info,
                );
            }
            MedicalEventMsg::ApprovalRequested(approval) => {
                self.agent.panel_status = AgentPanelStatus::WaitingForApproval;
                self.agent.active_turn_id = Some(approval.turn_id.clone());
                self.agent.pending_approval = Some(AgentPendingApproval {
                    approval_id: approval.approval_id.clone(),
                    turn_id: approval.turn_id,
                    class: approval.class,
                    redacted_reason: approval.redacted_reason.clone(),
                });
                self.agent.record_event(
                    format!(
                        "Approval requested: {}",
                        approval_class_label(approval.class)
                    ),
                    Severity::Warning,
                );
                self.last_message = format!(
                    "Agent waiting for {} approval",
                    approval_class_label(approval.class)
                );
            }
            MedicalEventMsg::ApprovalResolved(resolved) => {
                self.agent.pending_approval = None;
                self.agent.panel_status = AgentPanelStatus::Running;
                self.agent.record_event(
                    format!("Approval {}", review_decision_label(resolved.decision)),
                    Severity::Info,
                );
                self.last_message = "Agent approval resolved".to_owned();
            }
            MedicalEventMsg::TurnComplete(complete) => {
                self.agent.panel_status = AgentPanelStatus::Complete;
                self.agent.active_turn_id = None;
                self.agent.pending_approval = None;
                self.agent.record_event(
                    format!(
                        "Turn {} complete: {}",
                        short_text(&complete.turn_id),
                        complete.redacted_summary
                    ),
                    Severity::Info,
                );
                self.last_message = "Agent turn complete".to_owned();
            }
            MedicalEventMsg::TurnAborted(aborted) => {
                self.agent.panel_status = if aborted.reason == MedicalTurnAbortReason::UserCancelled
                {
                    AgentPanelStatus::Cancelled
                } else {
                    AgentPanelStatus::Aborted
                };
                self.agent.active_turn_id = None;
                self.agent.pending_approval = None;
                self.agent.last_error = Some(aborted.redacted_summary.clone());
                self.agent.record_event(
                    format!(
                        "Turn {} {}: {}",
                        short_text(&aborted.turn_id),
                        abort_reason_label(aborted.reason),
                        aborted.redacted_summary
                    ),
                    Severity::Warning,
                );
                self.last_message = "Agent turn stopped".to_owned();
            }
            MedicalEventMsg::PolicyBlocked(blocked) => {
                self.agent.panel_status = AgentPanelStatus::PolicyBlocked;
                self.agent.active_turn_id = blocked.turn_id;
                self.agent.pending_approval = None;
                self.agent.last_error = Some(blocked.redacted_summary.clone());
                self.agent
                    .record_event(blocked.redacted_summary.clone(), Severity::Blocked);
                self.last_message = "Agent policy block recorded".to_owned();
            }
            MedicalEventMsg::Error(error) => {
                self.agent.panel_status = AgentPanelStatus::Error;
                self.agent.last_error = Some(error.redacted_message.clone());
                self.agent
                    .record_event(error.redacted_message.clone(), Severity::Error);
                self.last_message = "Agent error recorded".to_owned();
            }
            MedicalEventMsg::ShutdownComplete => {
                self.agent.panel_status = AgentPanelStatus::Stopped;
                self.agent.active_turn_id = None;
                self.agent.pending_approval = None;
                self.agent
                    .record_event("Agent session stopped".to_owned(), Severity::Info);
            }
        }
    }

    fn sync_agent_thread_status(&mut self, status: MedicalAgentStatus) {
        self.agent.thread_status = status;
        if matches!(self.agent.panel_status, AgentPanelStatus::Idle)
            || matches!(
                status,
                MedicalAgentStatus::Running
                    | MedicalAgentStatus::WaitingForApproval
                    | MedicalAgentStatus::Cancelling
                    | MedicalAgentStatus::ShuttingDown
                    | MedicalAgentStatus::Stopped
            )
        {
            self.agent.panel_status = AgentPanelStatus::from_thread_status(status);
        }
    }

    fn refresh_from_store_with_selection(
        &mut self,
        store: &LocalStore,
        preferred_patient_id: Option<PatientId>,
    ) -> Result<()> {
        let patients = store.list_patients()?;
        let mut records = Vec::with_capacity(patients.len());

        for patient in patients {
            let encounters = store.list_encounters_for_patient(patient.id)?;
            records.push((patient, encounters));
        }

        self.selected_patient =
            selected_patient_index(&records, preferred_patient_id, self.selected_patient);
        self.data = DashboardData::from_local_records(&records, self.selected_patient);
        self.load_latest_note_draft(store)?;
        self.refresh_documentation_audit(store)?;

        Ok(())
    }

    fn create_local_patient(&mut self, store: &LocalStore) -> Result<()> {
        let id = new_id();
        let now = OffsetDateTime::now_utc();
        let id_text = id.to_string();
        let patient = Patient {
            id,
            medical_record_number: None,
            display_name: format!("New Local Patient {}", &id_text[..8]),
            date_of_birth: None,
            sex_at_birth: None,
            created_at: now,
            updated_at: now,
        };

        store.insert_patient(&patient)?;
        self.refresh_from_store_with_selection(store, Some(patient.id))?;
        self.last_message = format!("Created local patient {}", patient.display_name);

        Ok(())
    }

    fn create_encounter_for_selected_patient(&mut self, store: &LocalStore) -> Result<()> {
        let Some(patient_id) = self.active_patient().map(|patient| patient.id) else {
            self.last_message = "Create a patient before adding an encounter".to_owned();
            return Ok(());
        };

        let encounter = Encounter {
            id: new_id(),
            patient_id,
            practitioner_id: None,
            encounter_type: EncounterType::OfficeVisit,
            status: EncounterStatus::InProgress,
            started_at: OffsetDateTime::now_utc(),
            ended_at: None,
            reason: None,
        };

        store.insert_encounter(&encounter)?;
        self.refresh_from_store_with_selection(store, Some(patient_id))?;
        self.last_message = "Created local encounter".to_owned();

        Ok(())
    }

    fn save_note_draft(&mut self, store: &LocalStore) -> Result<()> {
        if self.note_is_signed() {
            self.block_signed_note_edit();
            return Ok(());
        }

        let Some(patient_id) = self.active_patient().map(|patient| patient.id) else {
            self.last_message = "Create or select a patient before saving a note".to_owned();
            return Ok(());
        };
        let Some(encounter_id) = self.active_encounter().map(|encounter| encounter.id) else {
            self.last_message = "Create an encounter before saving a note".to_owned();
            return Ok(());
        };

        let tool_name = if self.note_draft_id.is_some() {
            med_agent::MedicalToolName::UpdateNoteDraft
        } else {
            med_agent::MedicalToolName::CreateNoteDraft
        };
        let registry = MedicalToolRuntimeRegistry::default();
        let output = registry.dispatch(MedicalToolInvocation {
            store,
            call_id: format!("tui-note-save-{}", short_id(new_id())),
            tool_name,
            payload: MedicalToolPayload::SaveNoteDraft(SaveNoteDraftRequest {
                note_id: self.note_draft_id,
                patient_id,
                encounter_id,
                template: NoteTemplate::Soap,
                sections: note_sections_from_lines(self.note_editor.lines()),
            }),
            context: MedicalToolContext::default(),
            approval_policy: MedicalApprovalPolicy::local_default(),
        })?;

        let note_id = output
            .structured
            .get("note_id")
            .cloned()
            .ok_or_else(|| anyhow!("save note tool did not return note_id"))
            .and_then(|value| serde_json::from_value(value).map_err(Into::into))?;
        let note = store
            .get_note(note_id)?
            .ok_or_else(|| anyhow!("saved note draft {note_id} was not found"))?;
        self.apply_note_metadata(&note);
        self.refresh_documentation_audit(store)?;
        self.last_message = output.tui_summary;

        Ok(())
    }

    fn load_latest_note_draft(&mut self, store: &LocalStore) -> Result<()> {
        let Some(encounter_id) = self.active_encounter().map(|encounter| encounter.id) else {
            self.reset_note_editor();
            return Ok(());
        };

        if let Some(note) = store.latest_draft_note_for_encounter(encounter_id)? {
            self.note_editor = note_editor_from_sections(&note.sections);
            self.apply_note_metadata(&note);
        } else if let Some(note) = store
            .list_notes_for_encounter(encounter_id)?
            .into_iter()
            .next()
        {
            self.note_editor = note_editor_from_sections(&note.sections);
            self.apply_note_metadata(&note);
        } else {
            self.reset_note_editor();
        }

        Ok(())
    }

    fn apply_note_metadata(&mut self, note: &ClinicalNote) {
        self.note_draft_id = Some(note.id);
        self.note_status = Some(note_status_label(&note.status).to_owned());
        self.note_version = Some(note.version);
        self.note_updated_at = Some(note.updated_at.to_string());
        self.note_signed_at = note.signed_at.map(|signed_at| signed_at.to_string());
        self.note_signing_armed = false;
        self.note_dirty = false;
    }

    fn reset_note_editor(&mut self) {
        self.note_editor = default_note_editor();
        self.note_draft_id = None;
        self.note_status = None;
        self.note_version = None;
        self.note_updated_at = None;
        self.note_signed_at = None;
        self.note_signing_armed = false;
        self.note_dirty = false;
    }

    fn refresh_documentation_audit(&mut self, store: &LocalStore) -> Result<()> {
        let Some(patient_id) = self.active_patient().map(|patient| patient.id) else {
            self.data.audit_flags = vec![AuditFlagItem::info("No local chart selected")];
            self.data.billing_ready_percent = 0;
            return Ok(());
        };

        let encounter_id = self.active_encounter().map(|encounter| encounter.id);
        let encounter = encounter_id
            .map(|encounter_id| {
                store
                    .list_encounters_for_patient(patient_id)?
                    .into_iter()
                    .find(|encounter| encounter.id == encounter_id)
                    .ok_or_else(|| anyhow!("active encounter {encounter_id} was not found"))
            })
            .transpose()?;
        let note = self
            .note_draft_id
            .map(|note_id| store.get_note(note_id))
            .transpose()?
            .flatten();
        let report = audit_documentation(
            patient_id,
            encounter.as_ref(),
            note.as_ref(),
            OffsetDateTime::now_utc(),
        );

        self.apply_documentation_audit_report(&report);
        let claim = encounter
            .as_ref()
            .map(|encounter| store.get_claim_draft(encounter.id))
            .transpose()?
            .flatten();
        self.apply_billing_workbench(claim.as_ref(), note.as_ref(), &report);
        Ok(())
    }

    fn apply_documentation_audit_report(&mut self, report: &DocumentationAuditReport) {
        self.data.audit_flags = if report.flags.is_empty() {
            vec![AuditFlagItem::info("No documentation audit flags")]
        } else {
            report
                .flags
                .iter()
                .map(AuditFlagItem::from_documentation_flag)
                .collect()
        };
        self.data.billing_ready_percent = report.billing_ready_percent;
    }

    fn apply_billing_workbench(
        &mut self,
        claim: Option<&ClaimDraft>,
        note: Option<&ClinicalNote>,
        documentation_audit: &DocumentationAuditReport,
    ) {
        self.data.billing_rows = billing_rows_from_claim(claim, note, documentation_audit);
    }

    fn prepare_superbill_draft(&mut self, store: &LocalStore) -> Result<()> {
        let Some(patient_id) = self.active_patient().map(|patient| patient.id) else {
            self.last_message = "Select a patient before preparing billing".to_owned();
            return Ok(());
        };
        let Some(encounter_id) = self.active_encounter().map(|encounter| encounter.id) else {
            self.last_message = "Create an encounter before preparing billing".to_owned();
            return Ok(());
        };

        let registry = MedicalToolRuntimeRegistry::default();
        let output = registry.dispatch(MedicalToolInvocation {
            store,
            call_id: format!("tui-superbill-{}", short_id(new_id())),
            tool_name: med_agent::MedicalToolName::PrepareSuperbillDraft,
            payload: MedicalToolPayload::PrepareSuperbillDraft(PrepareSuperbillDraftRequest {
                patient_id,
                encounter_id,
                note_id: self.note_draft_id,
            }),
            context: MedicalToolContext::default(),
            approval_policy: MedicalApprovalPolicy::after_billing_confirmation(),
        })?;

        self.refresh_documentation_audit(store)?;
        self.last_message = output.tui_summary;

        Ok(())
    }

    fn arm_note_signing(&mut self) {
        if self.note_draft_id.is_none() {
            self.note_signing_armed = false;
            self.last_message = "Save a draft before signing".to_owned();
            return;
        }

        if self.note_is_signed() {
            self.note_signing_armed = false;
            self.last_message = "Note is already signed".to_owned();
            return;
        }

        if self.note_dirty {
            self.note_signing_armed = false;
            self.last_message = "Save the draft before signing".to_owned();
            return;
        }

        self.note_signing_armed = true;
        self.last_message = SIGNING_ARMED_MESSAGE.to_owned();
    }

    fn sign_note(&mut self, store: &LocalStore) -> Result<()> {
        let Some(note_id) = self.note_draft_id else {
            self.note_signing_armed = false;
            self.last_message = "Save a draft before signing".to_owned();
            return Ok(());
        };
        let Some(patient_id) = self.active_patient().map(|patient| patient.id) else {
            self.note_signing_armed = false;
            self.last_message = "Select a patient before signing".to_owned();
            return Ok(());
        };
        let Some(encounter_id) = self.active_encounter().map(|encounter| encounter.id) else {
            self.note_signing_armed = false;
            self.last_message = "Select an encounter before signing".to_owned();
            return Ok(());
        };

        let registry = MedicalToolRuntimeRegistry::default();
        let output = registry.dispatch(MedicalToolInvocation {
            store,
            call_id: format!("tui-note-sign-{}", short_id(new_id())),
            tool_name: med_agent::MedicalToolName::SignNote,
            payload: MedicalToolPayload::SignNote(SignNoteRequest {
                note_id,
                patient_id,
                encounter_id,
            }),
            context: MedicalToolContext::default(),
            approval_policy: MedicalApprovalPolicy::after_human_confirmation(),
        })?;
        let note = store
            .get_note(note_id)?
            .ok_or_else(|| anyhow!("signed note {note_id} was not found"))?;

        self.apply_note_metadata(&note);
        self.refresh_documentation_audit(store)?;
        self.last_message = output.tui_summary;

        Ok(())
    }

    fn cancel_note_signing_confirmation(&mut self) {
        self.note_signing_armed = false;
    }

    fn block_signed_note_edit(&mut self) {
        self.note_signing_armed = false;
        self.last_message = SIGNED_NOTE_LOCKED_MESSAGE.to_owned();

        if !self
            .data
            .audit_flags
            .iter()
            .any(|flag| flag.message == SIGNED_NOTE_LOCKED_MESSAGE)
        {
            self.data
                .audit_flags
                .push(AuditFlagItem::blocked(SIGNED_NOTE_LOCKED_MESSAGE));
        }
    }

    fn focus_next(&mut self) {
        self.focus = match self.focus {
            FocusArea::PatientQueue => FocusArea::Workspace,
            FocusArea::Workspace => FocusArea::Context,
            FocusArea::Context => FocusArea::Status,
            FocusArea::Status => FocusArea::PatientQueue,
        };
    }

    fn focus_previous(&mut self) {
        self.focus = match self.focus {
            FocusArea::PatientQueue => FocusArea::Status,
            FocusArea::Workspace => FocusArea::PatientQueue,
            FocusArea::Context => FocusArea::Workspace,
            FocusArea::Status => FocusArea::Context,
        };
    }

    fn select_next_patient(&mut self) {
        if self.data.patients.is_empty() {
            return;
        }

        self.selected_patient = (self.selected_patient + 1) % self.data.patients.len();
        self.reset_note_editor();
        self.last_message = self.selection_message();
    }

    fn select_previous_patient(&mut self) {
        if self.data.patients.is_empty() {
            return;
        }

        self.selected_patient = if self.selected_patient == 0 {
            self.data.patients.len() - 1
        } else {
            self.selected_patient - 1
        };
        self.reset_note_editor();
        self.last_message = self.selection_message();
    }

    fn selection_message(&self) -> String {
        self.active_patient()
            .map(|patient| format!("Selected {}", patient.display_name))
            .unwrap_or_else(|| "No patient selected".to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusArea {
    PatientQueue,
    Workspace,
    Context,
    Status,
}

impl FocusArea {
    pub fn title(self) -> &'static str {
        match self {
            Self::PatientQueue => "patients",
            Self::Workspace => "workspace",
            Self::Context => "context",
            Self::Status => "status",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceTab {
    Chart,
    Note,
    Audit,
    Billing,
}

impl WorkspaceTab {
    pub const ALL: [Self; 4] = [Self::Chart, Self::Note, Self::Audit, Self::Billing];

    pub fn title(self) -> &'static str {
        match self {
            Self::Chart => "Chart",
            Self::Note => "Note",
            Self::Audit => "Audit",
            Self::Billing => "Billing",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Chart => 0,
            Self::Note => 1,
            Self::Audit => 2,
            Self::Billing => 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentPanelState {
    pub panel_status: AgentPanelStatus,
    pub thread_status: MedicalAgentStatus,
    pub active_turn_id: Option<String>,
    pub patient_id: Option<PatientId>,
    pub encounter_id: Option<EncounterId>,
    pub note_id: Option<NoteId>,
    pub contains_phi: bool,
    pub loop_limits: MedicalLoopLimits,
    pub pending_approval: Option<AgentPendingApproval>,
    pub events: Vec<AgentEventItem>,
    pub last_error: Option<String>,
}

impl Default for AgentPanelState {
    fn default() -> Self {
        Self {
            panel_status: AgentPanelStatus::Idle,
            thread_status: MedicalAgentStatus::Idle,
            active_turn_id: None,
            patient_id: None,
            encounter_id: None,
            note_id: None,
            contains_phi: false,
            loop_limits: MedicalLoopLimits::default(),
            pending_approval: None,
            events: Vec::new(),
            last_error: None,
        }
    }
}

impl AgentPanelState {
    pub fn status_label(&self) -> &'static str {
        self.panel_status.label()
    }

    pub fn thread_status_label(&self) -> &'static str {
        medical_agent_status_label(self.thread_status)
    }

    fn record_event(&mut self, message: String, severity: Severity) {
        self.events.push(AgentEventItem { message, severity });
        if self.events.len() > AGENT_EVENT_LIMIT {
            let overflow = self.events.len() - AGENT_EVENT_LIMIT;
            self.events.drain(0..overflow);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPanelStatus {
    Idle,
    Running,
    WaitingForApproval,
    Cancelling,
    Cancelled,
    Complete,
    Aborted,
    PolicyBlocked,
    Error,
    ShuttingDown,
    Stopped,
}

impl AgentPanelStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::WaitingForApproval => "waiting",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::Complete => "complete",
            Self::Aborted => "aborted",
            Self::PolicyBlocked => "blocked",
            Self::Error => "error",
            Self::ShuttingDown => "shutdown",
            Self::Stopped => "stopped",
        }
    }

    fn from_thread_status(status: MedicalAgentStatus) -> Self {
        match status {
            MedicalAgentStatus::Idle => Self::Idle,
            MedicalAgentStatus::Running => Self::Running,
            MedicalAgentStatus::WaitingForApproval => Self::WaitingForApproval,
            MedicalAgentStatus::Cancelling => Self::Cancelling,
            MedicalAgentStatus::ShuttingDown => Self::ShuttingDown,
            MedicalAgentStatus::Stopped => Self::Stopped,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentPendingApproval {
    pub approval_id: String,
    pub turn_id: String,
    pub class: MedicalApprovalClass,
    pub redacted_reason: String,
}

#[derive(Debug, Clone)]
pub struct AgentEventItem {
    pub message: String,
    pub severity: Severity,
}

#[derive(Debug, Clone)]
pub struct DashboardData {
    pub patients: Vec<PatientQueueItem>,
    pub encounters: Vec<EncounterItem>,
    pub tasks: Vec<TaskItem>,
    pub problems: Vec<String>,
    pub medications: Vec<String>,
    pub allergies: Vec<String>,
    pub audit_flags: Vec<AuditFlagItem>,
    pub billing_rows: Vec<BillingRow>,
    pub vitals_trend: Vec<u64>,
    pub billing_ready_percent: u16,
    pub ai_status: AiStatus,
}

impl DashboardData {
    pub fn empty() -> Self {
        let ai_status = ai_status_from_env();

        Self {
            patients: Vec::new(),
            encounters: Vec::new(),
            tasks: vec![
                TaskItem::info("Local patients", 0),
                TaskItem::info("Open encounters", 0),
                TaskItem::error("Blocked AI calls", ai_blocked_count(ai_status)),
            ],
            problems: Vec::new(),
            medications: Vec::new(),
            allergies: Vec::new(),
            audit_flags: vec![AuditFlagItem::info("No local chart selected")],
            billing_rows: Vec::new(),
            vitals_trend: vec![0],
            billing_ready_percent: 0,
            ai_status,
        }
    }

    fn from_local_records(records: &[(Patient, Vec<Encounter>)], selected_patient: usize) -> Self {
        let ai_status = ai_status_from_env();
        let today = OffsetDateTime::now_utc().date();
        let patients = records
            .iter()
            .map(|(patient, encounters)| patient_queue_item(patient, encounters, today))
            .collect::<Vec<_>>();
        let encounters = records
            .get(selected_patient)
            .map(|(_, encounters)| {
                encounters
                    .iter()
                    .map(encounter_item)
                    .collect::<Vec<EncounterItem>>()
            })
            .unwrap_or_default();
        let open_encounter_count = records
            .iter()
            .flat_map(|(_, encounters)| encounters.iter())
            .filter(|encounter| is_open_encounter(&encounter.status))
            .count();

        Self {
            patients,
            encounters,
            tasks: vec![
                TaskItem::info("Local patients", records.len()),
                TaskItem::warning("Open encounters", open_encounter_count),
                TaskItem::error("Blocked AI calls", ai_blocked_count(ai_status)),
            ],
            problems: Vec::new(),
            medications: Vec::new(),
            allergies: Vec::new(),
            audit_flags: vec![
                AuditFlagItem::warning("Structured note audit pending"),
                AuditFlagItem::blocked("AI PHI request has no executed BAA"),
            ],
            billing_rows: Vec::new(),
            vitals_trend: vec![0],
            billing_ready_percent: if open_encounter_count > 0 { 25 } else { 0 },
            ai_status,
        }
    }

    #[cfg(test)]
    fn synthetic() -> Self {
        let patient_a = new_id();
        let patient_b = new_id();
        let patient_c = new_id();
        let encounter = new_id();

        Self {
            patients: vec![
                PatientQueueItem {
                    id: patient_a,
                    display_name: "Synthetic Patient A".to_owned(),
                    mrn: "MRN-0001".to_owned(),
                    age: Some(42),
                    status: "unsigned note".to_owned(),
                },
                PatientQueueItem {
                    id: patient_b,
                    display_name: "Synthetic Patient B".to_owned(),
                    mrn: "MRN-0002".to_owned(),
                    age: Some(58),
                    status: "billing flag".to_owned(),
                },
                PatientQueueItem {
                    id: patient_c,
                    display_name: "Synthetic Patient C".to_owned(),
                    mrn: "MRN-0003".to_owned(),
                    age: None,
                    status: "ready".to_owned(),
                },
            ],
            encounters: vec![EncounterItem {
                id: encounter,
                short_id: short_id(encounter),
                started_at: "2026-05-28".to_owned(),
                encounter_type: "Office visit".to_owned(),
                status: "In progress".to_owned(),
                reason: "-".to_owned(),
            }],
            tasks: vec![
                TaskItem::warning("Unsigned notes", 2),
                TaskItem::error("Billing flags", 3),
                TaskItem::info("AI blocked", 1),
            ],
            problems: vec![
                "Low back pain".to_owned(),
                "Hypertension".to_owned(),
                "Medication review due".to_owned(),
            ],
            medications: vec!["Lisinopril 10 mg".to_owned(), "Ibuprofen PRN".to_owned()],
            allergies: vec!["NKDA".to_owned()],
            audit_flags: vec![
                AuditFlagItem::warning("Assessment missing linked diagnosis"),
                AuditFlagItem::warning("Procedure code lacks supporting note section"),
                AuditFlagItem::info("Note is still unsigned"),
                AuditFlagItem::blocked("AI PHI request has no executed BAA"),
            ],
            billing_rows: vec![
                BillingRow {
                    code: "M54.50".to_owned(),
                    kind: "ICD-10-CM".to_owned(),
                    status: "linked".to_owned(),
                },
                BillingRow {
                    code: "97110".to_owned(),
                    kind: "CPT".to_owned(),
                    status: "needs note support".to_owned(),
                },
            ],
            vitals_trend: vec![98, 99, 97, 101, 100, 99, 98, 97],
            billing_ready_percent: 42,
            ai_status: ai_status_from_env(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PatientQueueItem {
    pub id: PatientId,
    pub display_name: String,
    pub mrn: String,
    pub age: Option<u8>,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct EncounterItem {
    pub id: EncounterId,
    pub short_id: String,
    pub started_at: String,
    pub encounter_type: String,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct TaskItem {
    pub label: String,
    pub count: usize,
    pub severity: Severity,
}

impl TaskItem {
    fn info(label: &str, count: usize) -> Self {
        Self {
            label: label.to_owned(),
            count,
            severity: Severity::Info,
        }
    }

    fn warning(label: &str, count: usize) -> Self {
        Self {
            label: label.to_owned(),
            count,
            severity: Severity::Warning,
        }
    }

    fn error(label: &str, count: usize) -> Self {
        Self {
            label: label.to_owned(),
            count,
            severity: Severity::Error,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditFlagItem {
    pub message: String,
    pub severity: Severity,
}

impl AuditFlagItem {
    fn info(message: &str) -> Self {
        Self {
            message: message.to_owned(),
            severity: Severity::Info,
        }
    }

    fn warning(message: &str) -> Self {
        Self {
            message: message.to_owned(),
            severity: Severity::Warning,
        }
    }

    fn blocked(message: &str) -> Self {
        Self {
            message: message.to_owned(),
            severity: Severity::Blocked,
        }
    }

    fn from_documentation_flag(flag: &DocumentationAuditFlag) -> Self {
        Self {
            message: flag.message.clone(),
            severity: match flag.severity {
                DocumentationAuditSeverity::Info => Severity::Info,
                DocumentationAuditSeverity::Warning => Severity::Warning,
                DocumentationAuditSeverity::Error => Severity::Error,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct BillingRow {
    pub code: String,
    pub kind: String,
    pub status: String,
}

fn billing_rows_from_claim(
    claim: Option<&ClaimDraft>,
    note: Option<&ClinicalNote>,
    documentation_audit: &DocumentationAuditReport,
) -> Vec<BillingRow> {
    let Some(claim) = claim else {
        return vec![BillingRow {
            code: "Draft".to_owned(),
            kind: "Superbill".to_owned(),
            status: if documentation_audit.encounter_id.is_some() {
                "press b to prepare".to_owned()
            } else {
                "no active encounter".to_owned()
            },
        }];
    };

    let readiness = assess_claim_readiness(claim, note, Some(documentation_audit));
    let mut rows = vec![BillingRow {
        code: "Readiness".to_owned(),
        kind: "Claim".to_owned(),
        status: format!("{:?}", readiness.status),
    }];

    rows.extend(claim.diagnoses.iter().map(|diagnosis| {
        BillingRow {
            code: diagnosis.code.clone(),
            kind: diagnosis_system_label(&diagnosis.system).to_owned(),
            status: diagnosis
                .description
                .clone()
                .unwrap_or_else(|| "requires review".to_owned()),
        }
    }));
    rows.extend(claim.procedures.iter().map(|procedure| {
        BillingRow {
            code: procedure.code.clone(),
            kind: procedure_system_label(&procedure.system).to_owned(),
            status: procedure
                .description
                .clone()
                .unwrap_or_else(|| format!("{} unit(s)", procedure.units)),
        }
    }));
    rows.extend(readiness.flags.iter().map(readiness_flag_row));

    rows
}

fn readiness_flag_row(flag: &ClaimReadinessFlag) -> BillingRow {
    BillingRow {
        code: flag.code.clone(),
        kind: "Checklist".to_owned(),
        status: format!(
            "{}: {}",
            readiness_severity_label(flag.severity),
            flag.message
        ),
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Blocked,
}

#[derive(Debug, Clone, Copy)]
pub enum AiStatus {
    Locked,
    Allowed,
}

fn selected_patient_index(
    records: &[(Patient, Vec<Encounter>)],
    preferred_patient_id: Option<PatientId>,
    fallback_index: usize,
) -> usize {
    if records.is_empty() {
        return 0;
    }

    preferred_patient_id
        .and_then(|patient_id| {
            records
                .iter()
                .position(|(patient, _)| patient.id == patient_id)
        })
        .unwrap_or_else(|| fallback_index.min(records.len() - 1))
}

fn patient_queue_item(
    patient: &Patient,
    encounters: &[Encounter],
    today: Date,
) -> PatientQueueItem {
    PatientQueueItem {
        id: patient.id,
        display_name: patient.display_name.clone(),
        mrn: patient
            .medical_record_number
            .clone()
            .unwrap_or_else(|| "-".to_owned()),
        age: patient
            .date_of_birth
            .and_then(|date_of_birth| age_on(date_of_birth, today)),
        status: patient_status(encounters),
    }
}

fn encounter_item(encounter: &Encounter) -> EncounterItem {
    EncounterItem {
        id: encounter.id,
        short_id: short_id(encounter.id),
        started_at: encounter.started_at.date().to_string(),
        encounter_type: encounter_type_label(&encounter.encounter_type),
        status: encounter_status_label(&encounter.status).to_owned(),
        reason: encounter.reason.clone().unwrap_or_else(|| "-".to_owned()),
    }
}

fn short_id(id: impl std::fmt::Display) -> String {
    id.to_string()[..8].to_owned()
}

fn short_text(value: &str) -> String {
    value.chars().take(8).collect()
}

fn agent_turn_instruction(note_id: Option<NoteId>, requests_signing: bool) -> String {
    if requests_signing {
        return "Review the selected local SOAP note for signed clinical change readiness"
            .to_owned();
    }

    if note_id.is_some() {
        "Review the selected local SOAP note and documentation audit state".to_owned()
    } else {
        "Review the selected local chart and documentation audit state".to_owned()
    }
}

fn approval_class_label(class: MedicalApprovalClass) -> &'static str {
    match class {
        MedicalApprovalClass::OutboundPhi => "outbound PHI",
        MedicalApprovalClass::SignedClinicalChange => "signed clinical change",
        MedicalApprovalClass::BillingSupportExport => "billing support export",
        MedicalApprovalClass::DestructiveLocalWrite => "destructive local write",
        MedicalApprovalClass::DesktopAutomation => "desktop automation",
        MedicalApprovalClass::BulkImport => "bulk import",
        MedicalApprovalClass::BulkExport => "bulk export",
        MedicalApprovalClass::PluginInstall => "plugin install",
    }
}

fn review_decision_label(decision: MedicalReviewDecision) -> &'static str {
    match decision {
        MedicalReviewDecision::Approved => "approved",
        MedicalReviewDecision::ApprovedForTurn => "approved for turn",
        MedicalReviewDecision::Denied => "denied",
        MedicalReviewDecision::AbortTurn => "aborted",
    }
}

fn abort_reason_label(reason: MedicalTurnAbortReason) -> &'static str {
    match reason {
        MedicalTurnAbortReason::UserCancelled => "cancelled",
        MedicalTurnAbortReason::ReplacedByNewTurn => "replaced",
        MedicalTurnAbortReason::PolicyBlocked => "blocked",
        MedicalTurnAbortReason::LoopLimitExceeded => "exceeded loop limit",
        MedicalTurnAbortReason::RuntimeError => "failed",
        MedicalTurnAbortReason::Shutdown => "stopped",
    }
}

fn medical_agent_status_label(status: MedicalAgentStatus) -> &'static str {
    match status {
        MedicalAgentStatus::Idle => "idle",
        MedicalAgentStatus::Running => "running",
        MedicalAgentStatus::WaitingForApproval => "waiting",
        MedicalAgentStatus::Cancelling => "cancelling",
        MedicalAgentStatus::ShuttingDown => "shutdown",
        MedicalAgentStatus::Stopped => "stopped",
    }
}

fn diagnosis_system_label(system: &DiagnosisSystem) -> &str {
    match system {
        DiagnosisSystem::Icd10Cm => "ICD-10-CM",
        DiagnosisSystem::Other(label) => label.as_str(),
    }
}

fn procedure_system_label(system: &ProcedureSystem) -> &str {
    match system {
        ProcedureSystem::Cpt => "CPT",
        ProcedureSystem::Hcpcs => "HCPCS",
        ProcedureSystem::Other(label) => label.as_str(),
    }
}

fn readiness_severity_label(severity: ClaimReadinessSeverity) -> &'static str {
    match severity {
        ClaimReadinessSeverity::Info => "info",
        ClaimReadinessSeverity::Warning => "warning",
        ClaimReadinessSeverity::Error => "blocked",
    }
}

fn patient_status(encounters: &[Encounter]) -> String {
    if encounters.is_empty() {
        return "no encounters".to_owned();
    }

    if encounters
        .iter()
        .any(|encounter| is_open_encounter(&encounter.status))
    {
        return "open encounter".to_owned();
    }

    "ready".to_owned()
}

fn is_open_encounter(status: &EncounterStatus) -> bool {
    matches!(
        status,
        EncounterStatus::Planned | EncounterStatus::InProgress
    )
}

fn encounter_type_label(encounter_type: &EncounterType) -> String {
    match encounter_type {
        EncounterType::OfficeVisit => "Office visit".to_owned(),
        EncounterType::Telehealth => "Telehealth".to_owned(),
        EncounterType::Procedure => "Procedure".to_owned(),
        EncounterType::Phone => "Phone".to_owned(),
        EncounterType::Administrative => "Administrative".to_owned(),
        EncounterType::Other(label) => label.clone(),
    }
}

fn encounter_status_label(status: &EncounterStatus) -> &'static str {
    match status {
        EncounterStatus::Planned => "Planned",
        EncounterStatus::InProgress => "In progress",
        EncounterStatus::Finished => "Finished",
        EncounterStatus::Cancelled => "Cancelled",
    }
}

fn age_on(date_of_birth: Date, today: Date) -> Option<u8> {
    let mut age = today.year() - date_of_birth.year();

    if today.ordinal() < date_of_birth.ordinal() {
        age -= 1;
    }

    u8::try_from(age).ok()
}

fn ai_status_from_env() -> AiStatus {
    if std::env::var_os("FLEKKS_EMR_TUI_DEMO_AI_ALLOWED").is_some()
        || std::env::var_os("MEDCLI_TUI_DEMO_AI_ALLOWED").is_some()
    {
        AiStatus::Allowed
    } else {
        AiStatus::Locked
    }
}

fn ai_blocked_count(ai_status: AiStatus) -> usize {
    match ai_status {
        AiStatus::Locked => 1,
        AiStatus::Allowed => 0,
    }
}

fn default_note_editor() -> TextArea<'static> {
    let mut textarea = TextArea::from([
        "Subjective:",
        "",
        "Objective:",
        "",
        "Assessment:",
        "",
        "Plan:",
        "",
    ]);
    textarea.move_cursor(CursorMove::Jump(1, 0));
    textarea
}

fn note_editor_from_sections(sections: &[NoteSection]) -> TextArea<'static> {
    const HEADINGS: [&str; 4] = ["Subjective", "Objective", "Assessment", "Plan"];

    let mut lines = Vec::new();

    for heading in HEADINGS {
        lines.push(format!("{heading}:"));

        if let Some(section) = sections
            .iter()
            .find(|section| section.heading.eq_ignore_ascii_case(heading))
        {
            lines.extend(section.body.lines().map(ToOwned::to_owned));
        }

        lines.push(String::new());
    }

    let mut textarea = TextArea::from(lines);
    textarea.move_cursor(CursorMove::Jump(1, 0));
    textarea
}

fn note_status_label(status: &NoteStatus) -> &'static str {
    match status {
        NoteStatus::Draft => "Draft",
        NoteStatus::Reviewed => "Reviewed",
        NoteStatus::Signed => "Signed",
        NoteStatus::Amended => "Amended",
        NoteStatus::Voided => "Voided",
    }
}

fn is_note_save_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn is_note_sign_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('S'))
        && !key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
}

fn is_note_editor_input_key(key: KeyEvent) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
        return false;
    }

    !matches!(key.code, KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab)
}

fn note_editor_input(key: KeyEvent) -> Input {
    if key.kind == KeyEventKind::Release {
        return Input::default();
    }

    let textarea_key = match key.code {
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Tab => Key::Tab,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Null,
        KeyCode::F(value) => Key::F(value),
        KeyCode::Char(value) => Key::Char(value),
        KeyCode::Esc
        | KeyCode::BackTab
        | KeyCode::Null
        | KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::KeypadBegin
        | KeyCode::Media(_)
        | KeyCode::Modifier(_) => Key::Null,
    };

    Input {
        key: textarea_key,
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
        alt: key.modifiers.contains(KeyModifiers::ALT),
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
    }
}

fn note_sections_from_lines(lines: &[String]) -> Vec<NoteSection> {
    const HEADINGS: [&str; 4] = ["Subjective", "Objective", "Assessment", "Plan"];

    let mut buckets: [Vec<String>; 4] = std::array::from_fn(|_| Vec::new());
    let mut current_section = 0;

    for line in lines {
        if let Some(index) = soap_heading_index(line) {
            current_section = index;
            continue;
        }

        buckets[current_section].push(line.clone());
    }

    HEADINGS
        .iter()
        .enumerate()
        .map(|(index, heading)| NoteSection {
            heading: (*heading).to_owned(),
            body: note_section_body(&buckets[index]),
            required: true,
        })
        .collect()
}

fn soap_heading_index(line: &str) -> Option<usize> {
    match line
        .trim()
        .trim_end_matches(':')
        .to_ascii_lowercase()
        .as_str()
    {
        "subjective" => Some(0),
        "objective" => Some(1),
        "assessment" => Some(2),
        "plan" => Some(3),
        _ => None,
    }
}

fn note_section_body(lines: &[String]) -> String {
    let mut start = 0;
    let mut end = lines.len();

    while start < end && lines[start].trim().is_empty() {
        start += 1;
    }

    while end > start && lines[end - 1].trim().is_empty() {
        end -= 1;
    }

    lines[start..end].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use med_store::LocalStore;
    use std::path::PathBuf;
    use std::time::Duration;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn temp_store() -> (LocalStore, PathBuf) {
        let path = std::env::temp_dir().join(format!("flekks-med-tui-test-{}.db", new_id()));
        let store = LocalStore::open_encrypted(&path, "test-key").unwrap();
        (store, path)
    }

    fn cleanup(path: PathBuf) {
        let _ = std::fs::remove_file(path);
    }

    fn drain_agent_until(
        app: &mut App,
        agent_thread: &med_agent::MedicalAgentThread,
        mut done: impl FnMut(&App) -> bool,
    ) {
        for _ in 0..8 {
            app.drain_agent_events(agent_thread, Duration::from_millis(100))
                .unwrap();
            if done(app) {
                return;
            }
        }

        panic!("agent events did not reach expected state");
    }

    fn insert_patient_with_encounter(
        store: &LocalStore,
        display_name: &str,
    ) -> (PatientId, EncounterId) {
        let now = OffsetDateTime::now_utc();
        let patient = Patient {
            id: new_id(),
            medical_record_number: None,
            display_name: display_name.to_owned(),
            date_of_birth: None,
            sex_at_birth: None,
            created_at: now,
            updated_at: now,
        };
        let encounter = Encounter {
            id: new_id(),
            patient_id: patient.id,
            practitioner_id: None,
            encounter_type: EncounterType::OfficeVisit,
            status: EncounterStatus::InProgress,
            started_at: now,
            ended_at: None,
            reason: Some("Synthetic follow-up".to_owned()),
        };

        store.insert_patient(&patient).unwrap();
        store.insert_encounter(&encounter).unwrap();

        (patient.id, encounter.id)
    }

    fn upsert_test_note(
        store: &LocalStore,
        patient_id: PatientId,
        encounter_id: EncounterId,
        subjective: &str,
        updated_at: OffsetDateTime,
        version: u32,
    ) -> NoteId {
        let note = ClinicalNote {
            id: new_id(),
            patient_id,
            encounter_id,
            author_id: None,
            template: NoteTemplate::Soap,
            status: NoteStatus::Draft,
            sections: vec![NoteSection {
                heading: "Subjective".to_owned(),
                body: subjective.to_owned(),
                required: true,
            }],
            created_at: updated_at - time::Duration::minutes(1),
            updated_at,
            signed_at: None,
            version,
        };
        let note_id = note.id;

        store.upsert_note(&note).unwrap();

        note_id
    }

    #[test]
    fn number_keys_select_workspace_tabs() {
        let mut app = App::with_data(DashboardData::synthetic());

        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.selected_tab, WorkspaceTab::Note);

        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.selected_tab, WorkspaceTab::Audit);

        app.handle_key(key(KeyCode::Char('4')));
        assert_eq!(app.selected_tab, WorkspaceTab::Billing);
    }

    #[test]
    fn patient_selection_wraps() {
        let mut app = App::with_data(DashboardData::synthetic());

        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected_patient, app.data.patients.len() - 1);

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected_patient, 0);
    }

    #[test]
    fn tab_moves_focus() {
        let mut app = App::with_data(DashboardData::synthetic());

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, FocusArea::Workspace);
    }

    #[test]
    fn note_editor_accepts_text_when_note_workspace_is_active() {
        let mut app = App::with_data(DashboardData::synthetic());
        app.selected_tab = WorkspaceTab::Note;
        app.focus = FocusArea::Workspace;

        app.handle_key(key(KeyCode::Char('x')));

        assert_eq!(app.note_editor.lines()[1], "x");
        assert!(app.note_dirty);
    }

    #[test]
    fn agent_start_key_without_thread_reports_unavailable() {
        let (store, path) = temp_store();
        let mut app = App::from_store(&store).unwrap();

        app.handle_key_with_store(key(KeyCode::F(5)), &store)
            .unwrap();

        assert_eq!(
            app.last_message,
            "Agent thread is not available in this context"
        );

        drop(store);
        cleanup(path);
    }

    #[test]
    fn loads_patients_from_store() {
        let (store, path) = temp_store();

        let now = OffsetDateTime::now_utc();
        store
            .insert_patient(&Patient {
                id: new_id(),
                medical_record_number: Some("MRN-SYNTH-001".to_owned()),
                display_name: "Synthetic Store Patient".to_owned(),
                date_of_birth: None,
                sex_at_birth: None,
                created_at: now,
                updated_at: now,
            })
            .unwrap();

        let app = App::from_store(&store).unwrap();

        assert_eq!(app.data.patients.len(), 1);
        assert_eq!(app.data.patients[0].display_name, "Synthetic Store Patient");

        drop(store);
        cleanup(path);
    }

    #[test]
    fn create_local_patient_persists_and_selects_it() {
        let (store, path) = temp_store();
        let mut app = App::from_store(&store).unwrap();

        app.handle_key_with_store(key(KeyCode::Char('n')), &store)
            .unwrap();

        let patients = store.list_patients().unwrap();
        assert_eq!(patients.len(), 1);
        assert_eq!(app.data.patients.len(), 1);
        assert_eq!(app.active_patient().unwrap().id, patients[0].id);

        drop(store);
        cleanup(path);
    }

    #[test]
    fn create_encounter_persists_for_selected_patient() {
        let (store, path) = temp_store();
        let mut app = App::from_store(&store).unwrap();

        app.handle_key_with_store(key(KeyCode::Char('n')), &store)
            .unwrap();
        let patient_id = app.active_patient().unwrap().id;

        app.handle_key_with_store(key(KeyCode::Char('e')), &store)
            .unwrap();

        let encounters = store.list_encounters_for_patient(patient_id).unwrap();
        assert_eq!(encounters.len(), 1);
        assert_eq!(app.data.encounters.len(), 1);
        assert_eq!(app.data.encounters[0].short_id, short_id(encounters[0].id));
        assert_eq!(app.data.billing_ready_percent, 0);
        assert!(app
            .data
            .audit_flags
            .iter()
            .any(|flag| flag.message == "No clinical note exists for the active encounter"));
        assert!(app
            .data
            .billing_rows
            .iter()
            .any(|row| row.status == "press b to prepare"));

        drop(store);
        cleanup(path);
    }

    #[test]
    fn loads_latest_draft_for_active_encounter() {
        let (store, path) = temp_store();
        let (patient_id, encounter_id) =
            insert_patient_with_encounter(&store, "Synthetic Draft Patient");
        let now = OffsetDateTime::now_utc();
        upsert_test_note(
            &store,
            patient_id,
            encounter_id,
            "Older subjective draft",
            now,
            1,
        );
        let latest_note_id = upsert_test_note(
            &store,
            patient_id,
            encounter_id,
            "Latest subjective draft",
            now + time::Duration::minutes(5),
            2,
        );

        let app = App::from_store(&store).unwrap();

        assert_eq!(app.note_draft_id, Some(latest_note_id));
        assert_eq!(app.note_status.as_deref(), Some("Draft"));
        assert_eq!(app.note_version, Some(2));
        assert_eq!(app.note_editor.lines()[1], "Latest subjective draft");

        drop(store);
        cleanup(path);
    }

    #[test]
    fn updating_loaded_draft_does_not_create_duplicate_notes() {
        let (store, path) = temp_store();
        let mut app = App::from_store(&store).unwrap();

        app.handle_key_with_store(key(KeyCode::Char('n')), &store)
            .unwrap();
        app.handle_key_with_store(key(KeyCode::Char('e')), &store)
            .unwrap();
        app.selected_tab = WorkspaceTab::Note;
        app.focus = FocusArea::Workspace;
        app.note_editor = TextArea::from([
            "Subjective:",
            "Initial subjective text",
            "Objective:",
            "Initial objective text",
            "Assessment:",
            "Initial assessment text",
            "Plan:",
            "Initial plan text",
        ]);
        app.note_dirty = true;

        app.handle_key_with_store(ctrl_key(KeyCode::Char('s')), &store)
            .unwrap();
        let encounter_id = app.active_encounter().unwrap().id;
        let note_id = app.note_draft_id.unwrap();

        app.note_editor = TextArea::from([
            "Subjective:",
            "Updated subjective text",
            "Objective:",
            "Updated objective text",
            "Assessment:",
            "Updated assessment text",
            "Plan:",
            "Updated plan text",
        ]);
        app.note_dirty = true;
        app.handle_key_with_store(ctrl_key(KeyCode::Char('s')), &store)
            .unwrap();

        let notes = store.list_notes_for_encounter(encounter_id).unwrap();
        let note = store.get_note(note_id).unwrap().unwrap();

        assert_eq!(notes.len(), 1);
        assert_eq!(note.version, 2);
        assert_eq!(note.sections[0].body, "Updated subjective text");
        assert_eq!(app.note_version, Some(2));
        assert_eq!(app.note_draft_id, Some(note_id));

        drop(store);
        cleanup(path);
    }

    #[test]
    fn changing_patient_selection_loads_that_patients_latest_draft() {
        let (store, path) = temp_store();
        let (patient_a, encounter_a) = insert_patient_with_encounter(&store, "A Patient");
        let (patient_b, encounter_b) = insert_patient_with_encounter(&store, "B Patient");
        let now = OffsetDateTime::now_utc();
        let note_a = upsert_test_note(&store, patient_a, encounter_a, "A patient note", now, 1);
        let note_b = upsert_test_note(
            &store,
            patient_b,
            encounter_b,
            "B patient note",
            now + time::Duration::minutes(1),
            1,
        );

        let mut app = App::from_store(&store).unwrap();

        assert_eq!(app.note_draft_id, Some(note_a));
        assert_eq!(app.note_editor.lines()[1], "A patient note");

        app.handle_key_with_store(key(KeyCode::Down), &store)
            .unwrap();

        assert_eq!(app.note_draft_id, Some(note_b));
        assert_eq!(app.note_editor.lines()[1], "B patient note");

        drop(store);
        cleanup(path);
    }

    #[test]
    fn sign_key_requires_second_confirmation_before_signing() {
        let (store, path) = temp_store();
        let (patient_id, encounter_id) =
            insert_patient_with_encounter(&store, "Synthetic Sign Guard Patient");
        let note_id = upsert_test_note(
            &store,
            patient_id,
            encounter_id,
            "Guarded draft",
            OffsetDateTime::now_utc(),
            1,
        );
        let mut app = App::from_store(&store).unwrap();
        app.selected_tab = WorkspaceTab::Note;
        app.focus = FocusArea::Workspace;

        app.handle_key_with_store(key(KeyCode::Char('S')), &store)
            .unwrap();

        let note = store.get_note(note_id).unwrap().unwrap();

        assert!(matches!(note.status, NoteStatus::Draft));
        assert_eq!(app.note_draft_id, Some(note_id));
        assert!(app.note_signing_armed);
        assert_eq!(app.last_message, SIGNING_ARMED_MESSAGE);

        drop(store);
        cleanup(path);
    }

    #[test]
    fn agent_note_turn_waits_for_signed_change_approval_then_completes() {
        let (store, path) = temp_store();
        let (patient_id, encounter_id) =
            insert_patient_with_encounter(&store, "Synthetic Agent Patient");
        let note_id = upsert_test_note(
            &store,
            patient_id,
            encounter_id,
            "Ready for agent review",
            OffsetDateTime::now_utc(),
            1,
        );
        let mut app = App::from_store(&store).unwrap();
        app.selected_tab = WorkspaceTab::Note;
        app.focus = FocusArea::Workspace;
        let agent_thread =
            med_agent::MedicalAgentThread::spawn(med_agent::MedicalAgentThreadConfig::default());

        drain_agent_until(&mut app, &agent_thread, |app| !app.agent.events.is_empty());
        app.handle_key_with_store_and_agent(key(KeyCode::F(5)), &store, Some(&agent_thread))
            .unwrap();
        drain_agent_until(&mut app, &agent_thread, |app| {
            app.agent.pending_approval.is_some()
        });

        let pending = app.agent.pending_approval.as_ref().unwrap();
        assert_eq!(app.agent.panel_status, AgentPanelStatus::WaitingForApproval);
        assert_eq!(pending.class, MedicalApprovalClass::SignedClinicalChange);
        assert_eq!(app.agent.note_id, Some(note_id));

        app.handle_key_with_store_and_agent(key(KeyCode::F(6)), &store, Some(&agent_thread))
            .unwrap();
        drain_agent_until(&mut app, &agent_thread, |app| {
            app.agent.panel_status == AgentPanelStatus::Complete
        });

        assert!(app.agent.pending_approval.is_none());
        assert!(app
            .agent
            .events
            .iter()
            .any(|event| event.message.contains("approved for turn")));

        agent_thread.shutdown_and_wait().unwrap();
        drop(store);
        cleanup(path);
    }

    #[test]
    fn agent_pending_approval_can_be_denied_from_tui() {
        let (store, path) = temp_store();
        let (patient_id, encounter_id) =
            insert_patient_with_encounter(&store, "Synthetic Agent Deny Patient");
        upsert_test_note(
            &store,
            patient_id,
            encounter_id,
            "Ready for denial",
            OffsetDateTime::now_utc(),
            1,
        );
        let mut app = App::from_store(&store).unwrap();
        app.selected_tab = WorkspaceTab::Note;
        let agent_thread =
            med_agent::MedicalAgentThread::spawn(med_agent::MedicalAgentThreadConfig::default());

        drain_agent_until(&mut app, &agent_thread, |app| !app.agent.events.is_empty());
        app.handle_key_with_store_and_agent(key(KeyCode::F(5)), &store, Some(&agent_thread))
            .unwrap();
        drain_agent_until(&mut app, &agent_thread, |app| {
            app.agent.pending_approval.is_some()
        });
        app.handle_key_with_store_and_agent(key(KeyCode::F(7)), &store, Some(&agent_thread))
            .unwrap();
        drain_agent_until(&mut app, &agent_thread, |app| {
            app.agent.panel_status == AgentPanelStatus::Aborted
        });

        assert!(app.agent.pending_approval.is_none());
        assert!(app
            .agent
            .events
            .iter()
            .any(|event| event.message.contains("denied")));

        agent_thread.shutdown_and_wait().unwrap();
        drop(store);
        cleanup(path);
    }

    #[test]
    fn agent_pending_turn_can_be_cancelled_from_tui() {
        let (store, path) = temp_store();
        let (patient_id, encounter_id) =
            insert_patient_with_encounter(&store, "Synthetic Agent Cancel Patient");
        upsert_test_note(
            &store,
            patient_id,
            encounter_id,
            "Ready for cancellation",
            OffsetDateTime::now_utc(),
            1,
        );
        let mut app = App::from_store(&store).unwrap();
        app.selected_tab = WorkspaceTab::Note;
        let agent_thread =
            med_agent::MedicalAgentThread::spawn(med_agent::MedicalAgentThreadConfig::default());

        drain_agent_until(&mut app, &agent_thread, |app| !app.agent.events.is_empty());
        app.handle_key_with_store_and_agent(key(KeyCode::F(5)), &store, Some(&agent_thread))
            .unwrap();
        drain_agent_until(&mut app, &agent_thread, |app| {
            app.agent.pending_approval.is_some()
        });
        app.handle_key_with_store_and_agent(key(KeyCode::F(8)), &store, Some(&agent_thread))
            .unwrap();
        drain_agent_until(&mut app, &agent_thread, |app| {
            app.agent.panel_status == AgentPanelStatus::Cancelled
        });

        assert!(app.agent.pending_approval.is_none());

        agent_thread.shutdown_and_wait().unwrap();
        drop(store);
        cleanup(path);
    }

    #[test]
    fn second_sign_key_signs_note_and_writes_audit_event() {
        let (store, path) = temp_store();
        let (patient_id, encounter_id) =
            insert_patient_with_encounter(&store, "Synthetic Sign Patient");
        let note_id = upsert_test_note(
            &store,
            patient_id,
            encounter_id,
            "Ready to sign",
            OffsetDateTime::now_utc(),
            1,
        );
        let mut app = App::from_store(&store).unwrap();
        app.selected_tab = WorkspaceTab::Note;
        app.focus = FocusArea::Workspace;
        let before = store.audit_event_count().unwrap();

        app.handle_key_with_store(key(KeyCode::Char('S')), &store)
            .unwrap();
        app.handle_key_with_store(key(KeyCode::Char('S')), &store)
            .unwrap();

        let note = store.get_note(note_id).unwrap().unwrap();

        assert!(matches!(note.status, NoteStatus::Signed));
        assert!(note.signed_at.is_some());
        assert_eq!(app.note_status.as_deref(), Some("Signed"));
        assert!(app.note_signed_at.is_some());
        assert!(!app.note_signing_armed);
        assert_eq!(store.audit_event_count().unwrap(), before + 1);

        drop(store);
        cleanup(path);
    }

    #[test]
    fn navigation_cancels_armed_note_signing() {
        let (store, path) = temp_store();
        let (patient_id, encounter_id) =
            insert_patient_with_encounter(&store, "Synthetic Cancel Patient");
        upsert_test_note(
            &store,
            patient_id,
            encounter_id,
            "Cancel signing",
            OffsetDateTime::now_utc(),
            1,
        );
        let mut app = App::from_store(&store).unwrap();
        app.selected_tab = WorkspaceTab::Note;
        app.focus = FocusArea::Workspace;

        app.handle_key_with_store(key(KeyCode::Char('S')), &store)
            .unwrap();
        app.handle_key_with_store(key(KeyCode::Tab), &store)
            .unwrap();

        assert!(!app.note_signing_armed);

        drop(store);
        cleanup(path);
    }

    #[test]
    fn signed_note_blocks_edits_and_saves() {
        let (store, path) = temp_store();
        let (patient_id, encounter_id) =
            insert_patient_with_encounter(&store, "Synthetic Locked Patient");
        let note_id = upsert_test_note(
            &store,
            patient_id,
            encounter_id,
            "Locked content",
            OffsetDateTime::now_utc(),
            1,
        );
        store
            .sign_note_draft(note_id, OffsetDateTime::now_utc())
            .unwrap();
        let mut app = App::from_store(&store).unwrap();
        app.selected_tab = WorkspaceTab::Note;
        app.focus = FocusArea::Workspace;

        app.handle_key_with_store(key(KeyCode::Char('x')), &store)
            .unwrap();
        app.handle_key_with_store(ctrl_key(KeyCode::Char('s')), &store)
            .unwrap();

        let note = store.get_note(note_id).unwrap().unwrap();

        assert_eq!(app.note_editor.lines()[1], "Locked content");
        assert!(!app.note_dirty);
        assert!(matches!(note.status, NoteStatus::Signed));
        assert_eq!(note.sections[0].body, "Locked content");
        assert_eq!(app.last_message, SIGNED_NOTE_LOCKED_MESSAGE);

        drop(store);
        cleanup(path);
    }

    #[test]
    fn save_note_draft_persists_note_for_active_encounter() {
        let (store, path) = temp_store();
        let mut app = App::from_store(&store).unwrap();

        app.handle_key_with_store(key(KeyCode::Char('n')), &store)
            .unwrap();
        app.handle_key_with_store(key(KeyCode::Char('e')), &store)
            .unwrap();
        app.selected_tab = WorkspaceTab::Note;
        app.focus = FocusArea::Workspace;
        app.note_editor = TextArea::from([
            "Subjective:",
            "Synthetic subjective text",
            "Objective:",
            "Synthetic objective text",
            "Assessment:",
            "Synthetic assessment text",
            "Plan:",
            "Synthetic plan text",
        ]);
        app.note_dirty = true;

        let before = store.audit_event_count().unwrap();
        app.handle_key_with_store(ctrl_key(KeyCode::Char('s')), &store)
            .unwrap();

        let note_id = app.note_draft_id.unwrap();
        let note = store.get_note(note_id).unwrap().unwrap();

        assert_eq!(note.patient_id, app.active_patient().unwrap().id);
        assert_eq!(note.encounter_id, app.active_encounter().unwrap().id);
        assert_eq!(note.sections.len(), 4);
        assert_eq!(note.sections[0].heading, "Subjective");
        assert_eq!(note.sections[0].body, "Synthetic subjective text");
        assert!(!app.note_dirty);
        assert_eq!(store.audit_event_count().unwrap(), before + 1);
        assert_eq!(app.data.billing_ready_percent, 50);
        assert!(app
            .data
            .audit_flags
            .iter()
            .any(|flag| flag.message == "Clinical note is still unsigned"));

        drop(store);
        cleanup(path);
    }

    #[test]
    fn signing_complete_soap_note_marks_billing_ready() {
        let (store, path) = temp_store();
        let mut app = App::from_store(&store).unwrap();

        app.handle_key_with_store(key(KeyCode::Char('n')), &store)
            .unwrap();
        app.handle_key_with_store(key(KeyCode::Char('e')), &store)
            .unwrap();
        app.selected_tab = WorkspaceTab::Note;
        app.focus = FocusArea::Workspace;
        app.note_editor = TextArea::from([
            "Subjective:",
            "Patient reports improvement",
            "Objective:",
            "Vitals stable",
            "Assessment:",
            "Improving",
            "Plan:",
            "Continue current plan",
        ]);
        app.note_dirty = true;

        app.handle_key_with_store(ctrl_key(KeyCode::Char('s')), &store)
            .unwrap();
        app.handle_key_with_store(key(KeyCode::Char('S')), &store)
            .unwrap();
        app.handle_key_with_store(key(KeyCode::Char('S')), &store)
            .unwrap();

        assert_eq!(app.note_status.as_deref(), Some("Signed"));
        assert_eq!(app.data.billing_ready_percent, 100);
        assert!(app.data.audit_flags.iter().any(|flag| {
            flag.message == "Signed note is immutable and ready for billing review"
        }));

        drop(store);
        cleanup(path);
    }

    #[test]
    fn billing_key_prepares_superbill_draft_for_active_encounter() {
        let (store, path) = temp_store();
        let mut app = App::from_store(&store).unwrap();

        app.handle_key_with_store(key(KeyCode::Char('n')), &store)
            .unwrap();
        app.handle_key_with_store(key(KeyCode::Char('e')), &store)
            .unwrap();
        app.selected_tab = WorkspaceTab::Note;
        app.focus = FocusArea::Workspace;
        app.note_editor = TextArea::from([
            "Subjective:",
            "Patient reports improvement",
            "Objective:",
            "Vitals stable",
            "Assessment:",
            "Improving",
            "Plan:",
            "Continue current plan",
        ]);
        app.note_dirty = true;
        app.handle_key_with_store(ctrl_key(KeyCode::Char('s')), &store)
            .unwrap();
        app.handle_key_with_store(key(KeyCode::Char('S')), &store)
            .unwrap();
        app.handle_key_with_store(key(KeyCode::Char('S')), &store)
            .unwrap();

        let encounter_id = app.active_encounter().unwrap().id;
        let before = store.audit_event_count().unwrap();
        app.selected_tab = WorkspaceTab::Billing;
        app.handle_key_with_store(key(KeyCode::Char('b')), &store)
            .unwrap();

        let claim = store.get_claim_draft(encounter_id).unwrap().unwrap();

        assert_eq!(claim.diagnoses[0].code, "TBD");
        assert_eq!(claim.procedures[0].code, "TBD");
        assert_eq!(store.audit_event_count().unwrap(), before + 1);
        assert!(app
            .data
            .billing_rows
            .iter()
            .any(|row| row.kind == "ICD-10-CM" && row.code == "TBD"));
        assert!(app
            .data
            .billing_rows
            .iter()
            .any(|row| row.kind == "Checklist" && row.code == "diagnosis_placeholder"));

        drop(store);
        cleanup(path);
    }
}
