use std::collections::HashMap;

use med_core::{
    audit_documentation, new_id, AuditAction, AuditEvent, ClinicalNote, Encounter, EncounterStatus,
    NoteId, NoteSection, NoteStatus, NoteTemplate, PatientId, PractitionerId,
};
use med_store::LocalStore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::MedicalToolName;

pub trait MedicalToolRuntime: Send + Sync {
    fn name(&self) -> MedicalToolName;

    fn risk_profile(&self) -> MedicalToolRisk;

    fn supports_parallel(&self) -> bool {
        false
    }

    fn handle(
        &self,
        invocation: MedicalToolInvocation<'_>,
    ) -> Result<MedicalToolOutput, MedicalToolError>;
}

pub struct MedicalToolRuntimeRegistry {
    tools: HashMap<MedicalToolName, Box<dyn MedicalToolRuntime>>,
}

impl Default for MedicalToolRuntimeRegistry {
    fn default() -> Self {
        Self::with_default_tools()
    }
}

impl MedicalToolRuntimeRegistry {
    pub fn with_default_tools() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };
        registry.register(ReadPatientSummaryTool);
        registry.register(SaveNoteDraftTool::new(MedicalToolName::CreateNoteDraft));
        registry.register(SaveNoteDraftTool::new(MedicalToolName::UpdateNoteDraft));
        registry.register(SignNoteTool);
        registry.register(RunDocumentationAuditTool);
        registry
    }

    pub fn register<T>(&mut self, tool: T)
    where
        T: MedicalToolRuntime + 'static,
    {
        self.tools.insert(tool.name(), Box::new(tool));
    }

    pub fn get(&self, name: &MedicalToolName) -> Option<&dyn MedicalToolRuntime> {
        self.tools.get(name).map(Box::as_ref)
    }

    pub fn dispatch(
        &self,
        invocation: MedicalToolInvocation<'_>,
    ) -> Result<MedicalToolOutput, MedicalToolError> {
        let tool = self
            .get(&invocation.tool_name)
            .ok_or_else(|| MedicalToolError::UnknownTool(invocation.tool_name.clone()))?;

        match invocation.approval_policy.evaluate(tool.risk_profile()) {
            MedicalApprovalDecision::Allowed => tool.handle(invocation),
            MedicalApprovalDecision::RequiresHumanApproval => {
                Err(MedicalToolError::ApprovalRequired(invocation.tool_name))
            }
            MedicalApprovalDecision::Blocked => {
                Err(MedicalToolError::PolicyBlocked(invocation.tool_name))
            }
        }
    }
}

#[derive(Clone)]
pub struct MedicalToolInvocation<'a> {
    pub store: &'a LocalStore,
    pub call_id: String,
    pub tool_name: MedicalToolName,
    pub payload: MedicalToolPayload,
    pub context: MedicalToolContext,
    pub approval_policy: MedicalApprovalPolicy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MedicalToolContext {
    pub actor_id: Option<PractitionerId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MedicalToolPayload {
    ReadPatientSummary(ReadPatientSummaryRequest),
    SaveNoteDraft(SaveNoteDraftRequest),
    SignNote(SignNoteRequest),
    RunDocumentationAudit(RunDocumentationAuditRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadPatientSummaryRequest {
    pub patient_id: PatientId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveNoteDraftRequest {
    pub note_id: Option<NoteId>,
    pub patient_id: PatientId,
    pub encounter_id: med_core::EncounterId,
    pub template: NoteTemplate,
    pub sections: Vec<NoteSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignNoteRequest {
    pub note_id: NoteId,
    pub patient_id: PatientId,
    pub encounter_id: med_core::EncounterId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDocumentationAuditRequest {
    pub patient_id: PatientId,
    pub encounter_id: Option<med_core::EncounterId>,
    pub note_id: Option<NoteId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalToolOutput {
    pub call_id: String,
    pub tool_name: MedicalToolName,
    pub success: bool,
    pub model_summary: String,
    pub tui_summary: String,
    pub structured: serde_json::Value,
    pub audit_event_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MedicalToolRisk {
    LocalRead,
    LocalDraftWrite,
    OutboundPhi,
    SignedClinicalChange,
    BillingSupportExport,
    DestructiveLocalWrite,
    DesktopAutomation,
    PluginInstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalApprovalPolicy {
    pub allow_local_reads: bool,
    pub allow_local_draft_writes: bool,
    pub require_human_approval_for_draft_writes: bool,
    pub allow_signed_clinical_changes: bool,
}

impl MedicalApprovalPolicy {
    pub fn local_default() -> Self {
        Self {
            allow_local_reads: true,
            allow_local_draft_writes: true,
            require_human_approval_for_draft_writes: false,
            allow_signed_clinical_changes: false,
        }
    }

    pub fn after_human_confirmation() -> Self {
        Self {
            allow_signed_clinical_changes: true,
            ..Self::local_default()
        }
    }

    pub fn read_only() -> Self {
        Self {
            allow_local_reads: true,
            allow_local_draft_writes: false,
            require_human_approval_for_draft_writes: false,
            allow_signed_clinical_changes: false,
        }
    }

    pub fn evaluate(self, risk: MedicalToolRisk) -> MedicalApprovalDecision {
        match risk {
            MedicalToolRisk::LocalRead if self.allow_local_reads => {
                MedicalApprovalDecision::Allowed
            }
            MedicalToolRisk::LocalRead => MedicalApprovalDecision::Blocked,
            MedicalToolRisk::LocalDraftWrite if !self.allow_local_draft_writes => {
                MedicalApprovalDecision::Blocked
            }
            MedicalToolRisk::LocalDraftWrite if self.require_human_approval_for_draft_writes => {
                MedicalApprovalDecision::RequiresHumanApproval
            }
            MedicalToolRisk::LocalDraftWrite => MedicalApprovalDecision::Allowed,
            MedicalToolRisk::SignedClinicalChange if self.allow_signed_clinical_changes => {
                MedicalApprovalDecision::Allowed
            }
            MedicalToolRisk::OutboundPhi
            | MedicalToolRisk::SignedClinicalChange
            | MedicalToolRisk::BillingSupportExport
            | MedicalToolRisk::DestructiveLocalWrite
            | MedicalToolRisk::DesktopAutomation
            | MedicalToolRisk::PluginInstall => MedicalApprovalDecision::RequiresHumanApproval,
        }
    }
}

impl Default for MedicalApprovalPolicy {
    fn default() -> Self {
        Self::local_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MedicalApprovalDecision {
    Allowed,
    RequiresHumanApproval,
    Blocked,
}

#[derive(Debug, Error)]
pub enum MedicalToolError {
    #[error("unknown medical tool runtime: {0:?}")]
    UnknownTool(MedicalToolName),

    #[error("tool {tool:?} received invalid payload; expected {expected}")]
    InvalidPayload {
        tool: MedicalToolName,
        expected: &'static str,
    },

    #[error("patient not found: {0}")]
    PatientNotFound(PatientId),

    #[error("encounter {encounter_id} does not belong to patient {patient_id}")]
    EncounterNotFound {
        patient_id: PatientId,
        encounter_id: med_core::EncounterId,
    },

    #[error("tool {0:?} requires human approval")]
    ApprovalRequired(MedicalToolName),

    #[error("tool {0:?} blocked by medical approval policy")]
    PolicyBlocked(MedicalToolName),

    #[error("note draft must include at least one section")]
    EmptyNoteSections,

    #[error("note not found: {0}")]
    NoteNotFound(NoteId),

    #[error("note {0} is not a draft and cannot be signed")]
    NoteNotDraft(NoteId),

    #[error("store error: {0}")]
    Store(#[from] med_store::StoreError),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

struct ReadPatientSummaryTool;

impl MedicalToolRuntime for ReadPatientSummaryTool {
    fn name(&self) -> MedicalToolName {
        MedicalToolName::ReadPatientSummary
    }

    fn risk_profile(&self) -> MedicalToolRisk {
        MedicalToolRisk::LocalRead
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    fn handle(
        &self,
        invocation: MedicalToolInvocation<'_>,
    ) -> Result<MedicalToolOutput, MedicalToolError> {
        let MedicalToolPayload::ReadPatientSummary(request) = invocation.payload else {
            return Err(MedicalToolError::InvalidPayload {
                tool: invocation.tool_name,
                expected: "ReadPatientSummary",
            });
        };

        let patient = invocation
            .store
            .get_patient(request.patient_id)?
            .ok_or(MedicalToolError::PatientNotFound(request.patient_id))?;
        let encounters = invocation
            .store
            .list_encounters_for_patient(request.patient_id)?;
        let open_encounter_count = encounters
            .iter()
            .filter(|encounter| is_open_encounter(&encounter.status))
            .count();
        let latest_encounter_id = encounters.first().map(|encounter| encounter.id);
        let audit_event_id = append_tool_audit(
            invocation.store,
            AuditAction::ChartOpened,
            invocation.context.actor_id,
            Some(patient.id),
            latest_encounter_id,
            None,
            json!({
                "tool": invocation.tool_name.as_str(),
                "call_id": invocation.call_id.clone(),
                "encounter_count": encounters.len(),
                "open_encounter_count": open_encounter_count
            }),
        )?;

        Ok(MedicalToolOutput {
            call_id: invocation.call_id,
            tool_name: invocation.tool_name,
            success: true,
            model_summary: format!(
                "Loaded local chart summary for selected patient with {} encounters.",
                encounters.len()
            ),
            tui_summary: format!(
                "{} has {} local encounters.",
                patient.display_name,
                encounters.len()
            ),
            structured: json!({
                "patient_id": patient.id,
                "encounter_count": encounters.len(),
                "open_encounter_count": open_encounter_count,
                "latest_encounter_id": latest_encounter_id
            }),
            audit_event_id: Some(audit_event_id),
        })
    }
}

struct SaveNoteDraftTool {
    name: MedicalToolName,
}

impl SaveNoteDraftTool {
    fn new(name: MedicalToolName) -> Self {
        Self { name }
    }
}

impl MedicalToolRuntime for SaveNoteDraftTool {
    fn name(&self) -> MedicalToolName {
        self.name.clone()
    }

    fn risk_profile(&self) -> MedicalToolRisk {
        MedicalToolRisk::LocalDraftWrite
    }

    fn handle(
        &self,
        invocation: MedicalToolInvocation<'_>,
    ) -> Result<MedicalToolOutput, MedicalToolError> {
        let MedicalToolPayload::SaveNoteDraft(request) = invocation.payload else {
            return Err(MedicalToolError::InvalidPayload {
                tool: invocation.tool_name,
                expected: "SaveNoteDraft",
            });
        };

        if request.sections.is_empty() {
            return Err(MedicalToolError::EmptyNoteSections);
        }

        invocation
            .store
            .get_patient(request.patient_id)?
            .ok_or(MedicalToolError::PatientNotFound(request.patient_id))?;
        let encounter_exists = invocation
            .store
            .list_encounters_for_patient(request.patient_id)?
            .iter()
            .any(|encounter| encounter.id == request.encounter_id);
        if !encounter_exists {
            return Err(MedicalToolError::EncounterNotFound {
                patient_id: request.patient_id,
                encounter_id: request.encounter_id,
            });
        }

        let now = OffsetDateTime::now_utc();
        let existing_note = request
            .note_id
            .map(|note_id| invocation.store.get_note(note_id))
            .transpose()?
            .flatten();
        let note_id = request.note_id.unwrap_or_else(new_id);
        let created_at = existing_note.as_ref().map_or(now, |note| note.created_at);
        let version = existing_note
            .as_ref()
            .map_or(1, |note| note.version.saturating_add(1));
        let action = if existing_note.is_some() {
            AuditAction::NoteEdited
        } else {
            AuditAction::NoteCreated
        };

        let note = ClinicalNote {
            id: note_id,
            patient_id: request.patient_id,
            encounter_id: request.encounter_id,
            author_id: invocation.context.actor_id,
            template: request.template,
            status: NoteStatus::Draft,
            sections: request.sections,
            created_at,
            updated_at: now,
            signed_at: None,
            version,
        };

        invocation.store.upsert_note(&note)?;
        let audit_event_id = append_tool_audit(
            invocation.store,
            action,
            invocation.context.actor_id,
            Some(note.patient_id),
            Some(note.encounter_id),
            Some(note.id),
            json!({
                "tool": invocation.tool_name.as_str(),
                "call_id": invocation.call_id.clone(),
                "section_count": note.sections.len(),
                "version": note.version
            }),
        )?;

        Ok(MedicalToolOutput {
            call_id: invocation.call_id,
            tool_name: invocation.tool_name,
            success: true,
            model_summary: format!(
                "Saved local note draft with {} sections. Human review is still required before signing.",
                note.sections.len()
            ),
            tui_summary: format!("Saved note draft {}", short_id(note.id)),
            structured: json!({
                "note_id": note.id,
                "patient_id": note.patient_id,
                "encounter_id": note.encounter_id,
                "status": "Draft",
                "version": note.version,
                "section_count": note.sections.len()
            }),
            audit_event_id: Some(audit_event_id),
        })
    }
}

struct SignNoteTool;

impl MedicalToolRuntime for SignNoteTool {
    fn name(&self) -> MedicalToolName {
        MedicalToolName::SignNote
    }

    fn risk_profile(&self) -> MedicalToolRisk {
        MedicalToolRisk::SignedClinicalChange
    }

    fn handle(
        &self,
        invocation: MedicalToolInvocation<'_>,
    ) -> Result<MedicalToolOutput, MedicalToolError> {
        let MedicalToolPayload::SignNote(request) = invocation.payload else {
            return Err(MedicalToolError::InvalidPayload {
                tool: invocation.tool_name,
                expected: "SignNote",
            });
        };

        let note = invocation
            .store
            .get_note(request.note_id)?
            .ok_or(MedicalToolError::NoteNotFound(request.note_id))?;
        if note.patient_id != request.patient_id || note.encounter_id != request.encounter_id {
            return Err(MedicalToolError::EncounterNotFound {
                patient_id: request.patient_id,
                encounter_id: request.encounter_id,
            });
        }
        if !matches!(note.status, NoteStatus::Draft) {
            return Err(MedicalToolError::NoteNotDraft(request.note_id));
        }

        let signed_at = OffsetDateTime::now_utc();
        let signed_note = invocation
            .store
            .sign_note_draft(request.note_id, signed_at)?;
        let audit_event_id = append_tool_audit(
            invocation.store,
            AuditAction::NoteSigned,
            invocation.context.actor_id,
            Some(signed_note.patient_id),
            Some(signed_note.encounter_id),
            Some(signed_note.id),
            json!({
                "tool": invocation.tool_name.as_str(),
                "call_id": invocation.call_id.clone(),
                "version": signed_note.version,
                "signed_at": signed_at.to_string()
            }),
        )?;

        Ok(MedicalToolOutput {
            call_id: invocation.call_id,
            tool_name: invocation.tool_name,
            success: true,
            model_summary:
                "Signed local note after human confirmation. Signed content is now immutable."
                    .to_owned(),
            tui_summary: format!("Signed note {}", short_id(signed_note.id)),
            structured: json!({
                "note_id": signed_note.id,
                "patient_id": signed_note.patient_id,
                "encounter_id": signed_note.encounter_id,
                "status": "Signed",
                "version": signed_note.version,
                "signed_at": signed_at
            }),
            audit_event_id: Some(audit_event_id),
        })
    }
}

struct RunDocumentationAuditTool;

impl MedicalToolRuntime for RunDocumentationAuditTool {
    fn name(&self) -> MedicalToolName {
        MedicalToolName::RunDocumentationAudit
    }

    fn risk_profile(&self) -> MedicalToolRisk {
        MedicalToolRisk::LocalRead
    }

    fn handle(
        &self,
        invocation: MedicalToolInvocation<'_>,
    ) -> Result<MedicalToolOutput, MedicalToolError> {
        let MedicalToolPayload::RunDocumentationAudit(request) = invocation.payload else {
            return Err(MedicalToolError::InvalidPayload {
                tool: invocation.tool_name,
                expected: "RunDocumentationAudit",
            });
        };

        invocation
            .store
            .get_patient(request.patient_id)?
            .ok_or(MedicalToolError::PatientNotFound(request.patient_id))?;

        let encounter = load_requested_encounter(invocation.store, &request)?;
        let note = load_requested_note(invocation.store, &request, encounter.as_ref())?;
        let report = audit_documentation(
            request.patient_id,
            encounter.as_ref(),
            note.as_ref(),
            OffsetDateTime::now_utc(),
        );
        let audit_event_id = append_tool_audit(
            invocation.store,
            AuditAction::DocumentationAuditRan,
            invocation.context.actor_id,
            Some(report.patient_id),
            report.encounter_id,
            report.note_id,
            json!({
                "tool": invocation.tool_name.as_str(),
                "call_id": invocation.call_id.clone(),
                "flag_count": report.flags.len(),
                "billing_ready_percent": report.billing_ready_percent
            }),
        )?;

        Ok(MedicalToolOutput {
            call_id: invocation.call_id,
            tool_name: invocation.tool_name,
            success: true,
            model_summary: format!(
                "Ran local documentation audit with {} flags and {}% billing readiness.",
                report.flags.len(),
                report.billing_ready_percent
            ),
            tui_summary: format!(
                "Documentation audit: {} flags, {}% billing ready",
                report.flags.len(),
                report.billing_ready_percent
            ),
            structured: serde_json::to_value(&report)?,
            audit_event_id: Some(audit_event_id),
        })
    }
}

fn load_requested_encounter(
    store: &LocalStore,
    request: &RunDocumentationAuditRequest,
) -> Result<Option<Encounter>, MedicalToolError> {
    let Some(encounter_id) = request.encounter_id else {
        return Ok(None);
    };

    let encounter = store
        .list_encounters_for_patient(request.patient_id)?
        .into_iter()
        .find(|encounter| encounter.id == encounter_id)
        .ok_or(MedicalToolError::EncounterNotFound {
            patient_id: request.patient_id,
            encounter_id,
        })?;

    Ok(Some(encounter))
}

fn load_requested_note(
    store: &LocalStore,
    request: &RunDocumentationAuditRequest,
    encounter: Option<&Encounter>,
) -> Result<Option<ClinicalNote>, MedicalToolError> {
    if let Some(note_id) = request.note_id {
        let note = store
            .get_note(note_id)?
            .ok_or(MedicalToolError::NoteNotFound(note_id))?;
        if note.patient_id != request.patient_id
            || request
                .encounter_id
                .is_some_and(|encounter_id| note.encounter_id != encounter_id)
        {
            return Err(MedicalToolError::EncounterNotFound {
                patient_id: request.patient_id,
                encounter_id: note.encounter_id,
            });
        }

        return Ok(Some(note));
    }

    let Some(encounter) = encounter else {
        return Ok(None);
    };

    Ok(store
        .list_notes_for_encounter(encounter.id)?
        .into_iter()
        .next())
}

fn append_tool_audit(
    store: &LocalStore,
    action: AuditAction,
    actor_id: Option<PractitionerId>,
    patient_id: Option<PatientId>,
    encounter_id: Option<med_core::EncounterId>,
    note_id: Option<NoteId>,
    details: serde_json::Value,
) -> Result<Uuid, med_store::StoreError> {
    let id = new_id();
    store.append_audit_event(&AuditEvent {
        id,
        actor_id,
        patient_id,
        encounter_id,
        note_id,
        action,
        occurred_at: OffsetDateTime::now_utc(),
        details,
    })?;
    Ok(id)
}

fn is_open_encounter(status: &EncounterStatus) -> bool {
    matches!(
        status,
        EncounterStatus::Planned | EncounterStatus::InProgress
    )
}

fn short_id(id: Uuid) -> String {
    id.to_string()[..8].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use med_core::{Encounter, EncounterType, Patient};
    use std::path::PathBuf;
    use time::Date;

    fn temp_store() -> (LocalStore, PathBuf) {
        let path = std::env::temp_dir().join(format!("flekks-med-agent-test-{}.db", new_id()));
        let store = LocalStore::open_encrypted(&path, "test-key").unwrap();
        (store, path)
    }

    fn cleanup(path: PathBuf) {
        let _ = std::fs::remove_file(path);
    }

    fn insert_patient_and_encounter(store: &LocalStore) -> (PatientId, med_core::EncounterId) {
        let now = OffsetDateTime::now_utc();
        let patient = Patient {
            id: new_id(),
            medical_record_number: Some("MRN-SYNTH-AGENT".to_owned()),
            display_name: "Synthetic Agent Patient".to_owned(),
            date_of_birth: Some(Date::from_calendar_date(1978, time::Month::March, 4).unwrap()),
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
            reason: Some("Synthetic agent test".to_owned()),
        };

        store.insert_patient(&patient).unwrap();
        store.insert_encounter(&encounter).unwrap();

        (patient.id, encounter.id)
    }

    fn insert_note_draft(
        store: &LocalStore,
        patient_id: PatientId,
        encounter_id: med_core::EncounterId,
    ) -> NoteId {
        let now = OffsetDateTime::now_utc();
        let note = ClinicalNote {
            id: new_id(),
            patient_id,
            encounter_id,
            author_id: None,
            template: NoteTemplate::Soap,
            status: NoteStatus::Draft,
            sections: vec![NoteSection {
                heading: "Subjective".to_owned(),
                body: "Synthetic note draft".to_owned(),
                required: true,
            }],
            created_at: now,
            updated_at: now,
            signed_at: None,
            version: 1,
        };
        let note_id = note.id;

        store.upsert_note(&note).unwrap();

        note_id
    }

    #[test]
    fn read_patient_summary_writes_audit_event() {
        let (store, path) = temp_store();
        let (patient_id, _) = insert_patient_and_encounter(&store);
        let registry = MedicalToolRuntimeRegistry::default();

        let before = store.audit_event_count().unwrap();
        let output = registry
            .dispatch(MedicalToolInvocation {
                store: &store,
                call_id: "call-read-summary".to_owned(),
                tool_name: MedicalToolName::ReadPatientSummary,
                payload: MedicalToolPayload::ReadPatientSummary(ReadPatientSummaryRequest {
                    patient_id,
                }),
                context: MedicalToolContext::default(),
                approval_policy: MedicalApprovalPolicy::local_default(),
            })
            .unwrap();

        assert!(output.success);
        assert_eq!(output.structured["encounter_count"], 1);
        assert_eq!(store.audit_event_count().unwrap(), before + 1);

        drop(store);
        cleanup(path);
    }

    #[test]
    fn save_note_draft_persists_note_and_audit_event() {
        let (store, path) = temp_store();
        let (patient_id, encounter_id) = insert_patient_and_encounter(&store);
        let registry = MedicalToolRuntimeRegistry::default();

        let before = store.audit_event_count().unwrap();
        let output = registry
            .dispatch(MedicalToolInvocation {
                store: &store,
                call_id: "call-save-note".to_owned(),
                tool_name: MedicalToolName::CreateNoteDraft,
                payload: MedicalToolPayload::SaveNoteDraft(SaveNoteDraftRequest {
                    note_id: None,
                    patient_id,
                    encounter_id,
                    template: NoteTemplate::Soap,
                    sections: vec![
                        NoteSection {
                            heading: "Subjective".to_owned(),
                            body: "Synthetic subjective text".to_owned(),
                            required: true,
                        },
                        NoteSection {
                            heading: "Assessment".to_owned(),
                            body: "Synthetic assessment text".to_owned(),
                            required: true,
                        },
                    ],
                }),
                context: MedicalToolContext::default(),
                approval_policy: MedicalApprovalPolicy::local_default(),
            })
            .unwrap();

        let note_id: NoteId = serde_json::from_value(output.structured["note_id"].clone()).unwrap();
        let note = store.get_note(note_id).unwrap().unwrap();

        assert!(matches!(note.status, NoteStatus::Draft));
        assert_eq!(note.sections.len(), 2);
        assert_eq!(note.sections[0].heading, "Subjective");
        assert_eq!(store.audit_event_count().unwrap(), before + 1);

        drop(store);
        cleanup(path);
    }

    #[test]
    fn read_only_policy_blocks_note_draft_write() {
        let (store, path) = temp_store();
        let (patient_id, encounter_id) = insert_patient_and_encounter(&store);
        let registry = MedicalToolRuntimeRegistry::default();

        let result = registry.dispatch(MedicalToolInvocation {
            store: &store,
            call_id: "call-blocked-note".to_owned(),
            tool_name: MedicalToolName::UpdateNoteDraft,
            payload: MedicalToolPayload::SaveNoteDraft(SaveNoteDraftRequest {
                note_id: None,
                patient_id,
                encounter_id,
                template: NoteTemplate::Soap,
                sections: vec![NoteSection {
                    heading: "Plan".to_owned(),
                    body: "Synthetic plan".to_owned(),
                    required: true,
                }],
            }),
            context: MedicalToolContext::default(),
            approval_policy: MedicalApprovalPolicy::read_only(),
        });

        assert!(matches!(
            result,
            Err(MedicalToolError::PolicyBlocked(
                MedicalToolName::UpdateNoteDraft
            ))
        ));

        drop(store);
        cleanup(path);
    }

    #[test]
    fn documentation_audit_tool_returns_flags_and_writes_audit_event() {
        let (store, path) = temp_store();
        let (patient_id, encounter_id) = insert_patient_and_encounter(&store);
        let note_id = insert_note_draft(&store, patient_id, encounter_id);
        let registry = MedicalToolRuntimeRegistry::default();

        let before = store.audit_event_count().unwrap();
        let output = registry
            .dispatch(MedicalToolInvocation {
                store: &store,
                call_id: "call-documentation-audit".to_owned(),
                tool_name: MedicalToolName::RunDocumentationAudit,
                payload: MedicalToolPayload::RunDocumentationAudit(RunDocumentationAuditRequest {
                    patient_id,
                    encounter_id: Some(encounter_id),
                    note_id: Some(note_id),
                }),
                context: MedicalToolContext::default(),
                approval_policy: MedicalApprovalPolicy::local_default(),
            })
            .unwrap();
        let report: med_core::DocumentationAuditReport =
            serde_json::from_value(output.structured).unwrap();

        assert!(output.success);
        assert_eq!(report.billing_ready_percent, 25);
        assert_eq!(report.note_id, Some(note_id));
        assert!(report
            .flags
            .iter()
            .any(|flag| flag.code == "unsigned_draft"));
        assert!(report
            .flags
            .iter()
            .any(|flag| flag.code == "missing_section_objective"));
        assert_eq!(store.audit_event_count().unwrap(), before + 1);

        drop(store);
        cleanup(path);
    }

    #[test]
    fn default_policy_requires_human_approval_for_note_signing() {
        let (store, path) = temp_store();
        let (patient_id, encounter_id) = insert_patient_and_encounter(&store);
        let note_id = insert_note_draft(&store, patient_id, encounter_id);
        let registry = MedicalToolRuntimeRegistry::default();

        let result = registry.dispatch(MedicalToolInvocation {
            store: &store,
            call_id: "call-sign-note-blocked".to_owned(),
            tool_name: MedicalToolName::SignNote,
            payload: MedicalToolPayload::SignNote(SignNoteRequest {
                note_id,
                patient_id,
                encounter_id,
            }),
            context: MedicalToolContext::default(),
            approval_policy: MedicalApprovalPolicy::local_default(),
        });

        assert!(matches!(
            result,
            Err(MedicalToolError::ApprovalRequired(
                MedicalToolName::SignNote
            ))
        ));

        drop(store);
        cleanup(path);
    }

    #[test]
    fn sign_note_tool_marks_note_signed_and_writes_audit_event() {
        let (store, path) = temp_store();
        let (patient_id, encounter_id) = insert_patient_and_encounter(&store);
        let note_id = insert_note_draft(&store, patient_id, encounter_id);
        let registry = MedicalToolRuntimeRegistry::default();

        let before = store.audit_event_count().unwrap();
        let output = registry
            .dispatch(MedicalToolInvocation {
                store: &store,
                call_id: "call-sign-note".to_owned(),
                tool_name: MedicalToolName::SignNote,
                payload: MedicalToolPayload::SignNote(SignNoteRequest {
                    note_id,
                    patient_id,
                    encounter_id,
                }),
                context: MedicalToolContext::default(),
                approval_policy: MedicalApprovalPolicy::after_human_confirmation(),
            })
            .unwrap();
        let note = store.get_note(note_id).unwrap().unwrap();

        assert!(output.success);
        assert_eq!(output.structured["status"], "Signed");
        assert!(matches!(note.status, NoteStatus::Signed));
        assert!(note.signed_at.is_some());
        assert_eq!(store.audit_event_count().unwrap(), before + 1);

        drop(store);
        cleanup(path);
    }
}
